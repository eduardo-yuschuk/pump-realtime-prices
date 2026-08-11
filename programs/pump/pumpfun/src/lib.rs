//! Pump.fun program integration.
//!
//! The parser is based on the official Pump.fun IDL at commit
//! `9c82f61cb711b044a17f770ab8ce9f9bdf78f333`:
//! <https://github.com/pump-fun/pump-public-docs/blob/9c82f61cb711b044a17f770ab8ce9f9bdf78f333/idl/pump.json>.
//!
//! Pump.fun emits Anchor events through self-CPI instructions. This parser only
//! emits storage events from those self-CPIs: outer create and trade
//! instructions describe intent and limits, while `CreateEvent` and
//! `TradeEvent` contain the authoritative result. Parsing only event CPIs also
//! prevents duplicate outputs when both outer and inner instructions are
//! traversed.

use std::str;

use common::{
    InstructionContext, InstructionParser, ParseError, ParseResult, ParsedEvent, TokenDiscovery,
    TokenSwap,
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

fn parse_create_event(payload: &[u8]) -> ParseResult<TokenDiscovery> {
    let mut decoder = Decoder::new(payload);
    let name = decoder.read_string("CreateEvent.name")?;
    let symbol = decoder.read_string("CreateEvent.symbol")?;
    let uri = decoder.read_string("CreateEvent.uri")?;
    let mint = decoder.read_pubkey("CreateEvent.mint")?;
    decoder.read_pubkey("CreateEvent.bonding_curve")?;
    decoder.read_pubkey("CreateEvent.user")?;
    let creator = decoder.read_pubkey("CreateEvent.creator")?;
    decoder.read_i64("CreateEvent.timestamp")?;
    decoder.read_u64("CreateEvent.virtual_token_reserves")?;
    decoder.read_u64("CreateEvent.virtual_sol_reserves")?;
    decoder.read_u64("CreateEvent.real_token_reserves")?;
    decoder.read_u64("CreateEvent.token_total_supply")?;
    decoder.read_pubkey("CreateEvent.token_program")?;
    decoder.read_bool("CreateEvent.is_mayhem_mode")?;
    decoder.read_bool("CreateEvent.is_cashback_enabled")?;
    decoder.read_pubkey("CreateEvent.quote_mint")?;
    decoder.read_u64("CreateEvent.virtual_quote_reserves")?;
    decoder.finish("CreateEvent")?;

    Ok(TokenDiscovery {
        mint: mint.to_string(),
        creator: creator.to_string(),
        name,
        symbol,
        uri,
    })
}

fn parse_trade_event(payload: &[u8]) -> ParseResult<TokenSwap> {
    let mut decoder = Decoder::new(payload);
    let mint = decoder.read_pubkey("TradeEvent.mint")?;
    decoder.read_u64("TradeEvent.sol_amount")?;
    let token_amount = decoder.read_u64("TradeEvent.token_amount")?;
    let is_buy = decoder.read_bool("TradeEvent.is_buy")?;
    let user = decoder.read_pubkey("TradeEvent.user")?;
    decoder.read_i64("TradeEvent.timestamp")?;
    decoder.read_u64("TradeEvent.virtual_sol_reserves")?;
    decoder.read_u64("TradeEvent.virtual_token_reserves")?;
    decoder.read_u64("TradeEvent.real_sol_reserves")?;
    decoder.read_u64("TradeEvent.real_token_reserves")?;
    decoder.read_pubkey("TradeEvent.fee_recipient")?;
    decoder.read_u64("TradeEvent.fee_basis_points")?;
    let fee = decoder.read_u64("TradeEvent.fee")?;
    decoder.read_pubkey("TradeEvent.creator")?;
    decoder.read_u64("TradeEvent.creator_fee_basis_points")?;
    let creator_fee = decoder.read_u64("TradeEvent.creator_fee")?;
    decoder.read_bool("TradeEvent.track_volume")?;
    decoder.read_u64("TradeEvent.total_unclaimed_tokens")?;
    decoder.read_u64("TradeEvent.total_claimed_tokens")?;
    decoder.read_u64("TradeEvent.current_sol_volume")?;
    decoder.read_i64("TradeEvent.last_update_timestamp")?;
    decoder.read_string("TradeEvent.ix_name")?;
    decoder.read_bool("TradeEvent.mayhem_mode")?;
    decoder.read_u64("TradeEvent.cashback_fee_basis_points")?;
    let cashback = decoder.read_u64("TradeEvent.cashback")?;
    decoder.read_u64("TradeEvent.buyback_fee_basis_points")?;
    // The buyback fee is a portion of `fee`, not an additional user charge.
    decoder.read_u64("TradeEvent.buyback_fee")?;
    let shareholder_count = decoder.read_u32("TradeEvent.shareholders.length")? as usize;
    let shareholder_bytes = shareholder_count.checked_mul(34).ok_or_else(|| {
        ParseError::InvalidInstructionData(
            "TradeEvent shareholder collection length overflows".to_owned(),
        )
    })?;
    decoder.read_bytes(shareholder_bytes, "TradeEvent.shareholders")?;
    let quote_mint = decoder.read_pubkey("TradeEvent.quote_mint")?;
    let quote_amount = decoder.read_u64("TradeEvent.quote_amount")?;
    decoder.read_u64("TradeEvent.virtual_quote_reserves")?;
    decoder.read_u64("TradeEvent.real_quote_reserves")?;
    decoder.finish("TradeEvent")?;

    let total_fees = fee
        .checked_add(creator_fee)
        .and_then(|amount| amount.checked_add(cashback))
        .ok_or_else(|| {
            ParseError::InvalidInstructionData("TradeEvent fee total overflows u64".to_owned())
        })?;
    let quote_mint = if quote_mint == Pubkey::default() {
        WRAPPED_SOL_MINT.to_owned()
    } else {
        quote_mint.to_string()
    };
    let program_id = Pubkey::try_from(PROGRAM_ID).map_err(|error| {
        ParseError::InvalidInstructionData(format!("invalid Pump.fun program address: {error}"))
    })?;
    let (pool, _) = Pubkey::find_program_address(&[b"bonding-curve", mint.as_ref()], &program_id);

    let (input_mint, input_amount, output_mint, output_amount) = if is_buy {
        let input_amount = quote_amount.checked_add(total_fees).ok_or_else(|| {
            ParseError::InvalidInstructionData(
                "TradeEvent gross input amount overflows u64".to_owned(),
            )
        })?;
        (quote_mint, input_amount, mint.to_string(), token_amount)
    } else {
        let output_amount = quote_amount.checked_sub(total_fees).ok_or_else(|| {
            ParseError::InvalidInstructionData(
                "TradeEvent fees exceed the quote output amount".to_owned(),
            )
        })?;
        (mint.to_string(), token_amount, quote_mint, output_amount)
    };

    Ok(TokenSwap {
        user: user.to_string(),
        pool: pool.to_string(),
        base_mint: None,
        quote_mint: None,
        input_mint,
        input_amount,
        output_mint,
        output_amount,
    })
}

struct Decoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_bytes(&mut self, length: usize, field: &str) -> ParseResult<&'a [u8]> {
        let end = self.offset.checked_add(length).ok_or_else(|| {
            ParseError::InvalidInstructionData(format!("{field} length overflows"))
        })?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(ParseError::DataTooShort {
                expected_at_least: EVENT_HEADER_LEN.saturating_add(end),
                actual: EVENT_HEADER_LEN.saturating_add(self.data.len()),
            })?;
        self.offset = end;
        Ok(bytes)
    }

    fn read_array<const LENGTH: usize>(&mut self, field: &str) -> ParseResult<[u8; LENGTH]> {
        self.read_bytes(LENGTH, field)?
            .try_into()
            .map_err(|_| ParseError::InvalidInstructionData(format!("invalid {field} length")))
    }

    fn read_u32(&mut self, field: &str) -> ParseResult<u32> {
        Ok(u32::from_le_bytes(self.read_array(field)?))
    }

    fn read_u64(&mut self, field: &str) -> ParseResult<u64> {
        Ok(u64::from_le_bytes(self.read_array(field)?))
    }

    fn read_i64(&mut self, field: &str) -> ParseResult<i64> {
        Ok(i64::from_le_bytes(self.read_array(field)?))
    }

    fn read_bool(&mut self, field: &str) -> ParseResult<bool> {
        match self.read_bytes(1, field)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(ParseError::InvalidInstructionData(format!(
                "{field} contains invalid bool value {value}"
            ))),
        }
    }

    fn read_pubkey(&mut self, field: &str) -> ParseResult<Pubkey> {
        Ok(Pubkey::new_from_array(self.read_array(field)?))
    }

    fn read_string(&mut self, field: &str) -> ParseResult<String> {
        let length = self.read_u32(&format!("{field}.length"))? as usize;
        let bytes = self.read_bytes(length, field)?;
        str::from_utf8(bytes).map(str::to_owned).map_err(|error| {
            ParseError::InvalidInstructionData(format!("{field} is not valid UTF-8: {error}"))
        })
    }

    fn finish(&self, event: &str) -> ParseResult<()> {
        if self.offset == self.data.len() {
            Ok(())
        } else {
            Err(ParseError::InvalidInstructionData(format!(
                "{event} contains {} unexpected trailing bytes",
                self.data.len() - self.offset
            )))
        }
    }
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
        data.pop();
        let accounts = [EVENT_AUTHORITY];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &data);

        assert!(matches!(
            PumpFunParser.parse_instruction(instruction),
            Err(ParseError::DataTooShort { .. })
        ));
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

        assert_eq!(
            PumpFunParser.parse_instruction(instruction),
            Err(ParseError::InvalidInstructionData(
                "TradeEvent.is_buy contains invalid bool value 2".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_unexpected_trailing_event_data() {
        let (_, mut data, _) =
            parse_fixture(include_str!("../tests/fixtures/create_event_mainnet.json"));
        data.push(0);
        let accounts = [EVENT_AUTHORITY];
        let instruction = InstructionContext::new(PROGRAM_ID, &accounts, &data);

        assert_eq!(
            PumpFunParser.parse_instruction(instruction),
            Err(ParseError::InvalidInstructionData(
                "CreateEvent contains 1 unexpected trailing bytes".to_owned()
            ))
        );
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

        assert!(matches!(
            PumpFunParser.parse_instruction(instruction),
            Err(ParseError::DataTooShort { .. })
        ));
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
