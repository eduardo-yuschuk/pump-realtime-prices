//! PostgreSQL and ClickHouse persistence for parsed token swap events.

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use clickhouse::Row;
use common::{ParsedEvent, TokenSwap};
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use parser::{BlockEvents, InstructionEvents, TransactionEvents};
use serde::Serialize;
use serde_repr::Serialize_repr;
use tokio_postgres::{config::SslMode, Client, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;

const WRAPPED_SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const WRAPPED_SOL_DECIMALS: u8 = 9;
const DECIMAL_SCALE: u32 = 18;

const UPSERT_TOKEN_PAIR_PRICE: &str = "
    INSERT INTO token_pair_prices (
        liquidity_provider_address,
        liquidity_provider_kind,
        base_token_address,
        quote_token_address,
        base_amount,
        quote_amount,
        base_token_decimals,
        quote_token_decimals
    )
    SELECT
        liquidity_provider_address,
        liquidity_provider_kind,
        base_token_address,
        quote_token_address,
        base_amount::NUMERIC,
        quote_amount::NUMERIC,
        base_token_decimals,
        quote_token_decimals
    FROM UNNEST(
        $1::TEXT[],
        $2::TEXT[],
        $3::TEXT[],
        $4::TEXT[],
        $5::TEXT[],
        $6::TEXT[],
        $7::SMALLINT[],
        $8::SMALLINT[]
    ) AS batch(
        liquidity_provider_address,
        liquidity_provider_kind,
        base_token_address,
        quote_token_address,
        base_amount,
        quote_amount,
        base_token_decimals,
        quote_token_decimals
    )
    ON CONFLICT (liquidity_provider_address) DO UPDATE SET
        liquidity_provider_kind = EXCLUDED.liquidity_provider_kind,
        base_token_address = EXCLUDED.base_token_address,
        quote_token_address = EXCLUDED.quote_token_address,
        base_amount = EXCLUDED.base_amount,
        quote_amount = EXCLUDED.quote_amount,
        base_token_decimals = EXCLUDED.base_token_decimals,
        quote_token_decimals = EXCLUDED.quote_token_decimals
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub database: String,
    pub password: String,
    pub disable_ssl: bool,
    pub clickhouse_url: String,
    pub clickhouse_username: String,
    pub clickhouse_database: String,
    pub clickhouse_password: String,
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self, SaverError> {
        let port = required_env("DB_PORT")?
            .parse()
            .map_err(|_| SaverError::InvalidPort)?;
        let disable_ssl = required_env("DB_DISABLE_SSL")?
            .parse()
            .map_err(|_| SaverError::InvalidDisableSsl)?;

        Ok(Self {
            host: required_env("DB_HOST")?,
            port,
            username: required_env("DB_USERNAME")?,
            database: required_env("DB_NAME")?,
            password: required_env("DB_PASSWORD")?,
            disable_ssl,
            clickhouse_url: required_env("CLICKHOUSE_URL")?,
            clickhouse_username: required_env("CLICKHOUSE_USERNAME")?,
            clickhouse_database: required_env("CLICKHOUSE_DATABASE")?,
            clickhouse_password: required_env("CLICKHOUSE_PASSWORD")?,
        })
    }
}

#[derive(Debug)]
pub enum SaverError {
    MissingEnvironmentVariable(&'static str),
    NonUnicodeEnvironmentVariable(&'static str),
    InvalidPort,
    InvalidDisableSsl,
    Database(tokio_postgres::Error),
    ClickHouse(clickhouse::error::Error),
    SystemClockBeforeUnixEpoch,
    DecimalOverflow(&'static str),
    ZeroBaseAmount,
    UnsupportedTokenSwapProgram(String),
    MissingTokenDecimals(String),
}

impl fmt::Display for SaverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEnvironmentVariable(variable) => {
                write!(formatter, "{variable} is not configured")
            }
            Self::NonUnicodeEnvironmentVariable(variable) => {
                write!(formatter, "{variable} is not valid Unicode")
            }
            Self::InvalidPort => formatter.write_str("DB_PORT must be a valid u16"),
            Self::InvalidDisableSsl => formatter.write_str("DB_DISABLE_SSL must be true or false"),
            Self::Database(error) => write!(formatter, "database operation failed: {error}"),
            Self::ClickHouse(error) => write!(formatter, "ClickHouse operation failed: {error}"),
            Self::SystemClockBeforeUnixEpoch => {
                formatter.write_str("system clock is before the Unix epoch")
            }
            Self::DecimalOverflow(column) => {
                write!(
                    formatter,
                    "{column} does not fit ClickHouse Decimal(38, 18)"
                )
            }
            Self::ZeroBaseAmount => {
                formatter.write_str("cannot calculate a price with zero base amount")
            }
            Self::UnsupportedTokenSwapProgram(program_id) => {
                write!(
                    formatter,
                    "cannot save TokenSwap from unsupported program {program_id}"
                )
            }
            Self::MissingTokenDecimals(mint) => {
                write!(
                    formatter,
                    "transaction token balances do not provide decimals for {mint}"
                )
            }
        }
    }
}

