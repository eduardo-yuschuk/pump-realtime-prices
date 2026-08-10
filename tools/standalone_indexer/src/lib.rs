//! WebSocket adapter that parses finalized Solana blocks as they arrive.

use std::{
    env,
    error::Error,
    fmt, io,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use parser::{BlockEvents, BlockParseError, BlockParser, ParserInitError};
use saver::{Saver, SaverError};
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum IndexerError {
    Dotenv(String),
    MissingWebSocketUrl,
    NonUnicodeWebSocketUrl,
    Parser(ParserInitError),
    Saver(SaverError),
    TlsProvider,
}

impl fmt::Display for IndexerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dotenv(error) => write!(formatter, "failed to load .env: {error}"),
            Self::MissingWebSocketUrl => formatter.write_str("SOLANA_WS_URL is not configured"),
            Self::NonUnicodeWebSocketUrl => {
                formatter.write_str("SOLANA_WS_URL is not valid Unicode")
            }
            Self::Parser(error) => write!(formatter, "failed to initialize block parser: {error}"),
            Self::Saver(error) => write!(formatter, "failed to initialize saver: {error}"),
            Self::TlsProvider => {
                formatter.write_str("failed to configure the Rustls crypto provider")
            }
        }
    }
}

impl Error for IndexerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parser(error) => Some(error),
            Self::Saver(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum NotificationError {
    InvalidField(&'static str),
    BlockUnavailable,
}

impl fmt::Display for NotificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => {
                write!(formatter, "block notification has no valid {field}")
            }
            Self::BlockUnavailable => {
                formatter.write_str("block notification does not contain a block")
            }
        }
    }
}

impl Error for NotificationError {}

#[derive(Debug)]
pub enum ProcessingError {
    Notification(NotificationError),
    Parse(BlockParseError),
    Save(SaverError),
}

impl fmt::Display for ProcessingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Notification(error) => write!(formatter, "invalid block notification: {error}"),
            Self::Parse(error) => write!(formatter, "failed to parse block: {error}"),
            Self::Save(error) => write!(formatter, "failed to save block events: {error}"),
        }
    }
}

impl Error for ProcessingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Notification(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Save(error) => Some(error),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BlockSummary {
    pub slot: u64,
    pub block_transactions: usize,
    pub parsed_transactions: usize,
    pub results: usize,
    pub events: usize,
    pub failures: usize,
    pub parse_duration: Duration,
    pub database_write_duration: Duration,
}

impl fmt::Display for BlockSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Block slot={}: transactions={} parsed_transactions={} results={} events={} failures={} parse_time_ms={:.3} database_write_time_ms={:.3}",
            self.slot,
            self.block_transactions,
            self.parsed_transactions,
            self.results,
            self.events,
            self.failures,
            self.parse_duration.as_secs_f64() * 1_000.0,
            self.database_write_duration.as_secs_f64() * 1_000.0
        )
    }
}

struct BlockNotification<'a> {
    slot: u64,
    block: &'a Value,
}

pub async fn run() -> Result<(), IndexerError> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| IndexerError::TlsProvider)?;
    load_dotenv()?;
    let websocket_url = env::var("SOLANA_WS_URL").map_err(|error| match error {
        env::VarError::NotPresent => IndexerError::MissingWebSocketUrl,
        env::VarError::NotUnicode(_) => IndexerError::NonUnicodeWebSocketUrl,
    })?;
    let parser = BlockParser::from_env().map_err(IndexerError::Parser)?;
    let mut saver = Saver::from_env().await.map_err(IndexerError::Saver)?;

    loop {
        if let Err(error) = receive_blocks(&websocket_url, &parser, &mut saver).await {
            eprintln!("WebSocket session ended: {error}; reconnecting in 5 seconds");
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

pub fn block_subscribe_request() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "blockSubscribe",
        "params": [
            "all",
            {
                "commitment": "finalized",
                "encoding": "json",
                "transactionDetails": "full",
                "maxSupportedTransactionVersion": 0,
                "showRewards": false
            }
        ]
    })
}

pub fn process_notification(
    message: &Value,
    parser: &BlockParser,
) -> Result<Option<BlockSummary>, ProcessingError> {
    Ok(process_notification_events(message, parser)?.map(|(summary, _)| summary))
}

fn process_notification_events(
    message: &Value,
    parser: &BlockParser,
) -> Result<Option<(BlockSummary, BlockEvents)>, ProcessingError> {
    let Some(notification) =
        extract_block_notification(message).map_err(ProcessingError::Notification)?
    else {
        return Ok(None);
    };
    let parse_started_at = Instant::now();
    let events = parser
        .parse_block(notification.block)
        .map_err(ProcessingError::Parse)?;
    let parse_duration = parse_started_at.elapsed();

    let summary = summarize_block(
        notification.slot,
        notification.block,
        &events,
        parse_duration,
    );

    Ok(Some((summary, events)))
}

fn load_dotenv() -> Result<(), IndexerError> {
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(dotenvy::Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(IndexerError::Dotenv(error.to_string())),
    }
}

