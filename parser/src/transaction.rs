use std::collections::BTreeMap;

use common::InstructionContext;
use serde_json::Value;

use crate::{
    DispatchOutcome, InstructionDispatcher, InstructionEvents, InstructionParseError,
    InstructionParseFailure, TransactionEvents, TransactionParseError,
};

const MAX_BASE58_INSTRUCTION_DATA_LEN: usize = 1_683;

pub struct TransactionParser {
    dispatcher: InstructionDispatcher,
}

impl TransactionParser {
    pub const fn new(dispatcher: InstructionDispatcher) -> Self {
        Self { dispatcher }
    }

    pub fn parse_transaction(
        &self,
        entry: &Value,
        transaction_index: usize,
    ) -> Result<Option<TransactionEvents>, TransactionParseError> {
        let meta = entry.get("meta").and_then(Value::as_object).ok_or(
            TransactionParseError::InvalidField {
                field: "meta",
                expected: "an object",
            },
        )?;
        let transaction_error = meta.get("err").ok_or(TransactionParseError::InvalidField {
            field: "meta.err",
            expected: "a value",
        })?;
        if !transaction_error.is_null() {
            return Ok(None);
        }

        let transaction = entry.get("transaction").and_then(Value::as_object).ok_or(
            TransactionParseError::InvalidField {
                field: "transaction",
                expected: "an object",
            },
        )?;
        let signature = transaction
            .get("signatures")
            .and_then(Value::as_array)
            .and_then(|signatures| signatures.first())
            .and_then(Value::as_str)
            .ok_or(TransactionParseError::InvalidField {
                field: "transaction.signatures[0]",
                expected: "a string",
            })?;
        let message = transaction
            .get("message")
            .and_then(Value::as_object)
            .ok_or(TransactionParseError::InvalidField {
                field: "transaction.message",
                expected: "an object",
            })?;
        let outer_instructions = message
            .get("instructions")
            .and_then(Value::as_array)
            .ok_or(TransactionParseError::InvalidField {
                field: "transaction.message.instructions",
                expected: "an array",
            })?;
        let account_keys = resolve_account_keys(message, meta)?;
        let inner_instruction_groups =
            resolve_inner_instruction_groups(meta, outer_instructions.len())?;

        let mut execution_ordinal = 0;
        let mut instruction_events = Vec::new();
        for (outer_instruction_index, instruction) in outer_instructions.iter().enumerate() {
            let mut invocation_stack = Vec::new();
            if let Some(result) = self.parse_instruction(
                instruction,
                &account_keys,
                outer_instruction_index,
                None,
                execution_ordinal,
                &mut invocation_stack,
            ) {
                instruction_events.push(result);
            }
            execution_ordinal += 1;

            if let Some(inner_instructions) = inner_instruction_groups.get(&outer_instruction_index)
            {
                for (inner_instruction_index, instruction) in inner_instructions.iter().enumerate()
                {
                    if let Some(result) = self.parse_instruction(
                        instruction,
                        &account_keys,
                        outer_instruction_index,
                        Some(inner_instruction_index),
                        execution_ordinal,
                        &mut invocation_stack,
                    ) {
                        instruction_events.push(result);
                    }
                    execution_ordinal += 1;
                }
            }
        }

        if instruction_events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(TransactionEvents {
                signature: signature.to_owned(),
                transaction_index,
                instructions: instruction_events,
            }))
        }
    }

    fn parse_instruction<'a>(
        &self,
        instruction: &Value,
        account_keys: &[&'a str],
        outer_instruction_index: usize,
        inner_instruction_index: Option<usize>,
        execution_ordinal: usize,
        invocation_stack: &mut Vec<NormalizedInstruction<'a>>,
    ) -> Option<InstructionEvents> {
        let observed_stack_height = instruction
            .get("stackHeight")
            .and_then(Value::as_u64)
            .and_then(|height| u32::try_from(height).ok());
        align_invocation_stack(
            invocation_stack,
            inner_instruction_index,
            observed_stack_height,
        );
        let program_index = match instruction.get("programIdIndex").and_then(Value::as_u64) {
            Some(index) => index,
            None => {
                return Some(instruction_failure(
                    None,
                    outer_instruction_index,
                    inner_instruction_index,
                    None,
                    execution_ordinal,
                    InstructionParseError::InvalidField {
                        field: "programIdIndex",
                        expected: "an unsigned integer",
                    },
                ));
            }
        };
        let Some(program_id) = usize::try_from(program_index)
            .ok()
            .and_then(|index| account_keys.get(index).copied())
        else {
            return Some(instruction_failure(
                None,
                outer_instruction_index,
                inner_instruction_index,
                None,
                execution_ordinal,
                InstructionParseError::IndexOutOfBounds {
                    field: "programIdIndex",
                    index: program_index,
                    len: account_keys.len(),
                },
            ));
        };

        if !self.dispatcher.is_configured(program_id) {
            return None;
        }

        let stack_height = match instruction.get("stackHeight") {
            None | Some(Value::Null) => None,
            Some(value) => match value.as_u64().and_then(|height| u32::try_from(height).ok()) {
                Some(height) => Some(height),
                None => {
                    return Some(instruction_failure(
                        Some(program_id),
                        outer_instruction_index,
                        inner_instruction_index,
                        None,
                        execution_ordinal,
                        InstructionParseError::InvalidField {
                            field: "stackHeight",
                            expected: "a u32 or null",
                        },
                    ));
                }
            },
        };

        let Some(account_indexes) = instruction.get("accounts").and_then(Value::as_array) else {
            return Some(instruction_failure(
                Some(program_id),
                outer_instruction_index,
                inner_instruction_index,
                stack_height,
                execution_ordinal,
                InstructionParseError::InvalidField {
                    field: "accounts",
                    expected: "an array of unsigned integers",
                },
            ));
        };
        let mut accounts = Vec::with_capacity(account_indexes.len());
        for account_index in account_indexes {
            let Some(index) = account_index.as_u64() else {
                return Some(instruction_failure(
                    Some(program_id),
                    outer_instruction_index,
                    inner_instruction_index,
                    stack_height,
                    execution_ordinal,
                    InstructionParseError::InvalidField {
                        field: "accounts[]",
                        expected: "an unsigned integer",
                    },
                ));
            };
            let Some(account) = usize::try_from(index)
                .ok()
                .and_then(|index| account_keys.get(index).copied())
            else {
                return Some(instruction_failure(
                    Some(program_id),
                    outer_instruction_index,
                    inner_instruction_index,
                    stack_height,
                    execution_ordinal,
                    InstructionParseError::IndexOutOfBounds {
                        field: "accounts",
                        index,
                        len: account_keys.len(),
                    },
                ));
            };
            accounts.push(account);
        }

        let Some(encoded_data) = instruction.get("data").and_then(Value::as_str) else {
            return Some(instruction_failure(
                Some(program_id),
                outer_instruction_index,
                inner_instruction_index,
                stack_height,
                execution_ordinal,
                InstructionParseError::InvalidField {
                    field: "data",
                    expected: "a base58 string",
                },
            ));
        };
        if encoded_data.len() > MAX_BASE58_INSTRUCTION_DATA_LEN {
            return Some(instruction_failure(
                Some(program_id),
                outer_instruction_index,
                inner_instruction_index,
                stack_height,
                execution_ordinal,
                InstructionParseError::InstructionDataTooLong {
                    encoded_len: encoded_data.len(),
                    max_encoded_len: MAX_BASE58_INSTRUCTION_DATA_LEN,
                },
            ));
        }
        let data = match bs58::decode(encoded_data).into_vec() {
            Ok(data) => data,
            Err(error) => {
                return Some(instruction_failure(
                    Some(program_id),
                    outer_instruction_index,
                    inner_instruction_index,
                    stack_height,
                    execution_ordinal,
                    InstructionParseError::InvalidBase58Data(error.to_string()),
                ));
            }
        };

        let parent = immediate_parent(invocation_stack, inner_instruction_index, stack_height)
            .map(NormalizedInstruction::context);
        let normalized = NormalizedInstruction {
            program_id,
            accounts,
            data,
            invocation_height: stack_height.or((inner_instruction_index.is_none()).then_some(1)),
        };
        let mut context = normalized.context();
        if let Some(parent) = parent {
            context = context.with_parent_instruction(parent);
        }
        let outcome = self.dispatcher.dispatch(context);
        invocation_stack.push(normalized);

        match outcome {
            DispatchOutcome::NoEvent => None,
            DispatchOutcome::Event(event) => Some(InstructionEvents {
                program_id: Some(program_id.to_owned()),
                outer_instruction_index,
                inner_instruction_index,
                stack_height,
                execution_ordinal,
                result: Ok(event),
            }),
            DispatchOutcome::Failure(failure) => Some(InstructionEvents {
                program_id: Some(program_id.to_owned()),
                outer_instruction_index,
                inner_instruction_index,
                stack_height,
                execution_ordinal,
                result: Err(failure),
            }),
        }
    }
}

