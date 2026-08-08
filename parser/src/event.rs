use common::ParsedEvent;

use crate::InstructionParseFailure;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockEvents {
    pub transactions: Vec<TransactionEvents>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionEvents {
    pub signature: String,
    pub transaction_index: usize,
    pub instructions: Vec<InstructionEvents>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionEvents {
    pub program_id: Option<String>,
    pub outer_instruction_index: usize,
    pub inner_instruction_index: Option<usize>,
    pub stack_height: Option<u32>,
    pub execution_ordinal: usize,
    pub result: Result<ParsedEvent, InstructionParseFailure>,
}
