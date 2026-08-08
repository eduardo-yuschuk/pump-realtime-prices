use common::{
    InstructionContext, InstructionParser, ParseError, ParseResult, ParsedEvent, TokenDiscovery,
};
use serde_json::{json, Value};

use crate::ParserRegistry;

pub const TEST_PROGRAM_ID: &str = "Test111111111111111111111111111111111111";

pub struct TestParser;

impl InstructionParser for TestParser {
    fn program_id(&self) -> &'static str {
        TEST_PROGRAM_ID
    }

    fn parse_instruction(
        &self,
        instruction: InstructionContext<'_>,
    ) -> ParseResult<Option<ParsedEvent>> {
        instruction.ensure_program_id(self.program_id())?;
        match instruction.data().first() {
            Some(1) => Ok(Some(test_event())),
            Some(2) => Err(ParseError::InvalidInstructionData(
                "test parser failure".to_owned(),
            )),
            Some(3) => Ok(Some(ParsedEvent::TokenDiscovery(TokenDiscovery {
                mint: instruction.account(0)?.to_owned(),
                creator: "creator".to_owned(),
                name: "Test Token".to_owned(),
                symbol: "TEST".to_owned(),
                uri: "https://example.com/token.json".to_owned(),
            }))),
            _ => Ok(None),
        }
    }
}

pub fn registry() -> ParserRegistry {
    ParserRegistry::from_parsers(vec![Box::new(TestParser)]).unwrap()
}

pub fn test_event() -> ParsedEvent {
    ParsedEvent::TokenDiscovery(TokenDiscovery {
        mint: "mint".to_owned(),
        creator: "creator".to_owned(),
        name: "Test Token".to_owned(),
        symbol: "TEST".to_owned(),
        uri: "https://example.com/token.json".to_owned(),
    })
}

pub fn instruction(
    program_id_index: usize,
    accounts: &[usize],
    data: &[u8],
    stack_height: u32,
) -> Value {
    json!({
        "accounts": accounts,
        "data": bs58::encode(data).into_string(),
        "programIdIndex": program_id_index,
        "stackHeight": stack_height,
    })
}

pub fn transaction(
    signature: &str,
    static_keys: Vec<&str>,
    loaded_writable: Vec<&str>,
    loaded_readonly: Vec<&str>,
    instructions: Vec<Value>,
    inner_instructions: Vec<Value>,
    error: Value,
) -> Value {
    json!({
        "meta": {
            "err": error,
            "innerInstructions": inner_instructions,
            "loadedAddresses": {
                "writable": loaded_writable,
                "readonly": loaded_readonly,
            }
        },
        "transaction": {
            "message": {
                "accountKeys": static_keys,
                "instructions": instructions,
            },
            "signatures": [signature],
        },
        "version": 0,
    })
}