impl Error for SaverError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::ClickHouse(error) => Some(error),
            _ => None,
        }
    }
}

pub struct Saver {
    client: Client,
    clickhouse_client: clickhouse::Client,
}

impl Saver {
    pub async fn from_env() -> Result<Self, SaverError> {
        Self::connect(DatabaseConfig::from_env()?).await
    }

    pub async fn connect(config: DatabaseConfig) -> Result<Self, SaverError> {
        let mut postgres_config = tokio_postgres::Config::new();
        postgres_config
            .host(&config.host)
            .port(config.port)
            .user(&config.username)
            .password(&config.password)
            .dbname(&config.database);

        let client = if config.disable_ssl {
            let (client, connection) = postgres_config
                .ssl_mode(SslMode::Disable)
                .connect(NoTls)
                .await
                .map_err(SaverError::Database)?;
            tokio::spawn(async move {
                if let Err(error) = connection.await {
                    eprintln!("PostgreSQL connection ended: {error}");
                }
            });
            client
        } else {
            let root_certificates =
                rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let tls_config = rustls::ClientConfig::builder()
                .with_root_certificates(root_certificates)
                .with_no_client_auth();
            let (client, connection) = postgres_config
                .ssl_mode(SslMode::Require)
                .connect(MakeRustlsConnect::new(tls_config))
                .await
                .map_err(SaverError::Database)?;
            tokio::spawn(async move {
                if let Err(error) = connection.await {
                    eprintln!("PostgreSQL connection ended: {error}");
                }
            });
            client
        };

        let clickhouse_client = clickhouse::Client::default()
            .with_url(config.clickhouse_url)
            .with_user(config.clickhouse_username)
            .with_database(config.clickhouse_database)
            .with_password(config.clickhouse_password)
            .with_validation(false);

        Ok(Self {
            client,
            clickhouse_client,
        })
    }

    pub async fn save_block_events(&mut self, events: &BlockEvents) -> Result<usize, SaverError> {
        let prices = collect_token_pair_prices(events)?;
        if prices.is_empty() {
            return Ok(0);
        }

        let latest_prices = deduplicate_token_pair_prices(prices.clone());
        let liquidity_provider_addresses: Vec<_> = latest_prices
            .iter()
            .map(|price| price.liquidity_provider_address.as_str())
            .collect();
        let liquidity_provider_kinds: Vec<_> = latest_prices
            .iter()
            .map(|price| price.liquidity_provider_kind.as_str())
            .collect();
        let base_token_addresses: Vec<_> = latest_prices
            .iter()
            .map(|price| price.base_token_address.as_str())
            .collect();
        let quote_token_addresses: Vec<_> = latest_prices
            .iter()
            .map(|price| price.quote_token_address.as_str())
            .collect();
        let base_amounts: Vec<_> = latest_prices
            .iter()
            .map(|price| price.base_amount.to_string())
            .collect();
        let quote_amounts: Vec<_> = latest_prices
            .iter()
            .map(|price| price.quote_amount.to_string())
            .collect();
        let base_token_decimals: Vec<_> = latest_prices
            .iter()
            .map(|price| price.base_token_decimals as i16)
            .collect();
        let quote_token_decimals: Vec<_> = latest_prices
            .iter()
            .map(|price| price.quote_token_decimals as i16)
            .collect();

        // println!("Saving token pair price batch: {prices:?}");
        self.client
            .execute(
                UPSERT_TOKEN_PAIR_PRICE,
                &[
                    &liquidity_provider_addresses,
                    &liquidity_provider_kinds,
                    &base_token_addresses,
                    &quote_token_addresses,
                    &base_amounts,
                    &quote_amounts,
                    &base_token_decimals,
                    &quote_token_decimals,
                ],
            )
            .await
            .map_err(SaverError::Database)?;

        let time = current_time_millis()?;
        self.save_latest_token_pair_prices(&latest_prices, time)
            .await?;
        self.save_real_time_prices(&prices, time).await?;

        Ok(prices.len())
    }

