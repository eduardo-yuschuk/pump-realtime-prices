//! Ordered block, transaction, and instruction parsing pipeline.

pub mod block;
pub mod config;
pub mod error;
pub mod event;
pub mod instruction;
pub mod registry;
pub mod transaction;

pub use block::BlockParser;
pub use config::{ParserConfig, ParserName};
pub use error::{
    BlockParseError, ConfigError, InstructionParseError, InstructionParseFailure, ParserInitError,
    RegistryError, TransactionParseError,
};
pub use event::{BlockEvents, InstructionEvents, TransactionEvents};
pub use instruction::{DispatchOutcome, InstructionDispatcher};
pub use registry::ParserRegistry;
pub use transaction::TransactionParser;

#[cfg(test)]
mod test_support;
