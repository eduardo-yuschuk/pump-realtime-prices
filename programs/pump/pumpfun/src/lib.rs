//! Pump.fun program integration.
//!
//! The parser is based on the IDL the program publishes on chain, stored as
//! `new_idl.json` next to this crate and read on 2026-09-22. See
//! `doc/protocol_parsers.md` for its provenance.
//!
//! Events are decoded up to the last field consumed here and any trailing
//! bytes are ignored, so fields appended by a protocol upgrade do not stop the
//! swap from being parsed.
//!
//! Pump.fun emits Anchor events through self-CPI instructions. This parser only
//! emits storage events from those self-CPIs: outer create and trade
//! instructions describe intent and limits, while `CreateEvent` and
//! `TradeEvent` contain the authoritative result. Parsing only event CPIs also
//! prevents duplicate outputs when both outer and inner instructions are
//! traversed.

use std::str;

use borsh::BorshDeserialize;
use common::{
    decode_event_prefix, InstructionContext, InstructionParser, ParseError, ParseResult,
    ParsedEvent, TokenDiscovery, TokenSwap,
};
use solana_pubkey::Pubkey;

/// Pump.fun program address on Solana mainnet and devnet.
pub const PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

const EVENT_AUTHORITY: &str = "Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1";
const WRAPPED_SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const EVENT_IX_TAG: [u8; 8] = [228, 69, 165, 46, 81, 203, 154, 29];
const CREATE_EVENT_DISCRIMINATOR: [u8; 8] = [27, 114, 169, 77, 222, 235, 99, 118];
const TRADE_EVENT_DISCRIMINATOR: [u8; 8] = [189, 219, 127, 211, 78, 230, 97, 238];
const EVENT_HEADER_LEN: usize = EVENT_IX_TAG.len() + CREATE_EVENT_DISCRIMINATOR.len();

/// Parses Pump.fun Anchor event-CPI instructions.
#[derive(Clone, Copy, Debug, Default)]
pub struct PumpFunParser;

impl InstructionParser for PumpFunParser {
    fn program_id(&self) -> &'static str {
        PROGRAM_ID
    }

    fn parse_instruction(
        &self,
        instruction: InstructionContext<'_>,
    ) -> ParseResult<Option<ParsedEvent>> {
        instruction.ensure_program_id(self.program_id())?;

        if !instruction.data().starts_with(&EVENT_IX_TAG) {
            return Ok(None);
        }

        instruction.ensure_data_len(EVENT_HEADER_LEN)?;

        let discriminator = &instruction.data()[EVENT_IX_TAG.len()..EVENT_HEADER_LEN];
        if discriminator != CREATE_EVENT_DISCRIMINATOR && discriminator != TRADE_EVENT_DISCRIMINATOR
        {
            return Ok(None);
        }

        if instruction.account(0)? != EVENT_AUTHORITY {
            return Err(ParseError::InvalidInstructionData(
                "Pump.fun event CPI has an invalid event authority".to_owned(),
            ));
        }

        let payload = &instruction.data()[EVENT_HEADER_LEN..];
        match discriminator {
            value if value == CREATE_EVENT_DISCRIMINATOR => {
                parse_create_event(payload).map(|event| Some(ParsedEvent::TokenDiscovery(event)))
            }
            value if value == TRADE_EVENT_DISCRIMINATOR => {
                parse_trade_event(payload).map(|event| Some(ParsedEvent::TokenSwap(event)))
            }
            _ => unreachable!(),
        }
    }
}

/// One entry of the `TradeEvent.shareholders` collection.
#[derive(BorshDeserialize)]
#[allow(dead_code)]
struct Shareholder {
    address: Pubkey,
    share_bps: u16,
}

/// Leading `CreateEvent` fields, up to the last one this parser consumes.
///
/// The program keeps appending fields, so anything after `creator` is
/// deliberately left unmodeled.
#[derive(BorshDeserialize)]
#[allow(dead_code)]
struct CreateEventPrefix {
    name: String,
    symbol: String,
    uri: String,
    mint: Pubkey,
    bonding_curve: Pubkey,
    user: Pubkey,
    creator: Pubkey,
}

