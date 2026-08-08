use std::{error::Error, fmt};

use common::ParseError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    MissingParsers,
    NonUnicodeParsers,
    EmptyParsers,
    EmptyParserName { index: usize },
    UnknownParser { name: String },
    DuplicateParser { name: String },
    Dotenv(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingParsers => write!(formatter, "PARSERS is not set"),
            Self::NonUnicodeParsers => write!(formatter, "PARSERS is not valid Unicode"),
            Self::EmptyParsers => write!(formatter, "PARSERS must contain at least one parser"),
            Self::EmptyParserName { index } => {
                write!(formatter, "PARSERS contains an empty name at index {index}")
            }
            Self::UnknownParser { name } => write!(formatter, "unknown parser name {name}"),
            Self::DuplicateParser { name } => write!(formatter, "duplicate parser name {name}"),
            Self::Dotenv(message) => write!(formatter, "failed to load .env: {message}"),
        }
    }
}

impl Error for ConfigError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateProgramId { program_id: String },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateProgramId { program_id } => {
                write!(formatter, "duplicate parser program ID {program_id}")
            }
        }
    }
}

impl Error for RegistryError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParserInitError {
    Config(ConfigError),
    Registry(RegistryError),
}

impl fmt::Display for ParserInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::Registry(error) => error.fmt(formatter),
        }
    }
}

impl Error for ParserInitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Registry(error) => Some(error),
        }
    }
}

impl From<ConfigError> for ParserInitError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<RegistryError> for ParserInitError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionParseFailure {
    pub program_id: Option<String>,
    pub error: InstructionParseError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstructionParseError {
    InvalidField {
        field: &'static str,
        expected: &'static str,
    },
    IndexOutOfBounds {
        field: &'static str,
        index: u64,
        len: usize,
    },
    InstructionDataTooLong {
        encoded_len: usize,
        max_encoded_len: usize,
    },
    InvalidBase58Data(String),
    Protocol(ParseError),
}

impl fmt::Display for InstructionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField { field, expected } => {
                write!(formatter, "{field} must be {expected}")
            }
            Self::IndexOutOfBounds { field, index, len } => {
                write!(
                    formatter,
                    "{field} index {index} is out of bounds for {len} addresses"
                )
            }
            Self::InstructionDataTooLong {
                encoded_len,
                max_encoded_len,
            } => write!(
                formatter,
                "encoded instruction data length {encoded_len} exceeds maximum {max_encoded_len}"
            ),
            Self::InvalidBase58Data(message) => {
                write!(formatter, "instruction data is not valid base58: {message}")
            }
            Self::Protocol(error) => error.fmt(formatter),
        }
    }
}

impl Error for InstructionParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Protocol(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionParseError {
    InvalidField {
        field: &'static str,
        expected: &'static str,
    },
    DuplicateInnerInstructionGroup {
        outer_instruction_index: usize,
    },
    InnerInstructionGroupOutOfBounds {
        outer_instruction_index: usize,
        outer_instruction_count: usize,
    },
}

impl fmt::Display for TransactionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField { field, expected } => {
                write!(formatter, "{field} must be {expected}")
            }
            Self::DuplicateInnerInstructionGroup {
                outer_instruction_index,
            } => write!(
                formatter,
                "duplicate inner instruction group for outer instruction {outer_instruction_index}"
            ),
            Self::InnerInstructionGroupOutOfBounds {
                outer_instruction_index,
                outer_instruction_count,
            } => write!(
                formatter,
                "inner instruction group index {outer_instruction_index} is out of bounds for {outer_instruction_count} outer instructions"
            ),
        }
    }
}

impl Error for TransactionParseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockParseError {
    InvalidField {
        field: &'static str,
        expected: &'static str,
    },
    Transaction {
        transaction_index: usize,
        source: TransactionParseError,
    },
}

impl fmt::Display for BlockParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField { field, expected } => {
                write!(formatter, "{field} must be {expected}")
            }
            Self::Transaction {
                transaction_index,
                source,
            } => write!(formatter, "transaction {transaction_index}: {source}"),
        }
    }
}

impl Error for BlockParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transaction { source, .. } => Some(source),
            _ => None,
        }
    }
}
