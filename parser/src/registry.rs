use std::collections::BTreeMap;

use common::InstructionParser;

use crate::{ParserConfig, ParserName, RegistryError};

pub struct ParserRegistry {
    parsers: BTreeMap<&'static str, Box<dyn InstructionParser>>,
}

impl ParserRegistry {
    pub fn from_config(config: &ParserConfig) -> Result<Self, RegistryError> {
        let parsers = config
            .parsers()
            .iter()
            .copied()
            .map(create_parser)
            .collect();
        Self::from_parsers(parsers)
    }

    pub fn from_parsers(parsers: Vec<Box<dyn InstructionParser>>) -> Result<Self, RegistryError> {
        let mut registry = BTreeMap::new();
        for parser in parsers {
            let program_id = parser.program_id();
            if registry.insert(program_id, parser).is_some() {
                return Err(RegistryError::DuplicateProgramId {
                    program_id: program_id.to_owned(),
                });
            }
        }
        Ok(Self { parsers: registry })
    }

    pub fn get(&self, program_id: &str) -> Option<&dyn InstructionParser> {
        self.parsers.get(program_id).map(Box::as_ref)
    }

    pub fn contains(&self, program_id: &str) -> bool {
        self.parsers.contains_key(program_id)
    }

    pub fn len(&self) -> usize {
        self.parsers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parsers.is_empty()
    }
}

fn create_parser(name: ParserName) -> Box<dyn InstructionParser> {
    match name {
        ParserName::PumpFun => Box::new(pumpfun::PumpFunParser),
        ParserName::PumpSwap => Box::new(pumpswap::PumpSwapParser),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support::TestParser, ParserName};

    #[test]
    fn builds_every_configured_parser_factory() {
        let config = ParserConfig::new(vec![ParserName::PumpFun, ParserName::PumpSwap]).unwrap();
        let registry = ParserRegistry::from_config(&config).unwrap();

        assert_eq!(registry.len(), 2);
        assert!(registry.contains(pumpfun::PROGRAM_ID));
        assert!(registry.contains(pumpswap::PROGRAM_ID));
        assert!(!registry.is_empty());
    }

    #[test]
    fn supports_injection_and_rejects_duplicate_program_ids() {
        let registry = ParserRegistry::from_parsers(vec![Box::new(TestParser)]).unwrap();
        assert!(registry.get(TestParser.program_id()).is_some());

        assert!(matches!(
            ParserRegistry::from_parsers(vec![Box::new(TestParser), Box::new(TestParser)]),
            Err(RegistryError::DuplicateProgramId { program_id })
                if program_id == TestParser.program_id()
        ));
    }
}