/// Leading `TradeEvent` fields, up to the last one this parser consumes.
///
/// `quote_mint` and `quote_amount` sit after the variable-length
/// `shareholders` collection, so the prefix has to span it.
#[derive(BorshDeserialize)]
#[allow(dead_code)]
struct TradeEventPrefix {
    mint: Pubkey,
    sol_amount: u64,
    token_amount: u64,
    is_buy: bool,
    user: Pubkey,
    timestamp: i64,
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
    real_sol_reserves: u64,
    real_token_reserves: u64,
    fee_recipient: Pubkey,
    fee_basis_points: u64,
    fee: u64,
    creator: Pubkey,
    creator_fee_basis_points: u64,
    creator_fee: u64,
    track_volume: bool,
    total_unclaimed_tokens: u64,
    total_claimed_tokens: u64,
    current_sol_volume: u64,
    last_update_timestamp: i64,
    ix_name: String,
    mayhem_mode: bool,
    cashback_fee_basis_points: u64,
    cashback: u64,
    buyback_fee_basis_points: u64,
    // The buyback fee is a portion of `fee`, not an additional user charge.
    buyback_fee: u64,
    shareholders: Vec<Shareholder>,
    quote_mint: Pubkey,
    quote_amount: u64,
}

fn parse_create_event(payload: &[u8]) -> ParseResult<TokenDiscovery> {
    let event: CreateEventPrefix = decode_event_prefix(payload, "CreateEvent")?;

    Ok(TokenDiscovery {
        mint: event.mint.to_string(),
        creator: event.creator.to_string(),
        name: event.name,
        symbol: event.symbol,
        uri: event.uri,
    })
}