    async fn save_latest_token_pair_prices(
        &self,
        prices: &[TokenPairPrice],
        updated_at: i64,
    ) -> Result<(), SaverError> {
        let mut insert = self
            .clickhouse_client
            .insert::<LatestTokenPairPriceRow<'_>>("token_pair_prices")
            .await
            .map_err(SaverError::ClickHouse)?;

        for price in prices {
            let row = LatestTokenPairPriceRow {
                liquidity_provider_address: &price.liquidity_provider_address,
                liquidity_provider_kind: price.liquidity_provider_kind,
                base_token_address: &price.base_token_address,
                quote_token_address: &price.quote_token_address,
                base_amount: price.base_amount,
                quote_amount: price.quote_amount,
                base_token_decimals: price.base_token_decimals,
                quote_token_decimals: price.quote_token_decimals,
                price: normalized_price(price)?,
                updated_at,
            };
            insert.write(&row).await.map_err(SaverError::ClickHouse)?;
        }
        insert.end().await.map_err(SaverError::ClickHouse)
    }

    async fn save_real_time_prices(
        &self,
        prices: &[TokenPairPrice],
        time: i64,
    ) -> Result<(), SaverError> {
        let mut insert = self
            .clickhouse_client
            .insert::<RealTimePriceRow<'_>>("real_time_prices")
            .await
            .map_err(SaverError::ClickHouse)?;

        for price in prices {
            let row = RealTimePriceRow {
                liquidity_provider_address: &price.liquidity_provider_address,
                liquidity_provider_kind: price.liquidity_provider_kind,
                base_token_address: &price.base_token_address,
                quote_token_address: &price.quote_token_address,
                price: normalized_price(price)?,
                volume: normalized_volume(price)?,
                time,
                sequence: price.sequence,
            };
            insert.write(&row).await.map_err(SaverError::ClickHouse)?;
        }
        insert.end().await.map_err(SaverError::ClickHouse)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TokenPairPrice {
    liquidity_provider_address: String,
    liquidity_provider_kind: LiquidityProviderKind,
    base_token_address: String,
    quote_token_address: String,
    base_amount: u64,
    quote_amount: u64,
    base_token_decimals: u8,
    quote_token_decimals: u8,
    sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize_repr)]
#[repr(i8)]
enum LiquidityProviderKind {
    Amm = 1,
    BondingCurve = 2,
}

impl LiquidityProviderKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Amm => "amm",
            Self::BondingCurve => "bonding_curve",
        }
    }
}

#[derive(Row, Serialize)]
struct RealTimePriceRow<'a> {
    liquidity_provider_address: &'a str,
    liquidity_provider_kind: LiquidityProviderKind,
    base_token_address: &'a str,
    quote_token_address: &'a str,
    price: i128,
    volume: i128,
    time: i64,
    sequence: u64,
}

#[derive(Row, Serialize)]
struct LatestTokenPairPriceRow<'a> {
    liquidity_provider_address: &'a str,
    liquidity_provider_kind: LiquidityProviderKind,
    base_token_address: &'a str,
    quote_token_address: &'a str,
    base_amount: u64,
    quote_amount: u64,
    base_token_decimals: u8,
    quote_token_decimals: u8,
    price: i128,
    updated_at: i64,
}

fn collect_token_pair_prices(events: &BlockEvents) -> Result<Vec<TokenPairPrice>, SaverError> {
    let mut prices = Vec::new();

    for transaction in &events.transactions {
        for instruction in &transaction.instructions {
            let Ok(ParsedEvent::TokenSwap(swap)) = &instruction.result else {
                continue;
            };
            prices.push(token_pair_price(transaction, instruction, swap)?);
        }
    }

    Ok(prices)
}

