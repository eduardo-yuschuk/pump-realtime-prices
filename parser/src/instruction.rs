use common::{InstructionContext, ParsedEvent};

use crate::{InstructionParseError, InstructionParseFailure, ParserRegistry};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    NoEvent,
    Event(ParsedEvent),
    Failure(InstructionParseFailure),
}

pub struct InstructionDispatcher {
    registry: ParserRegistry,
}

impl InstructionDispatcher {
    pub const fn new(registry: ParserRegistry) -> Self {
        Self { registry }
    }

    pub fn is_configured(&self, program_id: &str) -> bool {
        self.registry.contains(program_id)
    }

    pub fn dispatch(&self, instruction: InstructionContext<'_>) -> DispatchOutcome {
        let Some(parser) = self.registry.get(instruction.program_id()) else {
            return DispatchOutcome::NoEvent;
        };

        match parser.parse_instruction(instruction) {
            Ok(Some(event)) => DispatchOutcome::Event(event),
            Ok(None) => DispatchOutcome::NoEvent,
            Err(error) => DispatchOutcome::Failure(InstructionParseFailure {
                program_id: Some(instruction.program_id().to_owned()),
                error: InstructionParseError::Protocol(error),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use common::ParseError;

    use super::*;
    use crate::test_support::{registry, test_event, TEST_PROGRAM_ID};

    #[test]
    fn dispatches_events_none_and_recoverable_failures() {
        let dispatcher = InstructionDispatcher::new(registry());

        assert_eq!(
            dispatcher.dispatch(InstructionContext::new(TEST_PROGRAM_ID, &[], &[1])),
            DispatchOutcome::Event(test_event())
        );
        assert_eq!(
            dispatcher.dispatch(InstructionContext::new(TEST_PROGRAM_ID, &[], &[0])),
            DispatchOutcome::NoEvent
        );
        assert_eq!(
            dispatcher.dispatch(InstructionContext::new(TEST_PROGRAM_ID, &[], &[2])),
            DispatchOutcome::Failure(InstructionParseFailure {
                program_id: Some(TEST_PROGRAM_ID.to_owned()),
                error: InstructionParseError::Protocol(ParseError::InvalidInstructionData(
                    "test parser failure".to_owned()
                )),
            })
        );
    }

    #[test]
    fn ignores_unconfigured_programs() {
        let dispatcher = InstructionDispatcher::new(registry());
        let event_data = [1];

        assert!(!dispatcher.is_configured("OtherProgram"));
        assert_eq!(
            dispatcher.dispatch(InstructionContext::new("OtherProgram", &[], &event_data)),
            DispatchOutcome::NoEvent
        );
    }
}