fn parse_trade_event(payload: &[u8]) -> ParseResult<TokenSwap> {
    let event: TradeEventPrefix = decode_event_prefix(payload, "TradeEvent")?;

    let total_fees = event
        .fee
        .checked_add(event.creator_fee)
        .and_then(|amount| amount.checked_add(event.cashback))
        .ok_or_else(|| {
            ParseError::InvalidInstructionData("TradeEvent fee total overflows u64".to_owned())
        })?;
    let quote_mint = if event.quote_mint == Pubkey::default() {
        WRAPPED_SOL_MINT.to_owned()
    } else {
        event.quote_mint.to_string()
    };
    let program_id = Pubkey::try_from(PROGRAM_ID).map_err(|error| {
        ParseError::InvalidInstructionData(format!("invalid Pump.fun program address: {error}"))
    })?;
    let (pool, _) =
        Pubkey::find_program_address(&[b"bonding-curve", event.mint.as_ref()], &program_id);

    let (input_mint, input_amount, output_mint, output_amount) = if event.is_buy {
        let input_amount = event.quote_amount.checked_add(total_fees).ok_or_else(|| {
            ParseError::InvalidInstructionData(
                "TradeEvent gross input amount overflows u64".to_owned(),
            )
        })?;
        (
            quote_mint,
            input_amount,
            event.mint.to_string(),
            event.token_amount,
        )
    } else {
        let output_amount = event.quote_amount.checked_sub(total_fees).ok_or_else(|| {
            ParseError::InvalidInstructionData(
                "TradeEvent fees exceed the quote output amount".to_owned(),
            )
        })?;
        (
            event.mint.to_string(),
            event.token_amount,
            quote_mint,
            output_amount,
        )
    };

    Ok(TokenSwap {
        user: event.user.to_string(),
        pool: pool.to_string(),
        base_mint: None,
        quote_mint: None,
        input_mint,
        input_amount,
        output_mint,
        output_amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse_fixture(source: &str) -> (Value, Vec<u8>, ParsedEvent) {
        let fixture: Value = serde_json::from_str(source).unwrap();
        let program_id = fixture["program_id"].as_str().unwrap();
        let accounts: Vec<&str> = fixture["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|account| account.as_str().unwrap())
            .collect();
        let data = bs58::decode(fixture["data"].as_str().unwrap())
            .into_vec()
            .unwrap();
        let instruction = InstructionContext::new(program_id, &accounts, &data);
        let event = PumpFunParser
            .parse_instruction(instruction)
            .unwrap()
            .unwrap();

        (fixture, data, event)
    }

    #[test]
    fn parses_a_real_create_event() {
        let (fixture, _, event) =
            parse_fixture(include_str!("../tests/fixtures/create_event_mainnet.json"));

        assert_eq!(fixture["slot"], 438025562);
        assert_eq!(
            event,
            ParsedEvent::TokenDiscovery(TokenDiscovery {
                mint: "9Nkx7xDaUankJ1NfHAYixLdzSdzgEDGcSjAidQ6jpump".to_owned(),
                creator: "5x6NbLoPL5pxBhN6FMbGqrKiBkViPF7YA8SB8tShECq8".to_owned(),
                name: "OIL PEPE".to_owned(),
                symbol: "OILPEPE".to_owned(),
                uri: "https://ipfs.io/ipfs/QmTJUYbmXG3AT28fxiSyTjrNiNHTkyc1ifeT4BNMZ1JUQm"
                    .to_owned(),
            })
        );
    }

    #[test]
    fn parses_real_mainnet_events_with_holder_reward_fields() {
        // Captured after the program appended `holder_rewards_bps` and
        // `holder_rewards` to `TradeEvent`, and `creator_fee_bps` and
        // `is_holder_reward` to `CreateEvent`.
        let (fixture, _, event) = parse_fixture(include_str!(
            "../tests/fixtures/trade_event_holder_rewards_mainnet.json"
        ));

        assert_eq!(fixture["slot"], 449_427_601);
        assert_eq!(
            event,
            ParsedEvent::TokenSwap(TokenSwap {
                user: "HcfrKfAvxFaGdVUhH5iGXRX63Ku6MoYHT4Kj9AqHVSRA".to_owned(),
                pool: "G7V6o1RyWUn6PyXLdjHZ1MdTRzdGwQLXCjBnzvFLes6e".to_owned(),
                base_mint: None,
                quote_mint: None,
                input_mint: "91CfqPw2hwzi7azaDmJ4fFtHa93yc8KAQV7ss6Xrpump".to_owned(),
                input_amount: 335_031_172_403,
                output_mint: WRAPPED_SOL_MINT.to_owned(),
                output_amount: 9_753_083,
            })
        );

        let (fixture, _, event) = parse_fixture(include_str!(
            "../tests/fixtures/create_event_holder_rewards_mainnet.json"
        ));

        assert_eq!(fixture["slot"], 449_427_602);
        assert_eq!(
            event,
            ParsedEvent::TokenDiscovery(TokenDiscovery {
                mint: "DZyDVTXN4bfXT6xF8KUTfHZ669zJ3fAUVWysNibEHZfP".to_owned(),
                creator: "6nU2L7MQVUWjtdKHVpuZA9aind73nd3rXC4YFo8KQCy4".to_owned(),
                name: "OpenMuse".to_owned(),
                symbol: "OPENMUSE".to_owned(),
                uri: "https://pf.jake-98f.workers.dev/metadata/9zy230gh.json".to_owned(),
            })
        );
    }

    #[test]
    fn parses_a_real_buy_trade_event() {
        let (fixture, _, event) = parse_fixture(include_str!(
            "../tests/fixtures/trade_event_buy_mainnet.json"
        ));

        assert_eq!(fixture["slot"], 437840553);
        assert_eq!(
            event,
            ParsedEvent::TokenSwap(TokenSwap {
                user: "BwWK17cbHxwWBKZkUYvzxLcNQ1YVyaFezduWbtm2de6s".to_owned(),
                pool: "2ntct7fobbSv2rnMSccPXDxPsmuaRu4Zbykw8uvUcTmD".to_owned(),
                base_mint: None,
                quote_mint: None,
                input_mint: WRAPPED_SOL_MINT.to_owned(),
                input_amount: 81_566_824,
                output_mint: "8NMMzUZ3sGdS1ZPUj1YGcJyHzRgxMHW9aHjqZkfbpump".to_owned(),
                output_amount: 2_519_953_715_914,
            })
        );
    }

    #[test]
    fn parses_a_real_sell_without_double_counting_the_buyback_fee() {
        let (fixture, _, event) = parse_fixture(include_str!(
            "../tests/fixtures/trade_event_sell_mainnet.json"
        ));

        assert_eq!(fixture["slot"], 437840553);
        assert_eq!(
            event,
            ParsedEvent::TokenSwap(TokenSwap {
                user: "8T4stzcuUcRTRX3aBFuTjXPw3FMxuk98ppUEAFwej9RT".to_owned(),
                pool: "99rwCg3rNs3JnF6PGC2DYjdbAYsGSt9voCtLyc88VSUE".to_owned(),
                base_mint: None,
                quote_mint: None,
                input_mint: "E49s67zcz2Zomc6c8Pk57eCNsAv7j6a86T3vACzFpump".to_owned(),
                input_amount: 640_564_376_586,
                output_mint: WRAPPED_SOL_MINT.to_owned(),
                output_amount: 117_473_865,
            })
        );
    }

    #[test]
    fn returns_none_for_outer_pumpfun_instructions() {
        let outer_buy = [102, 6, 61, 18, 1, 218, 235, 234];
        let instruction = InstructionContext::new(PROGRAM_ID, &[], &outer_buy);

        assert_eq!(PumpFunParser.parse_instruction(instruction).unwrap(), None);
    }

    #[test]
    fn returns_none_for_unmodeled_anchor_events() {
        let mut data = EVENT_IX_TAG.to_vec();
        data.extend_from_slice(&[0; 8]);
        let instruction = InstructionContext::new(PROGRAM_ID, &[], &data);

        assert_eq!(PumpFunParser.parse_instruction(instruction).unwrap(), None);
    }

    #[test]
    fn rejects_truncated_event_headers() {
        let instruction = InstructionContext::new(PROGRAM_ID, &[], &EVENT_IX_TAG);

        assert_eq!(
            PumpFunParser.parse_instruction(instruction),
            Err(ParseError::DataTooShort {
                expected_at_least: EVENT_HEADER_LEN,
                actual: EVENT_IX_TAG.len(),
            })
        );
    }

    #[test]
    fn rejects_truncated_event_payloads() {
        let (_, mut data, _) =
            parse_fixture(include_str!("../tests/fixtures/create_event_mainnet.json"));
        // Bytes past the modeled prefix are ignored on purpose, so the payload
        // has to be cut inside it for the truncation to be detectable.
        let prefix_len = modeled_prefix_len::<CreateEventPrefix>(&data);
        data.truncate(EVENT_HEADER_LEN + prefix_len - 1);
        let accounts = [EVENT_AUTHORITY];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &data);

        assert_invalid(
            PumpFunParser.parse_instruction(instruction),
            "Unexpected length of input",
        );
    }

    /// Bytes consumed by the prefix this parser models for a real event payload.
    fn modeled_prefix_len<T: BorshDeserialize>(data: &[u8]) -> usize {
        let payload = &data[EVENT_HEADER_LEN..];
        let mut remaining = payload;
        T::deserialize(&mut remaining).expect("fixture must decode");
        payload.len() - remaining.len()
    }

    fn assert_invalid(result: ParseResult<Option<ParsedEvent>>, expected: &str) {
        assert!(
            matches!(&result, Err(ParseError::InvalidInstructionData(reason)) if reason.contains(expected)),
            "expected invalid instruction data containing {expected:?}, got {result:?}"
        );
    }

    #[test]
    fn rejects_invalid_borsh_booleans() {
        let (_, mut data, _) = parse_fixture(include_str!(
            "../tests/fixtures/trade_event_buy_mainnet.json"
        ));
        let is_buy_offset = EVENT_HEADER_LEN + 32 + 8 + 8;
        data[is_buy_offset] = 2;
        let accounts = [EVENT_AUTHORITY];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &data);

        assert_invalid(
            PumpFunParser.parse_instruction(instruction),
            "Invalid bool representation",
        );
    }

    #[test]
    fn parses_events_carrying_fields_appended_by_protocol_upgrades() {
        // The program appended `creator_fee_bps` and `is_holder_reward` to
        // `CreateEvent` after this fixture was captured. Fields added past the
        // modeled prefix must not stop the event from being parsed.
        for fixture in [
            include_str!("../tests/fixtures/create_event_mainnet.json"),
            include_str!("../tests/fixtures/trade_event_buy_mainnet.json"),
        ] {
            let (_, data, _) = parse_fixture(fixture);
            let accounts = [EVENT_AUTHORITY];
            let expected = PumpFunParser
                .parse_instruction(InstructionContext::new(PROGRAM_ID, &accounts, &data));

            let mut upgraded = data;
            upgraded.extend_from_slice(&[0; 16]);

            assert_eq!(
                PumpFunParser
                    .parse_instruction(InstructionContext::new(PROGRAM_ID, &accounts, &upgraded)),
                expected
            );
            assert!(matches!(expected, Ok(Some(_))));
        }
    }

    #[test]
    fn rejects_shareholder_lengths_larger_than_the_payload() {
        let (_, mut data, _) = parse_fixture(include_str!(
            "../tests/fixtures/trade_event_buy_mainnet.json"
        ));
        let shareholder_count_offset = data.len() - 60;
        data[shareholder_count_offset..shareholder_count_offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        let accounts = [EVENT_AUTHORITY];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &data);

        assert_invalid(
            PumpFunParser.parse_instruction(instruction),
            "Unexpected length of input",
        );
    }

    #[test]
    fn rejects_event_cpis_with_the_wrong_authority() {
        let (_, data, _) =
            parse_fixture(include_str!("../tests/fixtures/create_event_mainnet.json"));
        let accounts = ["WrongAuthority"];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &data);

        assert_eq!(
            PumpFunParser.parse_instruction(instruction),
            Err(ParseError::InvalidInstructionData(
                "Pump.fun event CPI has an invalid event authority".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_sell_events_whose_fees_exceed_the_quote_amount() {
        let (_, mut data, _) = parse_fixture(include_str!(
            "../tests/fixtures/trade_event_sell_mainnet.json"
        ));
        let quote_amount_offset = data.len() - 24;
        data[quote_amount_offset..quote_amount_offset + 8].fill(0);
        let accounts = [EVENT_AUTHORITY];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &data);

        assert_eq!(
            PumpFunParser.parse_instruction(instruction),
            Err(ParseError::InvalidInstructionData(
                "TradeEvent fees exceed the quote output amount".to_owned()
            ))
        );
    }
}