fn deduplicate_token_pair_prices(prices: Vec<TokenPairPrice>) -> Vec<TokenPairPrice> {
    prices
        .into_iter()
        .map(|price| (price.liquidity_provider_address.clone(), price))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

fn token_pair_price(
    transaction: &TransactionEvents,
    instruction: &InstructionEvents,
    swap: &TokenSwap,
) -> Result<TokenPairPrice, SaverError> {
    let program_id = instruction
        .program_id
        .as_deref()
        .ok_or_else(|| SaverError::UnsupportedTokenSwapProgram("unknown".to_owned()))?;
    let liquidity_provider_kind = match program_id {
        pumpfun::PROGRAM_ID => LiquidityProviderKind::BondingCurve,
        pumpswap::PROGRAM_ID => LiquidityProviderKind::Amm,
        _ => {
            return Err(SaverError::UnsupportedTokenSwapProgram(
                program_id.to_owned(),
            ))
        }
    };

    Ok(TokenPairPrice {
        liquidity_provider_address: swap.pool.clone(),
        liquidity_provider_kind,
        base_token_address: swap.input_mint.clone(),
        quote_token_address: swap.output_mint.clone(),
        base_amount: swap.input_amount,
        quote_amount: swap.output_amount,
        base_token_decimals: token_decimals(&transaction.token_decimals, &swap.input_mint)?,
        quote_token_decimals: token_decimals(&transaction.token_decimals, &swap.output_mint)?,
        sequence: ((transaction.transaction_index as u64) << 32)
            | instruction.execution_ordinal as u64,
    })
}

fn normalized_price(price: &TokenPairPrice) -> Result<i128, SaverError> {
    if price.base_amount == 0 {
        return Err(SaverError::ZeroBaseAmount);
    }

    let numerator = BigUint::from(price.quote_amount)
        * power_of_ten(u32::from(price.base_token_decimals) + DECIMAL_SCALE);
    let denominator =
        BigUint::from(price.base_amount) * power_of_ten(u32::from(price.quote_token_decimals));
    decimal_i128(numerator / denominator, "price")
}

fn normalized_volume(price: &TokenPairPrice) -> Result<i128, SaverError> {
    let value = BigUint::from(price.base_amount) * power_of_ten(DECIMAL_SCALE)
        / power_of_ten(u32::from(price.base_token_decimals));
    decimal_i128(value, "volume")
}

fn power_of_ten(exponent: u32) -> BigUint {
    BigUint::from(10_u8).pow(exponent)
}

fn decimal_i128(value: BigUint, column: &'static str) -> Result<i128, SaverError> {
    value.to_i128().ok_or(SaverError::DecimalOverflow(column))
}

fn current_time_millis() -> Result<i64, SaverError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SaverError::SystemClockBeforeUnixEpoch)?
        .as_millis()
        .try_into()
        .map_err(|_| SaverError::DecimalOverflow("time"))
}

fn token_decimals(token_decimals: &BTreeMap<String, u8>, mint: &str) -> Result<u8, SaverError> {
    token_decimals
        .get(mint)
        .copied()
        .or((mint == WRAPPED_SOL_MINT).then_some(WRAPPED_SOL_DECIMALS))
        .ok_or_else(|| SaverError::MissingTokenDecimals(mint.to_owned()))
}

