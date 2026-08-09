//! Human-readable inspection of parsed block and transaction fixtures.

use std::{
    error::Error,
    fmt::{self, Write},
    path::{Path, PathBuf},
};

use common::ParsedEvent;
use parser::{
    ConfigError, InstructionDispatcher, ParserConfig, ParserRegistry, RegistryError,
    TransactionEvents, TransactionParseError, TransactionParser,
};
use transaction_loader::TransactionLoaderError;

pub const USAGE: &str = "Usage: visual-inspector transaction <signature> <protocol>";

#[derive(Debug)]
pub enum InspectorError {
    Arguments(String),
    Config(ConfigError),
    LoadFixture {
        path: PathBuf,
        source: TransactionLoaderError,
    },
    InvalidResponse(&'static str),
    SignatureMismatch {
        expected: String,
        actual: String,
    },
    Registry(RegistryError),
    Parse(TransactionParseError),
}

impl InspectorError {
    pub const fn shows_usage(&self) -> bool {
        matches!(self, Self::Arguments(_) | Self::Config(_))
    }
}

impl fmt::Display for InspectorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arguments(message) => formatter.write_str(message),
            Self::Config(error) => write!(formatter, "invalid protocol: {error}"),
            Self::LoadFixture { path, source } => {
                write!(
                    formatter,
                    "failed to load transaction fixture {}: {source}",
                    path.display()
                )
            }
            Self::InvalidResponse(message) => write!(formatter, "invalid RPC response: {message}"),
            Self::SignatureMismatch { expected, actual } => write!(
                formatter,
                "transaction fixture signature mismatch: expected {expected}, found {actual}"
            ),
            Self::Registry(error) => write!(formatter, "failed to build parser registry: {error}"),
            Self::Parse(error) => write!(formatter, "failed to parse transaction: {error}"),
        }
    }
}

impl Error for InspectorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::LoadFixture { source, .. } => Some(source),
            Self::Registry(error) => Some(error),
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}

struct TransactionCommand<'a> {
    signature: &'a str,
    protocol: &'a str,
}

pub fn run(args: &[String], workspace_root: impl AsRef<Path>) -> Result<String, InspectorError> {
    let command = parse_args(args)?;
    let config = ParserConfig::resolve(Some(command.protocol)).map_err(InspectorError::Config)?;
    if config.parsers().len() != 1 {
        return Err(InspectorError::Arguments(
            "transaction mode requires exactly one protocol name".to_owned(),
        ));
    }
    let protocol = config.parsers()[0].as_str();
    let registry = ParserRegistry::from_config(&config).map_err(InspectorError::Registry)?;
    let parser = TransactionParser::new(InstructionDispatcher::new(registry));
    let path = workspace_root
        .as_ref()
        .join("sample_data")
        .join(format!("{}.json.gz", command.signature));
    let response = transaction_loader::load_gzip_json_file(&path)
        .map_err(|source| InspectorError::LoadFixture { path, source })?;
    let transaction = response
        .get("result")
        .filter(|result| !result.is_null())
        .ok_or(InspectorError::InvalidResponse(
            "result must contain a transaction object",
        ))?;
    let fixture_signature = transaction
        .pointer("/transaction/signatures/0")
        .and_then(|value| value.as_str())
        .ok_or(InspectorError::InvalidResponse(
            "result.transaction.signatures[0] must be a string",
        ))?;
    if fixture_signature != command.signature {
        return Err(InspectorError::SignatureMismatch {
            expected: command.signature.to_owned(),
            actual: fixture_signature.to_owned(),
        });
    }

    let events = parser
        .parse_transaction(transaction, 0)
        .map_err(InspectorError::Parse)?;
    Ok(render_transaction(
        command.signature,
        protocol,
        events.as_ref(),
    ))
}

fn parse_args(args: &[String]) -> Result<TransactionCommand<'_>, InspectorError> {
    match args {
        [mode, ..] if mode != "transaction" => Err(InspectorError::Arguments(format!(
            "unsupported inspection mode {mode}; supported mode: transaction"
        ))),
        [_, signature, protocol] => {
            validate_signature(signature)?;
            Ok(TransactionCommand {
                signature,
                protocol,
            })
        }
        _ => Err(InspectorError::Arguments(
            "transaction mode requires a signature and protocol name".to_owned(),
        )),
    }
}

fn validate_signature(signature: &str) -> Result<(), InspectorError> {
    let bytes = bs58::decode(signature).into_vec().map_err(|error| {
        InspectorError::Arguments(format!("invalid transaction signature: {error}"))
    })?;
    if bytes.len() != 64 {
        return Err(InspectorError::Arguments(format!(
            "invalid transaction signature: expected 64 decoded bytes, found {}",
            bytes.len()
        )));
    }
    Ok(())
}

