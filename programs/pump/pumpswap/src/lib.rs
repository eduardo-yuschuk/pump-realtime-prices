//! PumpSwap program integration.
//!
//! The parser is based on the IDL the program publishes on chain, stored as
//! `new_idl.json` next to this crate and read on 2026-09-22. That IDL is behind
//! the deployed program: observed payloads carry 41 undeclared trailing bytes.
//! See `doc/protocol_parsers.md`.
//!
//! Events are decoded up to the last field consumed here and any trailing
//! bytes are ignored, so fields appended by a protocol upgrade do not stop the
//! swap from being parsed.
//!
//! PumpSwap emits Anchor events through self-CPI instructions. A buy or sell
//! event is only authoritative when it matches its immediate PumpSwap parent,
//! so outer instructions and uncorrelated event CPIs do not produce swaps.

use std::str;

use borsh::BorshDeserialize;
use common::{
    decode_event_prefix, InstructionContext, InstructionParser, ParseError, ParseResult,
    ParsedEvent, TokenSwap,
};
use solana_pubkey::Pubkey;

/// PumpSwap program address on Solana mainnet and devnet.
pub const PROGRAM_ID: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";

const EVENT_AUTHORITY: &str = "GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR";
const EVENT_IX_TAG: [u8; 8] = [228, 69, 165, 46, 81, 203, 154, 29];
const BUY_EVENT_DISCRIMINATOR: [u8; 8] = [103, 244, 82, 31, 44, 245, 119, 119];
const SELL_EVENT_DISCRIMINATOR: [u8; 8] = [62, 47, 55, 10, 165, 3, 220, 42];
const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
const BUY_EXACT_QUOTE_IN_DISCRIMINATOR: [u8; 8] = [198, 46, 21, 82, 180, 217, 232, 112];
const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];
const EVENT_HEADER_LEN: usize = EVENT_IX_TAG.len() + BUY_EVENT_DISCRIMINATOR.len();

const POOL_ACCOUNT_INDEX: usize = 0;
const USER_ACCOUNT_INDEX: usize = 1;
const BASE_MINT_ACCOUNT_INDEX: usize = 3;
const QUOTE_MINT_ACCOUNT_INDEX: usize = 4;
const BUY_PARENT_ACCOUNT_COUNT: usize = 23;
const SELL_PARENT_ACCOUNT_COUNT: usize = 21;
const BUY_PARENT_DATA_LEN: usize = 25;
const BUY_EXACT_QUOTE_IN_PARENT_DATA_LEN: usize = 24;
const SELL_PARENT_DATA_LEN: usize = 24;

/// Parses correlated PumpSwap buy and sell event-CPI instructions.
#[derive(Clone, Copy, Debug, Default)]
pub struct PumpSwapParser;

impl InstructionParser for PumpSwapParser {
    fn program_id(&self) -> &'static str {
        PROGRAM_ID
    }

    fn parse_instruction(
        &self,
        instruction: InstructionContext<'_>,
    ) -> ParseResult<Option<ParsedEvent>> {
        instruction.ensure_program_id(self.program_id())?;
        let parent = instruction.parent_instruction();
        parse_instruction_with_parent(instruction, parent)
    }
}

fn parse_instruction_with_parent(
    instruction: InstructionContext<'_>,
    parent: Option<InstructionContext<'_>>,
) -> ParseResult<Option<ParsedEvent>> {
    if !instruction.data().starts_with(&EVENT_IX_TAG) {
        return Ok(None);
    }

    instruction.ensure_data_len(EVENT_HEADER_LEN)?;
    let discriminator = &instruction.data()[EVENT_IX_TAG.len()..EVENT_HEADER_LEN];
    if discriminator != BUY_EVENT_DISCRIMINATOR && discriminator != SELL_EVENT_DISCRIMINATOR {
        return Ok(None);
    }

    if instruction.account(0)? != EVENT_AUTHORITY {
        return Err(ParseError::InvalidInstructionData(
            "PumpSwap event CPI has an invalid event authority".to_owned(),
        ));
    }

    let parent = parent.ok_or_else(|| {
        ParseError::InvalidInstructionData(
            "PumpSwap event CPI has no immediate parent instruction".to_owned(),
        )
    })?;
    validate_parent_program(parent)?;

    let payload = &instruction.data()[EVENT_HEADER_LEN..];
    match discriminator {
        value if value == BUY_EVENT_DISCRIMINATOR => {
            parse_buy_event(payload, parent).map(|swap| Some(ParsedEvent::TokenSwap(swap)))
        }
        value if value == SELL_EVENT_DISCRIMINATOR => {
            parse_sell_event(payload, parent).map(|swap| Some(ParsedEvent::TokenSwap(swap)))
        }
        _ => unreachable!(),
    }
}