struct NormalizedInstruction<'a> {
    program_id: &'a str,
    accounts: Vec<&'a str>,
    data: Vec<u8>,
    invocation_height: Option<u32>,
}

impl NormalizedInstruction<'_> {
    fn context(&self) -> InstructionContext<'_> {
        InstructionContext::new(self.program_id, &self.accounts, &self.data)
    }
}

fn align_invocation_stack(
    stack: &mut Vec<NormalizedInstruction<'_>>,
    inner_instruction_index: Option<usize>,
    stack_height: Option<u32>,
) {
    if inner_instruction_index.is_none() {
        stack.clear();
        return;
    }

    let Some(stack_height) = stack_height else {
        stack.clear();
        return;
    };
    while stack.last().is_some_and(|instruction| {
        instruction
            .invocation_height
            .is_none_or(|height| height >= stack_height)
    }) {
        stack.pop();
    }
}

fn immediate_parent<'a>(
    stack: &'a [NormalizedInstruction<'_>],
    inner_instruction_index: Option<usize>,
    stack_height: Option<u32>,
) -> Option<&'a NormalizedInstruction<'a>> {
    inner_instruction_index?;
    match stack_height {
        Some(stack_height) => stack.last().filter(|instruction| {
            instruction
                .invocation_height
                .and_then(|height| height.checked_add(1))
                == Some(stack_height)
        }),
        None => None,
    }
}