fn required_env(variable: &'static str) -> Result<String, SaverError> {
    env::var(variable).map_err(|error| match error {
        env::VarError::NotPresent => SaverError::MissingEnvironmentVariable(variable),
        env::VarError::NotUnicode(_) => SaverError::NonUnicodeEnvironmentVariable(variable),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use common::TokenSwap;
    use parser::{InstructionEvents, TransactionEvents};

    use super::*;

    fn transaction(token_decimals: BTreeMap<String, u8>) -> TransactionEvents {
        TransactionEvents {
            signature: "signature".to_owned(),
            transaction_index: 0,
            token_decimals,
            instructions: vec![],
        }
    }

    fn instruction(program_id: &str) -> InstructionEvents {
        InstructionEvents {
            program_id: Some(program_id.to_owned()),
            outer_instruction_index: 0,
            inner_instruction_index: None,
            stack_height: None,
            execution_ordinal: 0,
            result: Ok(ParsedEvent::TokenDiscovery(common::TokenDiscovery {
                mint: "mint".to_owned(),
                creator: "creator".to_owned(),
                name: "name".to_owned(),
                symbol: "symbol".to_owned(),
                uri: "uri".to_owned(),
            })),
        }
    }

    fn swap() -> TokenSwap {
        TokenSwap {
            user: "user".to_owned(),
            pool: "pool".to_owned(),
            input_mint: "input-mint".to_owned(),
            input_amount: 123,
            output_mint: "output-mint".to_owned(),
            output_amount: 456,
        }
    }

    fn price(pool: &str, base_amount: u64) -> TokenPairPrice {
        TokenPairPrice {
            liquidity_provider_address: pool.to_owned(),
            liquidity_provider_kind: LiquidityProviderKind::Amm,
            base_token_address: "base-mint".to_owned(),
            quote_token_address: "quote-mint".to_owned(),
            base_amount,
            quote_amount: 456,
            base_token_decimals: 6,
            quote_token_decimals: 9,
            sequence: 0,
        }
    }

    #[test]
    fn maps_pumpfun_swaps_to_bonding_curve_prices() {
        let transaction = transaction(BTreeMap::from([
            ("input-mint".to_owned(), 9),
            ("output-mint".to_owned(), 6),
        ]));
        let instruction = instruction(pumpfun::PROGRAM_ID);

        let swap = swap();
        let price = token_pair_price(&transaction, &instruction, &swap).unwrap();

        assert_eq!(
            price.liquidity_provider_kind,
            LiquidityProviderKind::BondingCurve
        );
        assert_eq!(price.base_token_address, "input-mint");
        assert_eq!(price.quote_token_address, "output-mint");
        assert_eq!(price.base_token_decimals, 9);
        assert_eq!(price.quote_token_decimals, 6);
    }

    #[test]
    fn maps_pumpswap_swaps_to_amm_prices() {
        let transaction = transaction(BTreeMap::from([
            ("input-mint".to_owned(), 6),
            ("output-mint".to_owned(), 9),
        ]));
        let instruction = instruction(pumpswap::PROGRAM_ID);

        assert_eq!(
            token_pair_price(&transaction, &instruction, &swap())
                .unwrap()
                .liquidity_provider_kind,
            LiquidityProviderKind::Amm
        );
    }

    #[test]
    fn uses_the_known_wrapped_sol_decimals_when_balances_omit_it() {
        let transaction = transaction(BTreeMap::new());

        assert_eq!(
            token_decimals(&transaction.token_decimals, WRAPPED_SOL_MINT).unwrap(),
            9
        );
    }

    #[test]
    fn rejects_swaps_without_token_decimals() {
        let transaction = transaction(BTreeMap::new());
        let instruction = instruction(pumpfun::PROGRAM_ID);

        assert!(matches!(
            token_pair_price(&transaction, &instruction, &swap()),
            Err(SaverError::MissingTokenDecimals(mint)) if mint == "input-mint"
        ));
    }

    #[test]
    fn keeps_the_last_price_for_each_liquidity_provider_in_a_batch() {
        let prices = deduplicate_token_pair_prices(vec![
            price("first-pool", 1),
            price("second-pool", 2),
            price("first-pool", 3),
        ]);

        assert_eq!(
            prices,
            vec![price("first-pool", 3), price("second-pool", 2)]
        );
    }

    #[test]
    fn keeps_all_prices_for_clickhouse_before_deduplicating_postgresql_prices() {
        let mut first_instruction = instruction(pumpfun::PROGRAM_ID);
        first_instruction.result = Ok(ParsedEvent::TokenSwap(swap()));
        let mut second_instruction = instruction(pumpfun::PROGRAM_ID);
        second_instruction.execution_ordinal = 1;
        let mut second_swap = swap();
        second_swap.input_amount = 789;
        second_instruction.result = Ok(ParsedEvent::TokenSwap(second_swap));
        let events = BlockEvents {
            transactions: vec![TransactionEvents {
                instructions: vec![first_instruction, second_instruction],
                ..transaction(BTreeMap::from([
                    ("input-mint".to_owned(), 6),
                    ("output-mint".to_owned(), 9),
                ]))
            }],
        };

        let prices = collect_token_pair_prices(&events).unwrap();

        assert_eq!(prices.len(), 2);
        assert_eq!(prices[0].sequence, 0);
        assert_eq!(prices[1].sequence, 1);
        let latest_prices = deduplicate_token_pair_prices(prices);
        assert_eq!(latest_prices.len(), 1);
        assert_eq!(latest_prices[0].base_amount, 789);
        assert_eq!(latest_prices[0].sequence, 1);
    }

    #[test]
    fn normalizes_price_and_volume_for_clickhouse_decimal_columns() {
        let price = TokenPairPrice {
            base_amount: 200,
            quote_amount: 300,
            base_token_decimals: 2,
            quote_token_decimals: 3,
            ..price("pool", 200)
        };

        assert_eq!(normalized_price(&price).unwrap(), 150_000_000_000_000_000);
        assert_eq!(
            normalized_volume(&price).unwrap(),
            2_000_000_000_000_000_000
        );
    }

    #[test]
    fn rejects_zero_base_amount_for_clickhouse_prices() {
        assert!(matches!(
            normalized_price(&price("pool", 0)),
            Err(SaverError::ZeroBaseAmount)
        ));
    }

    #[test]
    fn uses_unnest_for_the_batch_upsert() {
        assert!(UPSERT_TOKEN_PAIR_PRICE.contains("FROM UNNEST("));
        assert!(UPSERT_TOKEN_PAIR_PRICE.contains("$5::TEXT[]"));
        assert!(UPSERT_TOKEN_PAIR_PRICE.contains("$6::TEXT[]"));
    }
}