fn render_transaction(
    signature: &str,
    protocol: &str,
    transaction: Option<&TransactionEvents>,
) -> String {
    let mut output = String::new();
    writeln!(output, "Transaction: {signature}").unwrap();
    writeln!(output, "Protocol: {protocol}").unwrap();

    let Some(transaction) = transaction else {
        output.push_str("Results: 0\nNo events or parse failures detected.\n");
        return output;
    };
    writeln!(output, "Results: {}", transaction.instructions.len()).unwrap();

    for instruction in &transaction.instructions {
        let inner_index = instruction
            .inner_instruction_index
            .map_or_else(|| "none".to_owned(), |index| index.to_string());
        let stack_height = instruction
            .stack_height
            .map_or_else(|| "none".to_owned(), |height| height.to_string());
        let program_id = instruction.program_id.as_deref().unwrap_or("unknown");
        writeln!(
            output,
            "[{}] outer={} inner={} stack_height={} program={}",
            instruction.execution_ordinal,
            instruction.outer_instruction_index,
            inner_index,
            stack_height,
            program_id
        )
        .unwrap();

        match &instruction.result {
            Ok(ParsedEvent::TokenDiscovery(event)) => {
                output.push_str("  event=token_discovery\n");
                writeln!(output, "  mint={}", event.mint).unwrap();
                writeln!(output, "  creator={}", event.creator).unwrap();
                writeln!(output, "  name={}", event.name).unwrap();
                writeln!(output, "  symbol={}", event.symbol).unwrap();
                writeln!(output, "  uri={}", event.uri).unwrap();
            }
            Ok(ParsedEvent::TokenSwap(event)) => {
                output.push_str("  event=token_swap\n");
                writeln!(output, "  user={}", event.user).unwrap();
                writeln!(output, "  pool={}", event.pool).unwrap();
                writeln!(output, "  input_mint={}", event.input_mint).unwrap();
                writeln!(output, "  input_amount={}", event.input_amount).unwrap();
                writeln!(output, "  output_mint={}", event.output_mint).unwrap();
                writeln!(output, "  output_amount={}", event.output_amount).unwrap();
            }
            Err(failure) => {
                let failure_program = failure.program_id.as_deref().unwrap_or("unknown");
                writeln!(output, "  failure_program={failure_program}").unwrap();
                writeln!(output, "  failure={}", failure.error).unwrap();
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use parser::{InstructionEvents, InstructionParseError, InstructionParseFailure};

    const SIGNATURE: &str =
        "UDV5KYvUo6JQDpvzpWD4XWhnHmgpsx91A6DV2YPpCGrS2DNm2wPzZ1gYXwQ7MCNXzdZ3GMzcSzE5oWdSEpK1RQz";

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn validates_transaction_arguments() {
        for args in [
            arguments(&[]),
            arguments(&["transaction"]),
            arguments(&["transaction", SIGNATURE]),
            arguments(&["transaction", SIGNATURE, "pumpfun", "extra"]),
            arguments(&["transaction", "not-a-signature", "pumpfun"]),
        ] {
            assert!(matches!(
                run(&args, workspace_root()),
                Err(InspectorError::Arguments(_))
            ));
        }

        let error = run(
            &arguments(&["block", SIGNATURE, "pumpfun"]),
            workspace_root(),
        )
        .unwrap_err();
        assert!(
            matches!(error, InspectorError::Arguments(message) if message.contains("unsupported inspection mode"))
        );
    }

    #[test]
    fn reports_missing_transaction_fixtures() {
        let missing_signature = "1".repeat(64);
        let error = run(
            &arguments(&["transaction", &missing_signature, "pumpfun"]),
            workspace_root(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            InspectorError::LoadFixture { path, .. }
                if path.ends_with(format!("{missing_signature}.json.gz"))
        ));
    }

    #[test]
    fn validates_the_selected_protocol_before_loading_the_fixture() {
        let error = run(
            &arguments(&["transaction", SIGNATURE, "unknown"]),
            workspace_root(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            InspectorError::Config(ConfigError::UnknownParser { name }) if name == "unknown"
        ));
    }

    #[test]
    fn renders_real_transaction_events_in_execution_order() {
        let output = run(
            &arguments(&["transaction", SIGNATURE, "pumpfun"]),
            workspace_root(),
        )
        .unwrap();

        assert_eq!(
            output,
            format!(
                concat!(
                    "Transaction: {}\n",
                    "Protocol: pumpfun\n",
                    "Results: 1\n",
                    "[7] outer=2 inner=4 stack_height=3 ",
                    "program=6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P\n",
                    "  event=token_swap\n",
                    "  user=BwWK17cbHxwWBKZkUYvzxLcNQ1YVyaFezduWbtm2de6s\n",
                    "  pool=2ntct7fobbSv2rnMSccPXDxPsmuaRu4Zbykw8uvUcTmD\n",
                    "  input_mint=So11111111111111111111111111111111111111112\n",
                    "  input_amount=81566824\n",
                    "  output_mint=8NMMzUZ3sGdS1ZPUj1YGcJyHzRgxMHW9aHjqZkfbpump\n",
                    "  output_amount=2519953715914\n"
                ),
                SIGNATURE
            )
        );
    }

    #[test]
    fn renders_instruction_parse_failures() {
        let transaction = TransactionEvents {
            signature: SIGNATURE.to_owned(),
            transaction_index: 0,
            instructions: vec![InstructionEvents {
                program_id: None,
                outer_instruction_index: 2,
                inner_instruction_index: Some(1),
                stack_height: Some(3),
                execution_ordinal: 4,
                result: Err(InstructionParseFailure {
                    program_id: Some("Program111".to_owned()),
                    error: InstructionParseError::InvalidBase58Data("invalid byte".to_owned()),
                }),
            }],
        };

        let output = render_transaction(SIGNATURE, "pumpfun", Some(&transaction));

        assert!(output.contains("[4] outer=2 inner=1 stack_height=3 program=unknown\n"));
        assert!(output.contains("  failure_program=Program111\n"));
        assert!(output.contains("  failure=instruction data is not valid base58: invalid byte\n"));
    }
}