fn resolve_account_keys<'a>(
    message: &'a serde_json::Map<String, Value>,
    meta: &'a serde_json::Map<String, Value>,
) -> Result<Vec<&'a str>, TransactionParseError> {
    let static_keys = message.get("accountKeys").and_then(Value::as_array).ok_or(
        TransactionParseError::InvalidField {
            field: "transaction.message.accountKeys",
            expected: "an array of strings",
        },
    )?;
    let mut account_keys = Vec::new();
    append_string_values(
        &mut account_keys,
        static_keys,
        "transaction.message.accountKeys[]",
    )?;

    if let Some(loaded_addresses) = meta.get("loadedAddresses") {
        if !loaded_addresses.is_null() {
            let loaded_addresses =
                loaded_addresses
                    .as_object()
                    .ok_or(TransactionParseError::InvalidField {
                        field: "meta.loadedAddresses",
                        expected: "an object or null",
                    })?;
            append_required_string_array(
                &mut account_keys,
                loaded_addresses.get("writable"),
                "meta.loadedAddresses.writable",
            )?;
            append_required_string_array(
                &mut account_keys,
                loaded_addresses.get("readonly"),
                "meta.loadedAddresses.readonly",
            )?;
        }
    }

    Ok(account_keys)
}

fn append_required_string_array<'a>(
    output: &mut Vec<&'a str>,
    value: Option<&'a Value>,
    field: &'static str,
) -> Result<(), TransactionParseError> {
    match value {
        None | Some(Value::Null) => Err(TransactionParseError::InvalidField {
            field,
            expected: "an array of strings",
        }),
        Some(value) => {
            let values = value
                .as_array()
                .ok_or(TransactionParseError::InvalidField {
                    field,
                    expected: "an array of strings",
                })?;
            append_string_values(output, values, field)
        }
    }
}

fn append_string_values<'a>(
    output: &mut Vec<&'a str>,
    values: &'a [Value],
    field: &'static str,
) -> Result<(), TransactionParseError> {
    for value in values {
        output.push(value.as_str().ok_or(TransactionParseError::InvalidField {
            field,
            expected: "a string",
        })?);
    }
    Ok(())
}