async fn receive_blocks(
    websocket_url: &str,
    parser: &BlockParser,
    saver: &mut Saver,
) -> Result<(), String> {
    eprintln!(
        "Connecting to Solana WebSocket endpoint: {}",
        redact_endpoint(websocket_url)
    );
    let (mut websocket, _) = connect_async(websocket_url)
        .await
        .map_err(|error| format!("failed to connect: {error}"))?;
    websocket
        .send(Message::Text(block_subscribe_request().to_string().into()))
        .await
        .map_err(|error| format!("failed to subscribe: {error}"))?;

    while let Some(message) = websocket.next().await {
        match message.map_err(|error| format!("failed to read message: {error}"))? {
            Message::Text(text) => {
                let message = serde_json::from_str(&text)
                    .map_err(|error| format!("received invalid JSON: {error}"))?;
                if let Some(error) = subscription_error(&message) {
                    return Err(format!("blockSubscribe was rejected: {error}"));
                }
                if message.get("id") == Some(&Value::from(1)) && message.get("result").is_some() {
                    eprintln!("Subscribed to finalized Solana blocks");
                    continue;
                }
                match process_notification_events(&message, parser) {
                    Ok(Some((mut summary, events))) => {
                        let save_started_at = Instant::now();
                        saver
                            .save_block_events(&events)
                            .await
                            .map_err(ProcessingError::Save)
                            .map_err(|error| error.to_string())?;
                        summary.database_write_duration = save_started_at.elapsed();
                        println!("{summary}");
                    }
                    Ok(None) => {}
                    Err(error) => eprintln!("{error}"),
                }
            }
            Message::Ping(payload) => websocket
                .send(Message::Pong(payload))
                .await
                .map_err(|error| format!("failed to respond to ping: {error}"))?,
            Message::Close(frame) => return Err(format!("server closed connection: {frame:?}")),
            _ => {}
        }
    }

    Err("server closed connection without a close frame".to_owned())
}

fn redact_endpoint(endpoint: &str) -> String {
    let visible_characters = endpoint.chars().count().saturating_sub(6);
    endpoint
        .chars()
        .take(visible_characters)
        .collect::<String>()
        + &"*".repeat(endpoint.chars().count() - visible_characters)
}

fn subscription_error(message: &Value) -> Option<&Value> {
    (message.get("id") == Some(&Value::from(1)))
        .then(|| message.get("error"))
        .flatten()
}

fn extract_block_notification(
    message: &Value,
) -> Result<Option<BlockNotification<'_>>, NotificationError> {
    if message.get("method").and_then(Value::as_str) != Some("blockNotification") {
        return Ok(None);
    }

    let value = message
        .pointer("/params/result/value")
        .ok_or(NotificationError::InvalidField("params.result.value"))?;
    let slot = value
        .get("slot")
        .or_else(|| message.pointer("/params/result/context/slot"))
        .and_then(Value::as_u64)
        .ok_or(NotificationError::InvalidField("slot"))?;
    let block = value
        .get("block")
        .filter(|block| !block.is_null())
        .ok_or(NotificationError::BlockUnavailable)?;

    Ok(Some(BlockNotification { slot, block }))
}

fn summarize_block(
    slot: u64,
    block: &Value,
    events: &BlockEvents,
    parse_duration: Duration,
) -> BlockSummary {
    let (results, event_count) = events
        .transactions
        .iter()
        .flat_map(|transaction| &transaction.instructions)
        .fold((0, 0), |(results, event_count), instruction| {
            (
                results + 1,
                event_count + usize::from(instruction.result.is_ok()),
            )
        });

    BlockSummary {
        slot,
        block_transactions: block
            .get("transactions")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        parsed_transactions: events.transactions.len(),
        results,
        events: event_count,
        failures: results - event_count,
        parse_duration,
        database_write_duration: Duration::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use parser::{ParserConfig, ParserName};

    use super::*;

    fn parser() -> BlockParser {
        BlockParser::from_config(ParserConfig::new(vec![ParserName::PumpFun]).unwrap()).unwrap()
    }

    #[test]
    fn creates_the_requested_block_subscription() {
        assert_eq!(
            block_subscribe_request(),
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "blockSubscribe",
                "params": [
                    "all",
                    {
                        "commitment": "finalized",
                        "encoding": "json",
                        "transactionDetails": "full",
                        "maxSupportedTransactionVersion": 0,
                        "showRewards": false
                    }
                ]
            })
        );
    }

    #[test]
    fn processes_block_notifications_with_the_block_parser() {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "blockNotification",
            "params": {
                "result": {
                    "context": { "slot": 42 },
                    "value": {
                        "slot": 42,
                        "block": { "transactions": [] }
                    }
                },
                "subscription": 7
            }
        });

        let summary = process_notification(&notification, &parser())
            .unwrap()
            .unwrap();

        assert_eq!(summary.slot, 42);
        assert_eq!(summary.block_transactions, 0);
        assert_eq!(summary.parsed_transactions, 0);
        assert_eq!(summary.results, 0);
        assert_eq!(summary.events, 0);
        assert_eq!(summary.failures, 0);
        assert_eq!(summary.database_write_duration, Duration::ZERO);
    }

    #[test]
    fn ignores_unrelated_messages() {
        assert_eq!(
            process_notification(&json!({"id": 1}), &parser()).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_block_notifications_without_a_block() {
        let notification = json!({
            "method": "blockNotification",
            "params": {
                "result": {
                    "value": { "slot": 42, "block": null }
                }
            }
        });

        assert!(matches!(
            process_notification(&notification, &parser()),
            Err(ProcessingError::Notification(
                NotificationError::BlockUnavailable
            ))
        ));
    }

    #[test]
    fn renders_block_summaries() {
        assert_eq!(
            BlockSummary {
                slot: 42,
                block_transactions: 12,
                parsed_transactions: 2,
                results: 3,
                events: 2,
                failures: 1,
                parse_duration: Duration::from_micros(1_500),
                database_write_duration: Duration::from_micros(2_500),
            }
            .to_string(),
            "Block slot=42: transactions=12 parsed_transactions=2 results=3 events=2 failures=1 parse_time_ms=1.500 database_write_time_ms=2.500"
        );
    }

    #[test]
    fn redacts_the_last_six_endpoint_characters() {
        assert_eq!(
            redact_endpoint("wss://provider.example/api-key"),
            "wss://provider.example/a******"
        );
        assert_eq!(redact_endpoint("short"), "*****");
    }
}
