//! Shared parsing and indexing primitives for program integrations.

use std::{error::Error, fmt};

/// A borrowed, source-independent view of a Solana instruction.
///
/// Block traversal and encoding concerns belong outside protocol parsers. Before
/// constructing this view, callers resolve account indexes to addresses and
/// decode the instruction data into bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstructionContext<'a> {
    program_id: &'a str,
    accounts: &'a [&'a str],
    data: &'a [u8],
}

impl<'a> InstructionContext<'a> {
    /// Creates an instruction context from already normalized instruction data.
    pub const fn new(program_id: &'a str, accounts: &'a [&'a str], data: &'a [u8]) -> Self {
        Self {
            program_id,
            accounts,
            data,
        }
    }

    /// Returns the program address that owns the instruction.
    pub const fn program_id(&self) -> &'a str {
        self.program_id
    }

    /// Returns instruction account addresses in their original order.
    pub const fn accounts(&self) -> &'a [&'a str] {
        self.accounts
    }

    /// Returns the decoded instruction data.
    pub const fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Returns the account at `index`, or a structured error if it is absent.
    pub fn account(&self, index: usize) -> ParseResult<&'a str> {
        self.accounts
            .get(index)
            .copied()
            .ok_or(ParseError::MissingAccounts {
                expected_at_least: index.saturating_add(1),
                actual: self.accounts.len(),
            })
    }

    /// Verifies that this instruction belongs to `expected`.
    pub fn ensure_program_id(&self, expected: &'static str) -> ParseResult<()> {
        if self.program_id == expected {
            Ok(())
        } else {
            Err(ParseError::ProgramMismatch {
                expected,
                actual: self.program_id.to_owned(),
            })
        }
    }

    /// Verifies that the instruction contains at least `expected_at_least` data bytes.
    pub fn ensure_data_len(&self, expected_at_least: usize) -> ParseResult<()> {
        if self.data.len() >= expected_at_least {
            Ok(())
        } else {
            Err(ParseError::DataTooShort {
                expected_at_least,
                actual: self.data.len(),
            })
        }
    }
}

/// Common interface implemented by each protocol instruction parser.
///
/// Implementations should validate [`Self::PROGRAM_ID`] before reading accounts
/// or instruction data. The associated output keeps protocol-specific
/// instruction variants strongly typed while allowing callers to use one
/// parsing contract.
pub trait InstructionParser {
    /// Program address accepted by this parser.
    const PROGRAM_ID: &'static str;

    /// Protocol-specific parsed instruction type.
    type ParsedInstruction;

    /// Parses one normalized instruction.
    fn parse_instruction(
        &self,
        instruction: InstructionContext<'_>,
    ) -> ParseResult<Self::ParsedInstruction>;
}

/// Errors produced while interpreting normalized instruction data.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// The instruction targets a different program.
    ProgramMismatch {
        expected: &'static str,
        actual: String,
    },
    /// The instruction does not contain all required accounts.
    MissingAccounts {
        expected_at_least: usize,
        actual: usize,
    },
    /// The instruction data is shorter than the parser requires.
    DataTooShort {
        expected_at_least: usize,
        actual: usize,
    },
    /// The instruction discriminator is not supported by the parser.
    UnknownDiscriminator(Vec<u8>),
    /// The instruction payload cannot be interpreted according to the protocol.
    InvalidInstructionData(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgramMismatch { expected, actual } => {
                write!(formatter, "expected program {expected}, found {actual}")
            }
            Self::MissingAccounts {
                expected_at_least,
                actual,
            } => write!(
                formatter,
                "expected at least {expected_at_least} accounts, found {actual}"
            ),
            Self::DataTooShort {
                expected_at_least,
                actual,
            } => write!(
                formatter,
                "expected at least {expected_at_least} instruction data bytes, found {actual}"
            ),
            Self::UnknownDiscriminator(discriminator) => {
                write!(
                    formatter,
                    "unknown instruction discriminator {discriminator:02x?}"
                )
            }
            Self::InvalidInstructionData(reason) => {
                write!(formatter, "invalid instruction data: {reason}")
            }
        }
    }
}

impl Error for ParseError {}

/// Result type shared by all instruction parsers.
pub type ParseResult<T> = Result<T, ParseError>;

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PROGRAM_ID: &str = "Test111111111111111111111111111111111111";

    #[derive(Debug, PartialEq, Eq)]
    struct ParsedInstruction {
        amount: u64,
    }

    struct TestParser;

    impl InstructionParser for TestParser {
        const PROGRAM_ID: &'static str = TEST_PROGRAM_ID;

        type ParsedInstruction = ParsedInstruction;

        fn parse_instruction(
            &self,
            instruction: InstructionContext<'_>,
        ) -> ParseResult<Self::ParsedInstruction> {
            instruction.ensure_program_id(Self::PROGRAM_ID)?;
            instruction.account(1)?;
            instruction.ensure_data_len(8)?;

            let amount = u64::from_le_bytes(instruction.data()[..8].try_into().unwrap());
            Ok(ParsedInstruction { amount })
        }
    }

    #[test]
    fn exposes_normalized_instruction_fields() {
        let accounts = ["payer", "mint"];
        let data = 42_u64.to_le_bytes();
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &data);

        assert_eq!(instruction.program_id(), TEST_PROGRAM_ID);
        assert_eq!(instruction.accounts(), accounts);
        assert_eq!(instruction.account(1).unwrap(), "mint");
        assert_eq!(instruction.data(), data);
    }

    #[test]
    fn parser_implementations_share_the_public_contract() {
        let accounts = ["payer", "mint"];
        let data = 42_u64.to_le_bytes();
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &data);

        assert_eq!(
            TestParser.parse_instruction(instruction).unwrap(),
            ParsedInstruction { amount: 42 }
        );
    }

    #[test]
    fn reports_a_program_mismatch() {
        let instruction = InstructionContext::new("AnotherProgram", &[], &[]);

        assert_eq!(
            TestParser.parse_instruction(instruction),
            Err(ParseError::ProgramMismatch {
                expected: TEST_PROGRAM_ID,
                actual: "AnotherProgram".to_owned(),
            })
        );
    }

    #[test]
    fn reports_missing_accounts_before_reading_data() {
        let accounts = ["payer"];
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &[]);

        assert_eq!(
            TestParser.parse_instruction(instruction),
            Err(ParseError::MissingAccounts {
                expected_at_least: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn reports_truncated_instruction_data() {
        let accounts = ["payer", "mint"];
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &[1, 2, 3]);

        assert_eq!(
            TestParser.parse_instruction(instruction),
            Err(ParseError::DataTooShort {
                expected_at_least: 8,
                actual: 3,
            })
        );
    }

    #[test]
    fn formats_errors_with_actionable_context() {
        let error = ParseError::MissingAccounts {
            expected_at_least: 3,
            actual: 1,
        };

        assert_eq!(error.to_string(), "expected at least 3 accounts, found 1");
    }
}
