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
    ) VALUES ($1, $2, $3, $4, $5::NUMERIC, $6::NUMERIC, $7, $8)
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
        let transaction = self
            .client
            .transaction()
            .await
            .map_err(SaverError::Database)?;
        let mut saved = 0;

        for transaction_events in &events.transactions {
            for instruction in &transaction_events.instructions {
                let Ok(ParsedEvent::TokenSwap(swap)) = &instruction.result else {
                    continue;
                };
                let price = token_pair_price(transaction_events, instruction, swap)?;
                transaction
                    .execute(
                        UPSERT_TOKEN_PAIR_PRICE,
                        &[
                            &price.liquidity_provider_address,
                            &price.liquidity_provider_kind,
                            &price.base_token_address,
                            &price.quote_token_address,
                            &price.base_amount.to_string(),
                            &price.quote_amount.to_string(),
                            &(price.base_token_decimals as i16),
                            &(price.quote_token_decimals as i16),
                        ],
                    )
                    .await
                    .map_err(SaverError::Database)?;
                saved += 1;
            }
        }
        transaction.commit().await.map_err(SaverError::Database)?;

        Ok(saved)
    }
}

struct TokenPairPrice<'a> {
    liquidity_provider_address: &'a str,
    liquidity_provider_kind: &'static str,
    base_token_address: &'a str,
    quote_token_address: &'a str,
    base_amount: u64,
    quote_amount: u64,
    base_token_decimals: u8,
    quote_token_decimals: u8,
}

fn token_pair_price<'a>(
    transaction: &'a TransactionEvents,
    instruction: &'a InstructionEvents,
    swap: &'a TokenSwap,
) -> Result<TokenPairPrice<'a>, SaverError> {
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
        liquidity_provider_address: &swap.pool,
        liquidity_provider_kind,
        base_token_address: &swap.input_mint,
        quote_token_address: &swap.output_mint,
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
}
