use std::{error::Error, fmt, fs::File, io, path::Path};

use flate2::read::GzDecoder;
use serde_json::Value;

/// Errors returned while reading or parsing block JSON.
#[derive(Debug)]
pub enum BlockLoaderError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for BlockLoaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Json(error) => write!(formatter, "JSON error: {error}"),
        }
    }
}

impl Error for BlockLoaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
        }
    }
}

impl From<io::Error> for BlockLoaderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for BlockLoaderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub type Result<T> = std::result::Result<T, BlockLoaderError>;

/// Loads a JSON block from an uncompressed file.
pub fn load_json_file(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

/// Loads a JSON block from a gzip-compressed file.
pub fn load_gzip_json_file(path: impl AsRef<Path>) -> Result<Value> {
    let file = File::open(path)?;
    Ok(serde_json::from_reader(GzDecoder::new(file))?)
}

/// Loads a JSON block from an in-memory string.
pub fn load_json_str(json: &str) -> Result<Value> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample_data")
            .join(name)
    }

    #[test]
    fn loads_an_uncompressed_json_file() {
        let block = load_json_file(fixture("block_empty.json")).unwrap();

        assert_eq!(block["blockHeight"], 300000000);
        assert!(block["transactions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn loads_a_gzip_compressed_json_file() {
        let block = load_gzip_json_file(fixture("437840553.json.gz")).unwrap();

        assert!(block.is_object());
        assert!(block["result"]["transactions"].is_array());
    }

    #[test]
    fn loads_an_in_memory_json_string() {
        let block = load_json_str(r#"{"blockHeight": 300000002, "transactions": []}"#).unwrap();

        assert_eq!(block["blockHeight"], 300000002);
        assert!(block["transactions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn reports_errors_for_each_source() {
        assert!(matches!(
            load_json_file(fixture("missing.json")),
            Err(BlockLoaderError::Io(_))
        ));
        assert!(matches!(
            load_gzip_json_file(fixture("missing.json.gz")),
            Err(BlockLoaderError::Io(_))
        ));
        assert!(matches!(
            load_json_str("not valid JSON"),
            Err(BlockLoaderError::Json(_))
        ));
    }
}
