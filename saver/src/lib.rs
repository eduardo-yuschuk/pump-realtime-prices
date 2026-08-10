//! PostgreSQL persistence for parsed token swap events.

use std::{collections::BTreeMap, env, error::Error, fmt};

use common::{ParsedEvent, TokenSwap};
use parser::{BlockEvents, InstructionEvents, TransactionEvents};
use tokio_postgres::{config::SslMode, Client, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;

const WRAPPED_SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const WRAPPED_SOL_DECIMALS: u8 = 9;

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
            _ => None,
        }
    }
}

pub struct Saver {
    client: Client,
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

        Ok(Self { client })
    }

    pub async fn save_block_events(&mut self, events: &BlockEvents) -> Result<usize, SaverError> {
        let prices = collect_token_pair_prices(events)?;
        if prices.is_empty() {
            return Ok(0);
        }

        let liquidity_provider_addresses: Vec<_> = prices
            .iter()
            .map(|price| price.liquidity_provider_address.as_str())
            .collect();
        let liquidity_provider_kinds: Vec<_> = prices
            .iter()
            .map(|price| price.liquidity_provider_kind)
            .collect();
        let base_token_addresses: Vec<_> = prices
            .iter()
            .map(|price| price.base_token_address.as_str())
            .collect();
        let quote_token_addresses: Vec<_> = prices
            .iter()
            .map(|price| price.quote_token_address.as_str())
            .collect();
        let base_amounts: Vec<_> = prices
            .iter()
            .map(|price| price.base_amount.to_string())
            .collect();
        let quote_amounts: Vec<_> = prices
            .iter()
            .map(|price| price.quote_amount.to_string())
            .collect();
        let base_token_decimals: Vec<_> = prices
            .iter()
            .map(|price| price.base_token_decimals as i16)
            .collect();
        let quote_token_decimals: Vec<_> = prices
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

        Ok(prices.len())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TokenPairPrice {
    liquidity_provider_address: String,
    liquidity_provider_kind: &'static str,
    base_token_address: String,
    quote_token_address: String,
    base_amount: u64,
    quote_amount: u64,
    base_token_decimals: u8,
    quote_token_decimals: u8,
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

    Ok(deduplicate_token_pair_prices(prices))
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
        pumpfun::PROGRAM_ID => "bonding_curve",
        pumpswap::PROGRAM_ID => "amm",
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
    })
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
            liquidity_provider_kind: "amm",
            base_token_address: "base-mint".to_owned(),
            quote_token_address: "quote-mint".to_owned(),
            base_amount,
            quote_amount: 456,
            base_token_decimals: 6,
            quote_token_decimals: 9,
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

        assert_eq!(price.liquidity_provider_kind, "bonding_curve");
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
            "amm"
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
    fn uses_unnest_for_the_batch_upsert() {
        assert!(UPSERT_TOKEN_PAIR_PRICE.contains("FROM UNNEST("));
        assert!(UPSERT_TOKEN_PAIR_PRICE.contains("$5::TEXT[]"));
        assert!(UPSERT_TOKEN_PAIR_PRICE.contains("$6::TEXT[]"));
    }
}