fn resolve_inner_instruction_groups(
    meta: &serde_json::Map<String, Value>,
    outer_instruction_count: usize,
) -> Result<BTreeMap<usize, &[Value]>, TransactionParseError> {
    let groups = match meta.get("innerInstructions") {
        None | Some(Value::Null) => return Ok(BTreeMap::new()),
        Some(value) => value
            .as_array()
            .ok_or(TransactionParseError::InvalidField {
                field: "meta.innerInstructions",
                expected: "an array or null",
            })?,
    };

    let mut resolved = BTreeMap::new();
    for group in groups {
        let group = group
            .as_object()
            .ok_or(TransactionParseError::InvalidField {
                field: "meta.innerInstructions[]",
                expected: "an object",
            })?;
        let index = group.get("index").and_then(Value::as_u64).ok_or(
            TransactionParseError::InvalidField {
                field: "meta.innerInstructions[].index",
                expected: "an unsigned integer",
            },
        )?;
        let index = usize::try_from(index).map_err(|_| {
            TransactionParseError::InnerInstructionGroupOutOfBounds {
                outer_instruction_index: usize::MAX,
                outer_instruction_count,
            }
        })?;
        if index >= outer_instruction_count {
            return Err(TransactionParseError::InnerInstructionGroupOutOfBounds {
                outer_instruction_index: index,
                outer_instruction_count,
            });
        }
        let instructions = group.get("instructions").and_then(Value::as_array).ok_or(
            TransactionParseError::InvalidField {
                field: "meta.innerInstructions[].instructions",
                expected: "an array",
            },
        )?;
        if resolved.insert(index, instructions.as_slice()).is_some() {
            return Err(TransactionParseError::DuplicateInnerInstructionGroup {
                outer_instruction_index: index,
            });
        }
    }

    Ok(resolved)
}

fn instruction_failure(
    program_id: Option<&str>,
    outer_instruction_index: usize,
    inner_instruction_index: Option<usize>,
    stack_height: Option<u32>,
    execution_ordinal: usize,
    error: InstructionParseError,
) -> InstructionEvents {
    let program_id = program_id.map(str::to_owned);
    InstructionEvents {
        program_id: program_id.clone(),
        outer_instruction_index,
        inner_instruction_index,
        stack_height,
        execution_ordinal,
        result: Err(InstructionParseFailure { program_id, error }),
    }
}

#[cfg(test)]
mod tests {
    use common::{ParseError, ParsedEvent, TokenDiscovery, TokenSwap};
    use serde_json::json;

    use super::*;
    use crate::{
        test_support::{instruction, registry, test_event, transaction, TEST_PROGRAM_ID},
        InstructionParseError, ParserConfig, ParserName, ParserRegistry,
    };

    fn parser() -> TransactionParser {
        TransactionParser::new(InstructionDispatcher::new(registry()))
    }