fn validate_parent_program(parent: InstructionContext<'_>) -> ParseResult<()> {
    if parent.program_id() == PROGRAM_ID {
        Ok(())
    } else {
        Err(ParseError::InvalidInstructionData(format!(
            "PumpSwap event CPI parent targets program {}",
            parent.program_id()
        )))
    }
}

/// Leading `BuyEvent` fields, up to the last one this parser consumes.
///
/// Everything the program appends after `ix_name` is deliberately left out:
/// the payload observed on chain already carries fields the published IDL does
/// not declare, and the protocol keeps adding more.
#[derive(BorshDeserialize)]
#[allow(dead_code)]
struct BuyEventPrefix {
    timestamp: i64,
    base_amount_out: u64,
    max_quote_amount_in: u64,
    user_base_token_reserves: u64,
    user_quote_token_reserves: u64,
    pool_base_token_reserves: u64,
    pool_quote_token_reserves: u64,
    quote_amount_in: u64,
    lp_fee_basis_points: u64,
    lp_fee: u64,
    protocol_fee_basis_points: u64,
    protocol_fee: u64,
    quote_amount_in_with_lp_fee: u64,
    user_quote_amount_in: u64,
    pool: Pubkey,
    user: Pubkey,
    user_base_token_account: Pubkey,
    user_quote_token_account: Pubkey,
    protocol_fee_recipient: Pubkey,
    protocol_fee_recipient_token_account: Pubkey,
    coin_creator: Pubkey,
    coin_creator_fee_basis_points: u64,
    coin_creator_fee: u64,
    track_volume: bool,
    total_unclaimed_tokens: u64,
    total_claimed_tokens: u64,
    current_sol_volume: u64,
    last_update_timestamp: i64,
    min_base_amount_out: u64,
    ix_name: String,
}

/// Leading `SellEvent` fields, up to the last one this parser consumes.
#[derive(BorshDeserialize)]
#[allow(dead_code)]
struct SellEventPrefix {
    timestamp: i64,
    base_amount_in: u64,
    min_quote_amount_out: u64,
    user_base_token_reserves: u64,
    user_quote_token_reserves: u64,
    pool_base_token_reserves: u64,
    pool_quote_token_reserves: u64,
    quote_amount_out: u64,
    lp_fee_basis_points: u64,
    lp_fee: u64,
    protocol_fee_basis_points: u64,
    protocol_fee: u64,
    quote_amount_out_without_lp_fee: u64,
    user_quote_amount_out: u64,
    pool: Pubkey,
    user: Pubkey,
}

