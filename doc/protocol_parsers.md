# Protocol Parser Source Policy

## Decision

Protocol parsers must be implemented from authoritative, versioned protocol sources and validated against real successful on-chain data.

## Required Evidence

Every protocol parser implementation must:

1. Use the protocol owner's official IDL and documentation as the primary specification for program addresses, discriminators, account ordering, argument encoding, and event layouts.
2. Record the exact source URL and commit, release, or version used by the implementation so later protocol changes can be evaluated reproducibly.
3. Validate supported instructions against real successful transactions or blocks from the target Solana cluster, and retain representative data as repository fixtures when licensing and size permit.
4. Add synthetic tests for malformed and truncated data in addition to tests based on real observations.
5. Treat third-party documentation, APIs, and decoded transaction services only as secondary evidence for discovery or cross-checking. They must not override an available official specification.

If official sources are unavailable, ambiguous, or inconsistent with observed on-chain data, the discrepancy must be documented before implementing assumptions. The parser must not silently infer a layout from a single third-party decoder.

## Event Decoding

Anchor events are extended by appending fields, and both Pump programs do so
without always republishing their IDL. A parser that requires a payload to be
consumed exactly therefore stops producing events the moment the protocol ships
an upgrade, even though every field it reads is unchanged.

Parsers must consequently:

1. Model only the prefix of an event up to the last field they consume, as a
   Borsh struct decoded through `common::decode_event_prefix`, and ignore any
   bytes that follow it.
2. Reject payloads that are too short for that prefix.
3. Cross-check decoded values against the surrounding instruction, so a
   reordered or reinterpreted layout cannot pass unnoticed. Correlating the
   event with its parent instruction accounts, amounts, and `ix_name` is the
   expected mechanism.

The trade-off is deliberate: a payload truncated after the modeled prefix is
indistinguishable from a shorter protocol version and is accepted. Detecting an
appended field is a documentation task, handled by re-reading the on-chain IDL
and the observed payload sizes, not a parsing failure.

## Pump.fun Baseline

The Pump.fun parser uses the IDL published on chain by the program itself,
stored as `programs/pump/pumpfun/new_idl.json`. It was read on 2026-09-22 from
the Anchor IDL account `AYgC53tU5BbP2NAnv5nConJxAdpQZctvmZK88pu69xRs`, derived
from program `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`. At that date the
declared `TradeEvent` and `CreateEvent` layouts matched mainnet payloads
exactly, with no trailing bytes.

That IDL supersedes commit [`9c82f61cb711b044a17f770ab8ce9f9bdf78f333`](https://github.com/pump-fun/pump-public-docs/commit/9c82f61cb711b044a17f770ab8ce9f9bdf78f333)
of `pump-fun/pump-public-docs`, which predates the `holder_rewards_bps` and
`holder_rewards` fields appended to `TradeEvent`, and the `creator_fee_bps` and
`is_holder_reward` fields appended to `CreateEvent`. Supported layouts are
checked against the successful mainnet instructions identified by the fixtures
under `programs/pump/pumpfun/tests/fixtures/`, which cover payloads captured
both before and after those fields were introduced.

## PumpSwap Baseline

The PumpSwap parser uses the IDL published on chain by the program itself,
stored as `programs/pump/pumpswap/new_idl.json`. It was read on 2026-09-22 from
the Anchor IDL account `5fLnXNNoZcZt9Qku6HARM3un3Ttm2cGsR7gN9Zp1R7h3`, derived
from program `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`. Its `BuyEvent` and
`SellEvent` layouts are checked against successful mainnet self-CPI
instructions identified by the fixtures under
`programs/pump/pumpswap/tests/fixtures/`.

**That IDL is behind the deployed program.** Mainnet payloads observed at slot
`449427600` carry 41 bytes that it does not declare, appended after
`buyback_fee`: `virtual_quote_reserves` (i128), `can_boost` (bool),
`base_supply` (u64), `holder_rewards_bps` (u64), and `holder_rewards` (u64).
The first three were already present at slot `438130307`. Because the fields
this parser consumes all precede them, the prefix rule above keeps both payload
versions parsable, and no layout is inferred for the undeclared tail.

The IDL declares a trailing `track_volume: OptionBool` argument for `buy_exact_quote_in`, but the successful mainnet instruction at slot `438131164` in fixture `buy_exact_quote_in_mainnet.json` omits that byte. The parser therefore requires the observed discriminator and two `u64` argument prefix for this parent instruction, does not infer a `track_volume` value, and obtains all executed amounts from the correlated `BuyEvent`.
