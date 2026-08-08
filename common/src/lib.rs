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

/// A token discovered by a protocol parser.
///
/// Addresses are represented as base58 strings and metadata values are kept as
/// published by the protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenDiscovery {
    /// Mint address of the new token.
    pub mint: String,
    /// Address attributed as the token creator by the protocol.
    pub creator: String,
    /// Token display name.
    pub name: String,
    /// Token ticker symbol.
    pub symbol: String,
    /// URI of the token's off-chain metadata.
    pub uri: String,
}

/// A swap between two tokens discovered by a protocol parser.
///
/// Amounts are raw integer quantities in each mint's base units. Consumers are
/// responsible for applying the corresponding mint decimals for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenSwap {
    /// Address whose assets were exchanged.
    pub user: String,
    /// Pool or market address through which the swap was executed.
    pub pool: String,
    /// Mint address of the token supplied by the user.
    pub input_mint: String,
    /// Amount of the input token supplied, in base units.
    pub input_amount: u64,
    /// Mint address of the token received by the user.
    pub output_mint: String,
    /// Amount of the output token received, in base units.
    pub output_amount: u64,
}

/// Storage-facing event produced by parsing one protocol instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedEvent {
    /// A newly discovered token.
    TokenDiscovery(TokenDiscovery),
    /// An executed token swap.
    TokenSwap(TokenSwap),
}

/// Common interface implemented by each protocol instruction parser.
///
/// Implementations should validate [`Self::PROGRAM_ID`] before reading accounts
/// or instruction data. They return `Ok(None)` for instructions that do not
/// produce a token discovery or token swap, and an error when a relevant event
/// is recognized but malformed.
pub trait InstructionParser {
    /// Program address accepted by this parser.
    const PROGRAM_ID: &'static str;

    /// Parses one normalized instruction into a storage-facing event, if any.
    fn parse_instruction(
        &self,
        instruction: InstructionContext<'_>,
    ) -> ParseResult<Option<ParsedEvent>>;
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

    struct TestParser;

    impl InstructionParser for TestParser {
        const PROGRAM_ID: &'static str = TEST_PROGRAM_ID;

        fn parse_instruction(
            &self,
            instruction: InstructionContext<'_>,
        ) -> ParseResult<Option<ParsedEvent>> {
            instruction.ensure_program_id(Self::PROGRAM_ID)?;

            match instruction.data().first() {
                Some(0) => Ok(Some(ParsedEvent::TokenDiscovery(TokenDiscovery {
                    creator: instruction.account(0)?.to_owned(),
                    mint: instruction.account(1)?.to_owned(),
                    name: "Test Token".to_owned(),
                    symbol: "TEST".to_owned(),
                    uri: "https://example.com/token.json".to_owned(),
                }))),
                Some(1) => {
                    instruction.ensure_data_len(17)?;
                    let input_amount =
                        u64::from_le_bytes(instruction.data()[1..9].try_into().unwrap());
                    let output_amount =
                        u64::from_le_bytes(instruction.data()[9..17].try_into().unwrap());

                    Ok(Some(ParsedEvent::TokenSwap(TokenSwap {
                        user: instruction.account(0)?.to_owned(),
                        pool: instruction.account(1)?.to_owned(),
                        input_mint: instruction.account(2)?.to_owned(),
                        input_amount,
                        output_mint: instruction.account(3)?.to_owned(),
                        output_amount,
                    })))
                }
                _ => Ok(None),
            }
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
    fn parser_implementations_return_token_discoveries() {
        let accounts = ["creator", "mint"];
        let data = [0];
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &data);

        assert_eq!(
            TestParser.parse_instruction(instruction).unwrap(),
            Some(ParsedEvent::TokenDiscovery(TokenDiscovery {
                mint: "mint".to_owned(),
                creator: "creator".to_owned(),
                name: "Test Token".to_owned(),
                symbol: "TEST".to_owned(),
                uri: "https://example.com/token.json".to_owned(),
            }))
        );
    }

    #[test]
    fn parser_implementations_return_token_swaps() {
        let accounts = ["user", "pool", "input-mint", "output-mint"];
        let mut data = vec![1];
        data.extend_from_slice(&42_u64.to_le_bytes());
        data.extend_from_slice(&84_u64.to_le_bytes());
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &data);

        assert_eq!(
            TestParser.parse_instruction(instruction).unwrap(),
            Some(ParsedEvent::TokenSwap(TokenSwap {
                user: "user".to_owned(),
                pool: "pool".to_owned(),
                input_mint: "input-mint".to_owned(),
                input_amount: 42,
                output_mint: "output-mint".to_owned(),
                output_amount: 84,
            }))
        );
    }

    #[test]
    fn parser_implementations_return_none_for_irrelevant_instructions() {
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &[], &[2]);

        assert_eq!(TestParser.parse_instruction(instruction).unwrap(), None);
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
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &[0]);

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
        let accounts = ["user", "pool", "input-mint", "output-mint"];
        let instruction = InstructionContext::new(TEST_PROGRAM_ID, &accounts, &[1, 2, 3]);

        assert_eq!(
            TestParser.parse_instruction(instruction),
            Err(ParseError::DataTooShort {
                expected_at_least: 17,
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
