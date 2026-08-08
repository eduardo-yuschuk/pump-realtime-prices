//! Utilities for loading traversable Solana transaction JSON values.

use std::{error::Error, fmt, fs::File, io, path::Path};

use flate2::read::GzDecoder;
use serde_json::Value;

/// Errors returned while reading or parsing transaction JSON.
#[derive(Debug)]
pub enum TransactionLoaderError {
    /// The transaction source could not be read.
    Io(io::Error),
    /// The transaction source did not contain valid JSON.
    Json(serde_json::Error),
}

impl fmt::Display for TransactionLoaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Json(error) => write!(formatter, "JSON error: {error}"),
        }
    }
}

impl Error for TransactionLoaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
        }
    }
}

impl From<io::Error> for TransactionLoaderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for TransactionLoaderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Result type returned by transaction loading functions.
pub type Result<T> = std::result::Result<T, TransactionLoaderError>;

/// Loads a transaction from a gzip-compressed JSON file.
pub fn load_gzip_json_file(path: impl AsRef<Path>) -> Result<Value> {
    let file = File::open(path)?;
    Ok(serde_json::from_reader(GzDecoder::new(file))?)
}

/// Loads a transaction from an uncompressed JSON file.
pub fn load_json_file(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

/// Loads a transaction from an in-memory JSON string.
pub fn load_json_str(json: &str) -> Result<Value> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const SIGNATURE: &str =
        "UDV5KYvUo6JQDpvzpWD4XWhnHmgpsx91A6DV2YPpCGrS2DNm2wPzZ1gYXwQ7MCNXzdZ3GMzcSzE5oWdSEpK1RQz";

    fn fixture_path(file_name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample_data")
            .join(file_name)
    }

    fn assert_real_transaction(value: &Value) {
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["result"]["slot"], 437_840_553);
        assert_eq!(value["result"]["meta"]["err"], Value::Null);
        assert_eq!(value["result"]["transaction"]["signatures"][0], SIGNATURE);
    }

    #[test]
    fn loads_transaction_from_json_str() {
        let json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../sample_data/",
            "UDV5KYvUo6JQDpvzpWD4XWhnHmgpsx91A6DV2YPpCGrS2DNm2wPzZ1gYXwQ7MCNXzdZ3GMzcSzE5oWdSEpK1RQz.json"
        ));

        let value = load_json_str(json).expect("real transaction JSON should load");

        assert_real_transaction(&value);
    }

    #[test]
    fn rejects_malformed_transaction_json_str() {
        let error = load_json_str(r#"{"result":{"meta":{"err":null}"#)
            .expect_err("truncated transaction JSON should fail");

        assert!(matches!(error, TransactionLoaderError::Json(_)));
    }

    #[test]
    fn loads_transaction_from_json_file() {
        let path = fixture_path(concat!(
            "UDV5KYvUo6JQDpvzpWD4XWhnHmgpsx91A6DV2YPpCGrS2DNm2wPzZ1gYXwQ7MCNXzdZ3GMzcSzE5oWdSEpK1RQz",
            ".json"
        ));

        let value = load_json_file(path).expect("real transaction JSON file should load");

        assert_real_transaction(&value);
    }

    #[test]
    fn reports_json_error_for_malformed_transaction_file() {
        let error = load_json_file(fixture_path("malformed_transaction.json"))
            .expect_err("malformed transaction file should fail");

        assert!(matches!(error, TransactionLoaderError::Json(_)));
    }

    #[test]
    fn reports_io_error_for_missing_json_file() {
        let error = load_json_file(fixture_path("missing_transaction.json"))
            .expect_err("missing transaction file should fail");

        assert!(matches!(error, TransactionLoaderError::Io(_)));
    }

    #[test]
    fn loads_transaction_from_gzip_json_file() {
        let path = fixture_path(concat!(
            "UDV5KYvUo6JQDpvzpWD4XWhnHmgpsx91A6DV2YPpCGrS2DNm2wPzZ1gYXwQ7MCNXzdZ3GMzcSzE5oWdSEpK1RQz",
            ".json.gz"
        ));

        let value = load_gzip_json_file(path).expect("real gzip transaction JSON file should load");

        assert_real_transaction(&value);
    }

    #[test]
    fn reports_json_error_for_malformed_gzip_transaction_file() {
        let error = load_gzip_json_file(fixture_path("malformed_transaction.json.gz"))
            .expect_err("gzip-compressed malformed transaction should fail");

        assert!(matches!(error, TransactionLoaderError::Json(_)));
    }

    #[test]
    fn reports_json_error_for_invalid_gzip_file() {
        let error = load_gzip_json_file(fixture_path("malformed_transaction.json"))
            .expect_err("plain JSON should not load as gzip");

        assert!(matches!(error, TransactionLoaderError::Json(_)));
    }

    #[test]
    fn reports_io_error_for_missing_gzip_file() {
        let error = load_gzip_json_file(fixture_path("missing_transaction.json.gz"))
            .expect_err("missing gzip transaction file should fail");

        assert!(matches!(error, TransactionLoaderError::Io(_)));
    }
}