fn parse_buy_event(payload: &[u8], parent: InstructionContext<'_>) -> ParseResult<TokenSwap> {
    let event: BuyEventPrefix = decode_event_prefix(payload, "BuyEvent")?;

    let input_amount = match parent_discriminator(parent)? {
        value if value == BUY_DISCRIMINATOR => {
            validate_parent_layout(parent, BUY_PARENT_DATA_LEN, BUY_PARENT_ACCOUNT_COUNT)?;
            validate_ix_name(&event.ix_name, "buy")?;
            validate_parent_amount(parent, event.base_amount_out, "BuyEvent.base_amount_out")?;
            event.user_quote_amount_in
        }
        value if value == BUY_EXACT_QUOTE_IN_DISCRIMINATOR => {
            // A successful mainnet instruction at slot 438131164 omits the
            // trailing OptionBool present in the current IDL.
            validate_parent_layout(
                parent,
                BUY_EXACT_QUOTE_IN_PARENT_DATA_LEN,
                BUY_PARENT_ACCOUNT_COUNT,
            )?;
            validate_ix_name(&event.ix_name, "buy_exact_quote_in")?;
            validate_parent_amount(parent, event.quote_amount_in, "BuyEvent.quote_amount_in")?;
            event.quote_amount_in
        }
        _ => {
            return Err(ParseError::InvalidInstructionData(
                "BuyEvent parent is not a supported PumpSwap buy instruction".to_owned(),
            ));
        }
    };
    validate_parent_accounts(event.pool, event.user, parent, "BuyEvent")?;
    let base_mint = parent_mint(parent, BASE_MINT_ACCOUNT_INDEX, "base_mint")?;
    let quote_mint = parent_mint(parent, QUOTE_MINT_ACCOUNT_INDEX, "quote_mint")?;

    Ok(TokenSwap {
        user: event.user.to_string(),
        pool: event.pool.to_string(),
        base_mint: Some(base_mint.clone()),
        quote_mint: Some(quote_mint.clone()),
        input_mint: quote_mint,
        input_amount,
        output_mint: base_mint,
        output_amount: event.base_amount_out,
    })
}

fn parse_sell_event(payload: &[u8], parent: InstructionContext<'_>) -> ParseResult<TokenSwap> {
    let event: SellEventPrefix = decode_event_prefix(payload, "SellEvent")?;

    if parent_discriminator(parent)? != SELL_DISCRIMINATOR {
        return Err(ParseError::InvalidInstructionData(
            "SellEvent parent is not a PumpSwap sell instruction".to_owned(),
        ));
    }
    validate_parent_layout(parent, SELL_PARENT_DATA_LEN, SELL_PARENT_ACCOUNT_COUNT)?;
    validate_parent_amount(parent, event.base_amount_in, "SellEvent.base_amount_in")?;
    validate_parent_accounts(event.pool, event.user, parent, "SellEvent")?;
    let base_mint = parent_mint(parent, BASE_MINT_ACCOUNT_INDEX, "base_mint")?;
    let quote_mint = parent_mint(parent, QUOTE_MINT_ACCOUNT_INDEX, "quote_mint")?;

    Ok(TokenSwap {
        user: event.user.to_string(),
        pool: event.pool.to_string(),
        base_mint: Some(base_mint.clone()),
        quote_mint: Some(quote_mint.clone()),
        input_mint: base_mint,
        input_amount: event.base_amount_in,
        output_mint: quote_mint,
        output_amount: event.user_quote_amount_out,
    })
}

fn parent_discriminator(parent: InstructionContext<'_>) -> ParseResult<[u8; 8]> {
    parent.ensure_data_len(8)?;
    parent.data()[..8].try_into().map_err(|_| {
        ParseError::InvalidInstructionData(
            "PumpSwap parent instruction has an invalid discriminator".to_owned(),
        )
    })
}

fn validate_parent_layout(
    parent: InstructionContext<'_>,
    minimum_data_len: usize,
    minimum_account_count: usize,
) -> ParseResult<()> {
    parent.ensure_data_len(minimum_data_len)?;
    parent.account(minimum_account_count - 1)?;
    Ok(())
}

fn validate_parent_amount(
    parent: InstructionContext<'_>,
    event_amount: u64,
    field: &str,
) -> ParseResult<()> {
    let parent_amount = u64::from_le_bytes(parent.data()[8..16].try_into().map_err(|_| {
        ParseError::InvalidInstructionData(
            "PumpSwap parent instruction has an invalid amount".to_owned(),
        )
    })?);
    if parent_amount == event_amount {
        Ok(())
    } else {
        Err(ParseError::InvalidInstructionData(format!(
            "{field} does not match its parent instruction"
        )))
    }
}

fn parent_mint(
    parent: InstructionContext<'_>,
    account_index: usize,
    account_name: &str,
) -> ParseResult<String> {
    let address = parent.account(account_index)?;
    Pubkey::try_from(address)
        .map(|pubkey| pubkey.to_string())
        .map_err(|error| {
            ParseError::InvalidInstructionData(format!(
                "PumpSwap parent {account_name} is not a valid address: {error}"
            ))
        })
}