    #[test]
    fn parses_legacy_transactions_and_allows_missing_loaded_addresses() {
        let mut entry = transaction(
            "legacy-signature",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[1], 1)],
            vec![],
            Value::Null,
        );
        entry["version"] = json!("legacy");
        entry["meta"]
            .as_object_mut()
            .unwrap()
            .remove("loadedAddresses");

        let events = parser().parse_transaction(&entry, 7).unwrap().unwrap();

        assert_eq!(events.signature, "legacy-signature");
        assert_eq!(events.transaction_index, 7);
        assert_eq!(events.instructions.len(), 1);
        assert_eq!(events.instructions[0].execution_ordinal, 0);
        assert_eq!(events.instructions[0].result, Ok(test_event()));
    }

    #[test]
    fn resolves_versioned_loaded_writable_and_readonly_addresses() {
        let entry = transaction(
            "v0-signature",
            vec!["static-account"],
            vec!["loaded-account"],
            vec![TEST_PROGRAM_ID],
            vec![instruction(2, &[1], &[3], 1)],
            vec![],
            Value::Null,
        );

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(
            events.instructions[0].result,
            Ok(ParsedEvent::TokenDiscovery(TokenDiscovery {
                mint: "loaded-account".to_owned(),
                creator: "creator".to_owned(),
                name: "Test Token".to_owned(),
                symbol: "TEST".to_owned(),
                uri: "https://example.com/token.json".to_owned(),
            }))
        );
    }

    #[test]
    fn preserves_outer_and_nested_cpi_evaluation_order() {
        let entry = transaction(
            "ordered-signature",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[1], 1), instruction(0, &[], &[0], 1)],
            vec![
                json!({
                    "index": 0,
                    "instructions": [
                        instruction(0, &[], &[1], 2),
                        instruction(0, &[], &[2], 3),
                    ]
                }),
                json!({
                    "index": 1,
                    "instructions": [instruction(0, &[], &[1], 2)]
                }),
            ],
            Value::Null,
        );

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(
            events
                .instructions
                .iter()
                .map(|instruction| instruction.execution_ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 4]
        );
        assert_eq!(events.instructions[0].inner_instruction_index, None);
        assert_eq!(events.instructions[1].inner_instruction_index, Some(0));
        assert_eq!(events.instructions[1].stack_height, Some(2));
        assert_eq!(events.instructions[2].inner_instruction_index, Some(1));
        assert_eq!(events.instructions[2].stack_height, Some(3));
        assert!(events.instructions[2].result.is_err());
        assert_eq!(events.instructions[3].outer_instruction_index, 1);
        assert_eq!(events.instructions[3].inner_instruction_index, Some(0));
    }

    #[test]
    fn exposes_the_immediate_parent_for_nested_cpi_events() {
        let entry = transaction(
            "parent-context",
            vec!["AggregatorProgram", TEST_PROGRAM_ID, "parent-account"],
            vec![],
            vec![],
            vec![instruction(0, &[], &[], 1)],
            vec![json!({
                "index": 0,
                "instructions": [
                    instruction(1, &[2], &[0], 2),
                    instruction(1, &[], &[4], 3),
                ]
            })],
            Value::Null,
        );

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(events.instructions.len(), 1);
        assert_eq!(events.instructions[0].execution_ordinal, 2);
        assert_eq!(
            events.instructions[0].result,
            Ok(ParsedEvent::TokenDiscovery(TokenDiscovery {
                mint: "parent-account".to_owned(),
                creator: "creator".to_owned(),
                name: "Test Token".to_owned(),
                symbol: "TEST".to_owned(),
                uri: "https://example.com/token.json".to_owned(),
            }))
        );
    }

    #[test]
    fn does_not_reuse_a_parent_after_an_unconfigured_sibling() {
        let entry = transaction(
            "stale-parent",
            vec!["AggregatorProgram", TEST_PROGRAM_ID, "parent-account"],
            vec![],
            vec![],
            vec![instruction(0, &[], &[], 1)],
            vec![json!({
                "index": 0,
                "instructions": [
                    instruction(1, &[2], &[0], 2),
                    instruction(0, &[], &[], 2),
                    instruction(1, &[], &[4], 3),
                ]
            })],
            Value::Null,
        );

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(events.instructions.len(), 1);
        assert!(matches!(
            &events.instructions[0].result,
            Err(InstructionParseFailure {
                error: InstructionParseError::Protocol(ParseError::InvalidInstructionData(reason)),
                ..
            }) if reason == "test instruction has no immediate parent"
        ));
    }

    #[test]
    fn does_not_reuse_a_parent_after_a_malformed_configured_sibling() {
        let mut entry = transaction(
            "malformed-sibling",
            vec!["AggregatorProgram", TEST_PROGRAM_ID, "parent-account"],
            vec![],
            vec![],
            vec![instruction(0, &[], &[], 1)],
            vec![json!({
                "index": 0,
                "instructions": [
                    instruction(1, &[2], &[0], 2),
                    instruction(1, &[], &[], 2),
                    instruction(1, &[], &[4], 3),
                ]
            })],
            Value::Null,
        );
        entry["meta"]["innerInstructions"][0]["instructions"][1]["data"] = json!("0");

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(events.instructions.len(), 2);
        assert!(matches!(
            &events.instructions[1].result,
            Err(InstructionParseFailure {
                error: InstructionParseError::Protocol(ParseError::InvalidInstructionData(reason)),
                ..
            }) if reason == "test instruction has no immediate parent"
        ));
    }

    #[test]
    fn does_not_infer_a_parent_without_stack_height() {
        let mut entry = transaction(
            "missing-stack-height",
            vec![TEST_PROGRAM_ID, "parent-account"],
            vec![],
            vec![],
            vec![instruction(0, &[1], &[0], 1)],
            vec![json!({
                "index": 0,
                "instructions": [instruction(0, &[], &[4], 2)]
            })],
            Value::Null,
        );
        entry["meta"]["innerInstructions"][0]["instructions"][0]["stackHeight"] = Value::Null;

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert!(matches!(
            &events.instructions[0].result,
            Err(InstructionParseFailure {
                error: InstructionParseError::Protocol(ParseError::InvalidInstructionData(reason)),
                ..
            }) if reason == "test instruction has no immediate parent"
        ));
    }

    #[test]
    fn parses_a_real_nested_pumpswap_event_through_the_registry() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../programs/pump/pumpswap/tests/fixtures/buy_mainnet.json"
        ))
        .unwrap();
        let parent = &fixture["parent_instruction"];
        let event = &fixture["event_instruction"];
        let parent_accounts = parent["accounts"].as_array().unwrap();
        let event_accounts = event["accounts"].as_array().unwrap();
        let mut account_keys = vec!["AggregatorProgram", pumpswap::PROGRAM_ID];
        for account in parent_accounts.iter().chain(event_accounts) {
            let account = account.as_str().unwrap();
            if !account_keys.contains(&account) {
                account_keys.push(account);
            }
        }
        let account_index = |account: &Value| {
            account_keys
                .iter()
                .position(|key| *key == account.as_str().unwrap())
                .unwrap()
        };
        let parent_account_indexes = parent_accounts
            .iter()
            .map(account_index)
            .collect::<Vec<_>>();
        let event_account_indexes = event_accounts.iter().map(account_index).collect::<Vec<_>>();
        let parent_data = bs58::decode(parent["data"].as_str().unwrap())
            .into_vec()
            .unwrap();
        let event_data = bs58::decode(event["data"].as_str().unwrap())
            .into_vec()
            .unwrap();
        let entry = transaction(
            fixture["signature"].as_str().unwrap(),
            account_keys,
            vec![],
            vec![],
            vec![instruction(0, &[], &[], 1)],
            vec![json!({
                "index": 0,
                "instructions": [
                    instruction(1, &parent_account_indexes, &parent_data, 2),
                    instruction(1, &event_account_indexes, &event_data, 3),
                ]
            })],
            Value::Null,
        );
        let config = ParserConfig::new(vec![ParserName::PumpSwap]).unwrap();
        let parser = TransactionParser::new(InstructionDispatcher::new(
            ParserRegistry::from_config(&config).unwrap(),
        ));

        let events = parser.parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(events.instructions.len(), 1);
        assert_eq!(events.instructions[0].execution_ordinal, 2);
        assert_eq!(
            events.instructions[0].result,
            Ok(ParsedEvent::TokenSwap(TokenSwap {
                user: "8nLd2NbuoGnj4YKKjwRo7Xkhw55V2dhhR1RQqWo7fYeA".to_owned(),
                pool: "5tvUjENJmJie8HG8kuJsCjGgM1r6siRRiRSwnEcdte2b".to_owned(),
                input_mint: "EfwTuoSdbvrUpWTU2uWapBGNXCgjM1zo7Codpeq4yup3".to_owned(),
                input_amount: 8_411_309_631_679,
                output_mint: "So11111111111111111111111111111111111111112".to_owned(),
                output_amount: 12_823_278_553,
            }))
        );
    }

    #[test]
    fn records_malformed_instructions_and_continues_the_transaction() {
        let mut entry = transaction(
            "partial-signature",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[1], 1), instruction(0, &[], &[1], 1)],
            vec![],
            Value::Null,
        );
        entry["transaction"]["message"]["instructions"][0]["data"] = json!("0");

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(events.instructions.len(), 2);
        assert!(matches!(
            &events.instructions[0].result,
            Err(InstructionParseFailure {
                error: InstructionParseError::InvalidBase58Data(_),
                ..
            })
        ));
        assert_eq!(events.instructions[1].execution_ordinal, 1);
        assert_eq!(events.instructions[1].result, Ok(test_event()));
    }

    #[test]
    fn rejects_instruction_data_larger_than_a_solana_transaction() {
        let mut entry = transaction(
            "oversized-data",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[1], 1)],
            vec![],
            Value::Null,
        );
        entry["transaction"]["message"]["instructions"][0]["data"] =
            json!("1".repeat(MAX_BASE58_INSTRUCTION_DATA_LEN + 1));

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert_eq!(
            events.instructions[0].result,
            Err(InstructionParseFailure {
                program_id: Some(TEST_PROGRAM_ID.to_owned()),
                error: InstructionParseError::InstructionDataTooLong {
                    encoded_len: MAX_BASE58_INSTRUCTION_DATA_LEN + 1,
                    max_encoded_len: MAX_BASE58_INSTRUCTION_DATA_LEN,
                },
            })
        );
    }

    #[test]
    fn records_out_of_bounds_indexes_and_continues() {
        let entry = transaction(
            "index-signature",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(3, &[], &[1], 1), instruction(0, &[], &[1], 1)],
            vec![],
            Value::Null,
        );

        let events = parser().parse_transaction(&entry, 0).unwrap().unwrap();

        assert!(matches!(
            events.instructions[0].result,
            Err(InstructionParseFailure {
                program_id: None,
                error: InstructionParseError::IndexOutOfBounds {
                    field: "programIdIndex",
                    index: 3,
                    len: 1,
                },
            })
        ));
        assert_eq!(events.instructions[1].result, Ok(test_event()));
    }

    #[test]
    fn ignores_failed_transactions_before_dispatch() {
        let entry = transaction(
            "failed-signature",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[1], 1)],
            vec![],
            json!({"InstructionError": [0, {"Custom": 1}]}),
        );

        assert_eq!(parser().parse_transaction(&entry, 0).unwrap(), None);
    }

    #[test]
    fn rejects_malformed_transaction_structure() {
        let mut entry = transaction(
            "signature",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![],
            vec![],
            Value::Null,
        );
        entry["transaction"]["signatures"] = json!([]);

        assert_eq!(
            parser().parse_transaction(&entry, 0),
            Err(TransactionParseError::InvalidField {
                field: "transaction.signatures[0]",
                expected: "a string",
            })
        );

        let mut missing_loaded_readonly = transaction(
            "missing-loaded-readonly",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![],
            vec![],
            Value::Null,
        );
        missing_loaded_readonly["meta"]["loadedAddresses"]
            .as_object_mut()
            .unwrap()
            .remove("readonly");
        assert_eq!(
            parser().parse_transaction(&missing_loaded_readonly, 0),
            Err(TransactionParseError::InvalidField {
                field: "meta.loadedAddresses.readonly",
                expected: "an array of strings",
            })
        );
    }

    #[test]
    fn rejects_invalid_inner_instruction_groups() {
        let duplicate_groups = transaction(
            "duplicate-groups",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[1], 1)],
            vec![
                json!({"index": 0, "instructions": []}),
                json!({"index": 0, "instructions": []}),
            ],
            Value::Null,
        );
        assert_eq!(
            parser().parse_transaction(&duplicate_groups, 0),
            Err(TransactionParseError::DuplicateInnerInstructionGroup {
                outer_instruction_index: 0,
            })
        );

        let out_of_bounds_group = transaction(
            "out-of-bounds-group",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[1], 1)],
            vec![json!({"index": 1, "instructions": []})],
            Value::Null,
        );
        assert_eq!(
            parser().parse_transaction(&out_of_bounds_group, 0),
            Err(TransactionParseError::InnerInstructionGroupOutOfBounds {
                outer_instruction_index: 1,
                outer_instruction_count: 1,
            })
        );
    }

    #[test]
    fn accepts_empty_base58_instruction_data() {
        let entry = transaction(
            "empty-data-signature",
            vec![TEST_PROGRAM_ID],
            vec![],
            vec![],
            vec![instruction(0, &[], &[], 1)],
            vec![],
            Value::Null,
        );

        assert_eq!(parser().parse_transaction(&entry, 0).unwrap(), None);
    }
}
