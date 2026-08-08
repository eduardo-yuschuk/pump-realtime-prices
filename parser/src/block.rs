use serde_json::Value;

use crate::{
    BlockEvents, BlockParseError, InstructionDispatcher, ParserConfig, ParserInitError,
    ParserRegistry, RegistryError, TransactionParser,
};

pub struct BlockParser {
    transaction_parser: TransactionParser,
}

impl BlockParser {
    pub fn from_env() -> Result<Self, ParserInitError> {
        let config = ParserConfig::from_env()?;
        Ok(Self::from_config(config)?)
    }

    pub fn from_config(config: ParserConfig) -> Result<Self, RegistryError> {
        Ok(Self::from_registry(ParserRegistry::from_config(&config)?))
    }

    pub const fn from_registry(registry: ParserRegistry) -> Self {
        Self {
            transaction_parser: TransactionParser::new(InstructionDispatcher::new(registry)),
        }
    }

    pub fn parse_block(&self, block: &Value) -> Result<BlockEvents, BlockParseError> {
        let transactions = block.get("transactions").and_then(Value::as_array).ok_or(
            BlockParseError::InvalidField {
                field: "transactions",
                expected: "an array",
            },
        )?;
        let mut events = BlockEvents::default();

        for (transaction_index, transaction) in transactions.iter().enumerate() {
            match self
                .transaction_parser
                .parse_transaction(transaction, transaction_index)
            {
                Ok(Some(transaction_events)) => events.transactions.push(transaction_events),
                Ok(None) => {}
                Err(source) => {
                    return Err(BlockParseError::Transaction {
                        transaction_index,
                        source,
                    });
                }
            }
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use common::{ParsedEvent, TokenSwap};
    use serde_json::json;

    use super::*;
    use crate::{
        test_support::{instruction, registry, test_event, transaction, TEST_PROGRAM_ID},
        InstructionParseError, InstructionParseFailure, ParserName,
    };

    #[test]
    fn collects_only_relevant_transactions_in_block_order() {
        let block = json!({
            "transactions": [
                transaction(
                    "first",
                    vec![TEST_PROGRAM_ID],
                    vec![],
                    vec![],
                    vec![instruction(0, &[], &[1], 1)],
                    vec![],
                    Value::Null,
                ),
                transaction(
                    "empty",
                    vec![TEST_PROGRAM_ID],
                    vec![],
                    vec![],
                    vec![instruction(0, &[], &[0], 1)],
                    vec![],
                    Value::Null,
                ),
                transaction(
                    "failed",
                    vec![TEST_PROGRAM_ID],
                    vec![],
                    vec![],
                    vec![instruction(0, &[], &[1], 1)],
                    vec![],
                    json!({"InstructionError": [0, {"Custom": 1}]}),
                ),
                transaction(
                    "partial",
                    vec![TEST_PROGRAM_ID],
                    vec![],
                    vec![],
                    vec![
                        instruction(0, &[], &[2], 1),
                        instruction(0, &[], &[1], 1),
                    ],
                    vec![],
                    Value::Null,
                ),
            ]
        });
        let parser = BlockParser::from_registry(registry());

        let events = parser.parse_block(&block).unwrap();

        assert_eq!(events.transactions.len(), 2);
        assert_eq!(events.transactions[0].signature, "first");
        assert_eq!(events.transactions[0].transaction_index, 0);
        assert_eq!(
            events.transactions[0].instructions[0].result,
            Ok(test_event())
        );
        assert_eq!(events.transactions[1].signature, "partial");
        assert_eq!(events.transactions[1].transaction_index, 3);
        assert!(matches!(
            &events.transactions[1].instructions[0].result,
            Err(InstructionParseFailure {
                error: InstructionParseError::Protocol(_),
                ..
            })
        ));
        assert_eq!(
            events.transactions[1].instructions[1].result,
            Ok(test_event())
        );
    }

    #[test]
    fn parses_real_mainnet_block_events_in_execution_order() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sample_data/437840553.json.gz");
        let mut response = block_loader::load_gzip_json_file(path).unwrap();
        let config = ParserConfig::new(vec![ParserName::PumpFun]).unwrap();
        let parser = BlockParser::from_config(config).unwrap();

        let events = parser.parse_block(&response["result"]).unwrap();

        assert_eq!(
            events
                .transactions
                .iter()
                .map(|transaction| transaction.transaction_index)
                .collect::<Vec<_>>(),
            vec![10, 22, 552, 593, 730, 775, 843, 1054, 1213]
        );
        assert!(events.transactions.iter().all(|transaction| {
            transaction.instructions.len() == 1 && transaction.instructions[0].result.is_ok()
        }));
        let first = &events.transactions[0];
        assert_eq!(first.transaction_index, 10);
        assert_eq!(
            first.signature,
            "UDV5KYvUo6JQDpvzpWD4XWhnHmgpsx91A6DV2YPpCGrS2DNm2wPzZ1gYXwQ7MCNXzdZ3GMzcSzE5oWdSEpK1RQz"
        );
        assert_eq!(first.instructions.len(), 1);
        assert_eq!(first.instructions[0].outer_instruction_index, 2);
        assert_eq!(first.instructions[0].inner_instruction_index, Some(4));
        assert_eq!(first.instructions[0].stack_height, Some(3));
        assert_eq!(first.instructions[0].execution_ordinal, 7);
        assert_eq!(
            first.instructions[0].program_id.as_deref(),
            Some(pumpfun::PROGRAM_ID)
        );
        assert_eq!(
            first.instructions[0].result,
            Ok(ParsedEvent::TokenSwap(TokenSwap {
                user: "BwWK17cbHxwWBKZkUYvzxLcNQ1YVyaFezduWbtm2de6s".to_owned(),
                pool: "2ntct7fobbSv2rnMSccPXDxPsmuaRu4Zbykw8uvUcTmD".to_owned(),
                input_mint: "So11111111111111111111111111111111111111112".to_owned(),
                input_amount: 81_566_824,
                output_mint: "8NMMzUZ3sGdS1ZPUj1YGcJyHzRgxMHW9aHjqZkfbpump".to_owned(),
                output_amount: 2_519_953_715_914,
            }))
        );

        response["result"]["transactions"][10]["meta"]["innerInstructions"][0]["instructions"][4]
            ["data"] = json!("0");
        let partial_events = parser.parse_block(&response["result"]).unwrap();
        assert_eq!(partial_events.transactions.len(), 9);
        assert!(matches!(
            partial_events.transactions[0].instructions[0].result,
            Err(InstructionParseFailure {
                error: InstructionParseError::InvalidBase58Data(_),
                ..
            })
        ));
        assert!(partial_events.transactions[1].instructions[0]
            .result
            .is_ok());
    }

    #[test]
    fn rejects_blocks_without_transaction_arrays() {
        let parser = BlockParser::from_registry(registry());

        assert_eq!(
            parser.parse_block(&json!({})),
            Err(BlockParseError::InvalidField {
                field: "transactions",
                expected: "an array",
            })
        );
    }
}