fn validate_parent_accounts(
    event_pool: Pubkey,
    event_user: Pubkey,
    parent: InstructionContext<'_>,
    event_name: &str,
) -> ParseResult<()> {
    if event_pool.to_string() != parent.account(POOL_ACCOUNT_INDEX)? {
        return Err(ParseError::InvalidInstructionData(format!(
            "{event_name}.pool does not match its parent instruction"
        )));
    }
    if event_user.to_string() != parent.account(USER_ACCOUNT_INDEX)? {
        return Err(ParseError::InvalidInstructionData(format!(
            "{event_name}.user does not match its parent instruction"
        )));
    }

    Ok(())
}

fn validate_ix_name(actual: &str, expected: &str) -> ParseResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(ParseError::InvalidInstructionData(format!(
            "BuyEvent.ix_name is {actual:?}, expected {expected:?} for its parent instruction"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const SIGNATURE: &str =
        "31FxUQgkNxdcMzcZpuPkw4B32Vg37YMCNiMqcgNeRzF1jH9FHKxsUa6RRHFn3ekYxRhu8EFwgrkNGP1V1ofXjzL1";
    const BASE_MINT: &str = "So11111111111111111111111111111111111111112";
    const QUOTE_MINT: &str = "EfwTuoSdbvrUpWTU2uWapBGNXCgjM1zo7Codpeq4yup3";
    const POOL: &str = "5tvUjENJmJie8HG8kuJsCjGgM1r6siRRiRSwnEcdte2b";
    const BUY_IX_NAME_LENGTH_OFFSET: usize = EVENT_HEADER_LEN + 393;
    const BUY_TRACK_VOLUME_OFFSET: usize = EVENT_HEADER_LEN + 352;

    struct Fixture {
        metadata: Value,
        event_program_id: String,
        event_accounts: Vec<String>,
        event_data: Vec<u8>,
        parent_program_id: String,
        parent_accounts: Vec<String>,
        parent_data: Vec<u8>,
    }

    fn load_fixture(source: &str) -> Fixture {
        let metadata: Value = serde_json::from_str(source).unwrap();
        let decode_instruction = |name: &str| {
            let instruction = &metadata[name];
            let program_id = instruction["program_id"].as_str().unwrap().to_owned();
            let accounts = instruction["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|account| account.as_str().unwrap().to_owned())
                .collect();
            let data = bs58::decode(instruction["data"].as_str().unwrap())
                .into_vec()
                .unwrap();
            (program_id, accounts, data)
        };
        let (event_program_id, event_accounts, event_data) =
            decode_instruction("event_instruction");
        let (parent_program_id, parent_accounts, parent_data) =
            decode_instruction("parent_instruction");

        Fixture {
            metadata,
            event_program_id,
            event_accounts,
            event_data,
            parent_program_id,
            parent_accounts,
            parent_data,
        }
    }

    fn parse_parts(
        event_program_id: &str,
        event_accounts: &[String],
        event_data: &[u8],
        parent_program_id: &str,
        parent_accounts: &[String],
        parent_data: &[u8],
    ) -> ParseResult<Option<ParsedEvent>> {
        let event_accounts: Vec<&str> = event_accounts.iter().map(String::as_str).collect();
        let parent_accounts: Vec<&str> = parent_accounts.iter().map(String::as_str).collect();
        let parent = InstructionContext::new(parent_program_id, &parent_accounts, parent_data);
        let event = InstructionContext::new(event_program_id, &event_accounts, event_data)
            .with_parent_instruction(parent);
        PumpSwapParser.parse_instruction(event)
    }

    fn parse_fixture(fixture: &Fixture) -> ParseResult<Option<ParsedEvent>> {
        parse_parts(
            &fixture.event_program_id,
            &fixture.event_accounts,
            &fixture.event_data,
            &fixture.parent_program_id,
            &fixture.parent_accounts,
            &fixture.parent_data,
        )
    }

    fn replace_buy_ix_name(data: &mut Vec<u8>, name: &[u8]) {
        let old_length = u32::from_le_bytes(
            data[BUY_IX_NAME_LENGTH_OFFSET..BUY_IX_NAME_LENGTH_OFFSET + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let mut replacement = (name.len() as u32).to_le_bytes().to_vec();
        replacement.extend_from_slice(name);
        data.splice(
            BUY_IX_NAME_LENGTH_OFFSET..BUY_IX_NAME_LENGTH_OFFSET + 4 + old_length,
            replacement,
        );
    }

    fn assert_invalid(result: ParseResult<Option<ParsedEvent>>, expected: &str) {
        assert!(
            matches!(&result, Err(ParseError::InvalidInstructionData(reason)) if reason.contains(expected)),
            "expected invalid instruction data containing {expected:?}, got {result:?}"
        );
    }

    #[test]
    fn parses_the_real_mainnet_buy() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));

        assert_eq!(fixture.metadata["slot"], 438130307);
        assert_eq!(fixture.metadata["signature"], SIGNATURE);
        assert_eq!(fixture.metadata["outer_instruction_index"], 10);
        assert_eq!(fixture.metadata["inner_instruction_index"], 5);
        assert_eq!(
            parse_fixture(&fixture).unwrap(),
            Some(ParsedEvent::TokenSwap(TokenSwap {
                user: "8nLd2NbuoGnj4YKKjwRo7Xkhw55V2dhhR1RQqWo7fYeA".to_owned(),
                pool: POOL.to_owned(),
                base_mint: Some(BASE_MINT.to_owned()),
                quote_mint: Some(QUOTE_MINT.to_owned()),
                input_mint: QUOTE_MINT.to_owned(),
                input_amount: 8_411_309_631_679,
                output_mint: BASE_MINT.to_owned(),
                output_amount: 12_823_278_553,
            }))
        );
    }

    #[test]
    fn parses_the_real_mainnet_sell() {
        let fixture = load_fixture(include_str!("../tests/fixtures/sell_mainnet.json"));

        assert_eq!(fixture.metadata["slot"], 438130307);
        assert_eq!(fixture.metadata["signature"], SIGNATURE);
        assert_eq!(fixture.metadata["outer_instruction_index"], 6);
        assert_eq!(fixture.metadata["inner_instruction_index"], 5);
        assert_eq!(
            parse_fixture(&fixture).unwrap(),
            Some(ParsedEvent::TokenSwap(TokenSwap {
                user: "4iPmppqTTkEaokWw9iKCBU8YaN55gVfnSCkkqfa4aQFf".to_owned(),
                pool: POOL.to_owned(),
                base_mint: Some(BASE_MINT.to_owned()),
                quote_mint: Some(QUOTE_MINT.to_owned()),
                input_mint: BASE_MINT.to_owned(),
                input_amount: 12_334_640_759,
                output_mint: QUOTE_MINT.to_owned(),
                output_amount: 8_039_744_516_239,
            }))
        );
    }

    #[test]
    fn parses_the_real_mainnet_buy_exact_quote_in() {
        let fixture = load_fixture(include_str!(
            "../tests/fixtures/buy_exact_quote_in_mainnet.json"
        ));

        assert_eq!(fixture.metadata["slot"], 438131164);
        assert_eq!(fixture.metadata["outer_instruction_index"], 7);
        assert_eq!(fixture.metadata["inner_instruction_index"], 7);
        assert_eq!(
            parse_fixture(&fixture).unwrap(),
            Some(ParsedEvent::TokenSwap(TokenSwap {
                user: "JBAq4zbH46ApCAxte6hmXGdk3aH5ZXaGCjUqh2LyYKck".to_owned(),
                pool: "BimKw2yfEvAmwYFTy6jGXVsT7A8aCuquxjqJW5i9jv7K".to_owned(),
                base_mint: Some("HFJXAF8yKgPD43xsyb2cd433hq9nb7jkVbeGji3spump".to_owned()),
                quote_mint: Some(BASE_MINT.to_owned()),
                input_mint: BASE_MINT.to_owned(),
                input_amount: 23_000_000,
                output_mint: "HFJXAF8yKgPD43xsyb2cd433hq9nb7jkVbeGji3spump".to_owned(),
                output_amount: 81_884_012_831,
            }))
        );
    }

    #[test]
    fn returns_none_for_outer_and_unmodeled_instructions() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        let accounts: Vec<&str> = fixture.parent_accounts.iter().map(String::as_str).collect();
        let outer = InstructionContext::new(PROGRAM_ID, &accounts, &fixture.parent_data);
        assert_eq!(PumpSwapParser.parse_instruction(outer).unwrap(), None);

        let mut unmodeled = EVENT_IX_TAG.to_vec();
        unmodeled.extend_from_slice(&[0; 8]);
        let event_accounts = [EVENT_AUTHORITY];
        let event = InstructionContext::new(PROGRAM_ID, &event_accounts, &unmodeled);
        assert_eq!(PumpSwapParser.parse_instruction(event).unwrap(), None);
    }

    #[test]
    fn rejects_an_event_for_the_wrong_program() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        let accounts: Vec<&str> = fixture.event_accounts.iter().map(String::as_str).collect();
        let instruction = InstructionContext::new("WrongProgram", &accounts, &fixture.event_data);

        assert_eq!(
            PumpSwapParser.parse_instruction(instruction),
            Err(ParseError::ProgramMismatch {
                expected: PROGRAM_ID,
                actual: "WrongProgram".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_an_event_with_no_parent() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        let accounts: Vec<&str> = fixture.event_accounts.iter().map(String::as_str).collect();
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &fixture.event_data);

        assert_invalid(
            PumpSwapParser.parse_instruction(instruction),
            "no immediate parent",
        );
    }

    #[test]
    fn rejects_the_wrong_event_authority() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        let accounts = ["WrongAuthority".to_owned()];

        assert_invalid(
            parse_parts(
                &fixture.event_program_id,
                &accounts,
                &fixture.event_data,
                &fixture.parent_program_id,
                &fixture.parent_accounts,
                &fixture.parent_data,
            ),
            "invalid event authority",
        );
    }

    #[test]
    fn rejects_missing_event_and_parent_accounts() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        assert_eq!(
            parse_parts(
                &fixture.event_program_id,
                &[],
                &fixture.event_data,
                &fixture.parent_program_id,
                &fixture.parent_accounts,
                &fixture.parent_data,
            ),
            Err(ParseError::MissingAccounts {
                expected_at_least: 1,
                actual: 0,
            })
        );

        assert_eq!(
            parse_parts(
                &fixture.event_program_id,
                &fixture.event_accounts,
                &fixture.event_data,
                &fixture.parent_program_id,
                &fixture.parent_accounts[..QUOTE_MINT_ACCOUNT_INDEX],
                &fixture.parent_data,
            ),
            Err(ParseError::MissingAccounts {
                expected_at_least: BUY_PARENT_ACCOUNT_COUNT,
                actual: QUOTE_MINT_ACCOUNT_INDEX,
            })
        );
    }

    #[test]
    fn rejects_a_parent_for_the_wrong_program() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));

        assert_invalid(
            parse_parts(
                &fixture.event_program_id,
                &fixture.event_accounts,
                &fixture.event_data,
                "WrongParentProgram",
                &fixture.parent_accounts,
                &fixture.parent_data,
            ),
            "parent targets program",
        );
    }

    #[test]
    fn rejects_wrong_or_truncated_parent_discriminators() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        let wrong = [0; 8];
        assert_invalid(
            parse_parts(
                &fixture.event_program_id,
                &fixture.event_accounts,
                &fixture.event_data,
                &fixture.parent_program_id,
                &fixture.parent_accounts,
                &wrong,
            ),
            "not a supported PumpSwap buy",
        );

        assert_eq!(
            parse_parts(
                &fixture.event_program_id,
                &fixture.event_accounts,
                &fixture.event_data,
                &fixture.parent_program_id,
                &fixture.parent_accounts,
                &[1, 2, 3],
            ),
            Err(ParseError::DataTooShort {
                expected_at_least: 8,
                actual: 3,
            })
        );

        let discriminator_only = BUY_DISCRIMINATOR;
        assert_eq!(
            parse_parts(
                &fixture.event_program_id,
                &fixture.event_accounts,
                &fixture.event_data,
                &fixture.parent_program_id,
                &fixture.parent_accounts,
                &discriminator_only,
            ),
            Err(ParseError::DataTooShort {
                expected_at_least: BUY_PARENT_DATA_LEN,
                actual: BUY_DISCRIMINATOR.len(),
            })
        );
    }

    #[test]
    fn rejects_an_event_type_that_does_not_match_its_parent() {
        let buy = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        let sell = load_fixture(include_str!("../tests/fixtures/sell_mainnet.json"));

        assert_invalid(
            parse_parts(
                &buy.event_program_id,
                &buy.event_accounts,
                &buy.event_data,
                &sell.parent_program_id,
                &sell.parent_accounts,
                &sell.parent_data,
            ),
            "not a supported PumpSwap buy",
        );
        assert_invalid(
            parse_parts(
                &sell.event_program_id,
                &sell.event_accounts,
                &sell.event_data,
                &buy.parent_program_id,
                &buy.parent_accounts,
                &buy.parent_data,
            ),
            "not a PumpSwap sell",
        );
    }

    #[test]
    fn rejects_pool_and_user_mismatches() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        for (index, expected) in [
            (POOL_ACCOUNT_INDEX, "BuyEvent.pool"),
            (USER_ACCOUNT_INDEX, "BuyEvent.user"),
        ] {
            let mut accounts = fixture.parent_accounts.clone();
            accounts[index] = "Mismatch111111111111111111111111111111111".to_owned();
            assert_invalid(
                parse_parts(
                    &fixture.event_program_id,
                    &fixture.event_accounts,
                    &fixture.event_data,
                    &fixture.parent_program_id,
                    &accounts,
                    &fixture.parent_data,
                ),
                expected,
            );
        }
    }

    #[test]
    fn rejects_invalid_parent_mint_addresses() {
        let fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        for index in [BASE_MINT_ACCOUNT_INDEX, QUOTE_MINT_ACCOUNT_INDEX] {
            let mut accounts = fixture.parent_accounts.clone();
            accounts[index] = "not-a-pubkey".to_owned();
            assert_invalid(
                parse_parts(
                    &fixture.event_program_id,
                    &fixture.event_accounts,
                    &fixture.event_data,
                    &fixture.parent_program_id,
                    &accounts,
                    &fixture.parent_data,
                ),
                "is not a valid address",
            );
        }
    }

    #[test]
    fn rejects_a_buy_ix_name_that_does_not_match_its_parent_variant() {
        let mut fixture = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        replace_buy_ix_name(&mut fixture.event_data, b"buy_exact_quote_in");

        assert_invalid(parse_fixture(&fixture), "expected \"buy\"");
    }

    #[test]
    fn rejects_every_truncated_real_event_payload() {
        for fixture in [
            load_fixture(include_str!("../tests/fixtures/buy_mainnet.json")),
            load_fixture(include_str!(
                "../tests/fixtures/buy_exact_quote_in_mainnet.json"
            )),
            load_fixture(include_str!("../tests/fixtures/sell_mainnet.json")),
        ] {
            // Only truncations that cut into the modeled prefix are
            // detectable: bytes past it are ignored on purpose, so a payload
            // cut after the prefix is indistinguishable from a shorter
            // protocol version.
            let prefix_len = EVENT_HEADER_LEN + modeled_prefix_len(&fixture.event_data);
            for length in EVENT_HEADER_LEN..prefix_len {
                let result = parse_parts(
                    &fixture.event_program_id,
                    &fixture.event_accounts,
                    &fixture.event_data[..length],
                    &fixture.parent_program_id,
                    &fixture.parent_accounts,
                    &fixture.parent_data,
                );
                assert!(
                    result.is_err(),
                    "length {length} unexpectedly produced {result:?}"
                );
            }
        }
    }

    #[test]
    fn rejects_a_truncated_event_header() {
        let accounts = [EVENT_AUTHORITY];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &EVENT_IX_TAG);

        assert_eq!(
            parse_instruction_with_parent(instruction, None),
            Err(ParseError::DataTooShort {
                expected_at_least: EVENT_HEADER_LEN,
                actual: EVENT_IX_TAG.len(),
            })
        );
    }

    #[test]
    fn rejects_invalid_borsh_booleans() {
        let mut buy = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        buy.event_data[BUY_TRACK_VOLUME_OFFSET] = 2;
        assert_invalid(parse_fixture(&buy), "Invalid bool representation");
    }

    #[test]
    fn rejects_invalid_borsh_strings() {
        let mut invalid_utf8 = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        replace_buy_ix_name(&mut invalid_utf8.event_data, &[0xff]);
        assert_invalid(parse_fixture(&invalid_utf8), "invalid utf-8 sequence");

        let mut invalid_length = load_fixture(include_str!("../tests/fixtures/buy_mainnet.json"));
        invalid_length.event_data[BUY_IX_NAME_LENGTH_OFFSET..BUY_IX_NAME_LENGTH_OFFSET + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        assert_invalid(parse_fixture(&invalid_length), "Unexpected length of input");
    }

    #[test]
    fn parses_real_mainnet_events_with_holder_reward_fields() {
        // Captured after the protocol appended `holder_rewards_bps` and
        // `holder_rewards`, which the published IDL does not declare.
        let buy = load_fixture(include_str!(
            "../tests/fixtures/buy_exact_quote_in_holder_rewards_mainnet.json"
        ));
        assert_eq!(buy.metadata["slot"], 449_427_600);

        let swap = parse_fixture(&buy).unwrap().unwrap();
        let ParsedEvent::TokenSwap(swap) = swap else {
            panic!("expected a token swap");
        };
        assert_eq!(swap.pool, "2uF4Xh61rDwxnG9woyxsVQP7zuA6kLFpb3NvnRQeoiSd");
        assert_eq!(swap.user, "1XraKHzVSjixEhzNph2aEdXX4sxJ6iYRWVRfCFjzNoP");
        assert_eq!(swap.output_amount, 7_844_434_277);

        let sell = load_fixture(include_str!(
            "../tests/fixtures/sell_holder_rewards_mainnet.json"
        ));
        let swap = parse_fixture(&sell).unwrap().unwrap();
        let ParsedEvent::TokenSwap(swap) = swap else {
            panic!("expected a token swap");
        };
        assert_eq!(swap.pool, "2iUGkVrpsw3b39pNWRPLhyjzMp4vG9PCj8pKyUnoXpgA");
        assert_eq!(swap.user, "5Q7sU7RaR6XNcztFRzDgyDRod2CPmiuq8apJp6DpjsUM");
    }

    #[test]
    fn parses_events_carrying_fields_appended_by_protocol_upgrades() {
        // Both programs appended `holder_rewards_bps` and `holder_rewards`
        // after these fixtures were captured. Fields added past the modeled
        // prefix must not stop the swap from being parsed.
        for fixture in [
            load_fixture(include_str!("../tests/fixtures/buy_mainnet.json")),
            load_fixture(include_str!("../tests/fixtures/sell_mainnet.json")),
        ] {
            let expected = parse_fixture(&fixture);
            let mut upgraded = fixture;
            upgraded.event_data.extend_from_slice(&[0; 16]);

            assert_eq!(parse_fixture(&upgraded), expected);
            assert!(matches!(expected, Ok(Some(ParsedEvent::TokenSwap(_)))));
        }
    }

    /// Bytes consumed by the prefix this parser models for a real event payload.
    fn modeled_prefix_len(event_data: &[u8]) -> usize {
        let payload = &event_data[EVENT_HEADER_LEN..];
        let mut remaining = payload;
        if event_data[EVENT_IX_TAG.len()..EVENT_HEADER_LEN] == BUY_EVENT_DISCRIMINATOR {
            BuyEventPrefix::deserialize(&mut remaining).expect("fixture must decode");
        } else {
            SellEventPrefix::deserialize(&mut remaining).expect("fixture must decode");
        }
        payload.len() - remaining.len()
    }
}
