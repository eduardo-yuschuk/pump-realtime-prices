# Parser Architecture

The `parser` crate is the off-chain orchestration layer that turns Solana block JSON into ordered, storage-facing protocol events. It does not perform RPC or filesystem I/O and it does not contain protocol-specific binary layouts. Those responsibilities are separated into input adapters and protocol crates.

## Crate Boundaries

The parsing design has three layers:

```text
                         +----------------------+
block JSON ------------>| parser               |----> BlockEvents
                         |                      |
                         | config -> registry   |
                         |        -> dispatcher |
                         |        -> tx/block   |
                         +----------+-----------+
                                    |
                                    v
                         +----------------------+
                         | programs/*           |
                         | protocol decoders    |
                         +----------+-----------+
                                    |
                                    v
                         +----------------------+
                         | common               |
                         | contracts and events |
                         +----------------------+
```

The dependency direction is:

```text
common <- protocol crates under programs/* <- parser <- applications and tools
```

More precisely, protocol crates depend only on `common`, while `parser` depends on both `common` and every protocol crate included in its built-in registry. This keeps protocol decoding independent from block traversal and prevents dependency cycles.

The crates under `programs/` are off-chain parser integrations for deployed Solana programs. Despite the directory name, they are not on-chain program binaries.

### `common`

`common/src/lib.rs` owns the interface shared by orchestration and protocol crates:

- `InstructionContext` is a borrowed view containing a resolved program ID, account addresses in instruction order, decoded instruction data, and the immediate calling instruction when the current instruction is a CPI and that relationship can be reconstructed.
- `InstructionParser` is the object-safe protocol parser interface.
- `ParseError` and `ParseResult` describe protocol-level decoding failures.
- `ParsedEvent`, `TokenDiscovery`, and `TokenSwap` are the storage-facing event model.

The contract for one protocol instruction is:

```rust
pub trait InstructionParser: Send + Sync {
    fn program_id(&self) -> &'static str;

    fn parse_instruction(
        &self,
        instruction: InstructionContext<'_>,
    ) -> ParseResult<Option<ParsedEvent>>;
}
```

Implementations return:

- `Ok(Some(event))` when the instruction produces a modeled event.
- `Ok(None)` when it produces no modeled event.
- `Err(error)` when a relevant instruction cannot be decoded according to the protocol.

Protocol crates do not resolve transaction account indexes, decode base58, traverse CPI instructions, read configuration, or decide output ordering.

### Protocol crates under `programs/`

Each protocol crate owns the program-specific details required to implement `InstructionParser`, including program IDs, discriminators, account positions, binary layouts, and event semantics. Implementations must follow the [protocol parser source policy](protocol_parsers.md), including versioned official sources and validation against successful on-chain data.

`programs/pump/pumpfun` and `programs/pump/pumpswap` are connected to the built-in registry. Both parsers consume authoritative Anchor event self-CPIs rather than treating outer instruction limits as executed results. PumpSwap event CPIs are correlated with their immediate parent invocation because the event contains executed amounts while the parent accounts identify the base and quote mints.

### `parser`

The modules in `parser/src` divide orchestration responsibilities as follows:

- `config` validates protocol selection.
- `registry` constructs protocol parsers and indexes them by program ID.
- `instruction` dispatches one normalized instruction.
- `transaction` traverses one successful transaction and normalizes its instructions.
- `block` traverses transactions and owns the public block-level entry point.
- `event` defines ordered block, transaction, and instruction output containers.
- `error` separates initialization, structural, normalization, and protocol errors.

`parser/src/lib.rs` reexports the public parser constructors, pipeline types, output types, and errors.

## Configuration

`ParserConfig` accepts a strict, case-sensitive, comma-separated list of parser names. Whitespace around each name is ignored, but the following are rejected:

- A missing or non-Unicode `PARSERS` environment variable.
- An empty list or an empty list element.
- An unknown parser name.
- A duplicate parser name.

The currently supported names are `pumpfun` and `pumpswap`.

There are three configuration paths:

- `ParserConfig::resolve(Some(value))` uses the explicit override and does not load `.env` or read `PARSERS`.
- `ParserConfig::resolve(None)` falls back to `ParserConfig::from_env()`.
- `ParserConfig::new(...)` accepts typed `ParserName` values for tests and callers that do not use strings.

Without an explicit override, effective precedence is the existing process environment followed by `.env`, because `dotenvy` does not overwrite an existing environment variable. `BlockParser::from_env()` is the convenience constructor for this path. A caller with an override resolves it first and then calls `BlockParser::from_config(config)`.

Configuration is startup state, not per-block state. It has no hot-reload behavior.

## Registry Lifecycle

`ParserRegistry::from_config` calls the built-in factory for each selected `ParserName`. Each selected protocol parser is instantiated once and stored as a `Box<dyn InstructionParser>` in a map keyed by `program_id()`.

Duplicate program IDs are rejected even if they came from different parser implementations. `ParserRegistry::from_parsers` also permits explicit parser injection for tests or custom applications, bypassing the built-in name-to-factory mapping.

The ownership chain is:

```text
BlockParser
  owns TransactionParser
    owns InstructionDispatcher
      owns ParserRegistry
        owns one instance of each configured protocol parser
```

Applications should build this chain once during startup and reuse the resulting parser for every block. The registry is immutable after construction.

## Block Parsing Pipeline

`BlockParser::parse_block(&serde_json::Value)` is the primary entry point. The input must be the block object whose `transactions` field is an array, not an enclosing JSON-RPC response object.

For each transaction in block order, the pipeline performs these steps:

1. Read transaction metadata and skip the complete transaction when `meta.err` is not `null`.
2. Resolve account keys as static `message.accountKeys`, followed by `meta.loadedAddresses.writable`, followed by `meta.loadedAddresses.readonly`.
3. Visit outer and inner instructions in validator evaluation order.
4. Resolve each instruction's program and account indexes against the combined key list.
5. Decode the base58 instruction data into bytes.
6. Build an `InstructionContext` and dispatch it by program ID.
7. Retain detected events and instruction-scoped failures while filtering no-event results.

The parser consumes loaded addresses already resolved in the RPC block response. It does not fetch address lookup table accounts itself.

An instruction targeting an unconfigured program is ignored. The dispatcher maps configured parser results to `Event`, `NoEvent`, or `Failure`; a failure does not prevent later instructions in the transaction or block from being visited.

## Validator Instruction Ordering

The transaction parser preserves the order exposed by validator block data:

1. Visit each outer instruction in `message.instructions` order.
2. Immediately after an outer instruction, visit the associated `meta.innerInstructions` group in its recorded array order.
3. Continue with the next outer instruction.

The `index` on an inner-instruction group associates that group with its outer instruction. Duplicate groups and groups referencing an outer index that does not exist are structural errors.

`stackHeight` is retained as output metadata but is not used to sort instructions. The parser trusts the validator-provided inner-instruction order and uses stack height only to associate a CPI with its immediate calling instruction. This parent context allows event parsers such as PumpSwap to combine authoritative event amounts with mints carried by the parent swap instruction.

`execution_ordinal` starts at zero for each transaction and increments for every visited outer or inner instruction, including instructions that are later filtered because their program is unconfigured or their parser returns no event. Visible output ordinals may therefore contain gaps.

## Ordered Output Contract

The public output hierarchy is:

```text
BlockEvents
  transactions: Vec<TransactionEvents>
    signature
    transaction_index
    instructions: Vec<InstructionEvents>
      program_id
      outer_instruction_index
      inner_instruction_index
      stack_height
      execution_ordinal
      result: Result<ParsedEvent, InstructionParseFailure>
```

The vectors preserve block order and transaction execution order. Source indexes always refer to the original block or transaction; filtering does not renumber them.

The output omits:

- Failed Solana transactions.
- Instructions for unconfigured programs.
- Configured instructions that return `Ok(None)`.
- Transactions left with no event or instruction-scoped failure.

`InstructionEvents::result` keeps a detected event or a recoverable instruction-scoped failure at the instruction's ordered position. These failures include instruction normalization errors and errors returned by a protocol parser.

Each `TransactionEvents` also includes `token_decimals`, a mint-address map collected from the transaction's `meta.preTokenBalances` and `meta.postTokenBalances`. This preserves the decimals required to persist raw `TokenSwap` amounts without making protocol parsers depend on token account balance data.

Structural errors are different. An invalid block shape or a malformed successful transaction returns `BlockParseError` or `TransactionParseError` and aborts the current `parse_block` call. Structural errors are not embedded in `BlockEvents`.

## Input and Real-Time Boundaries

`BlockParser` is synchronous and source-independent. Its `serde_json::Value` input boundary allows a file loader, an RPC poller, or a future real-time block stream to use the same parsing pipeline.

A source adapter is responsible for I/O, decompression or transport, JSON decoding, and extracting the block from any transport envelope. For example, an adapter receiving a JSON-RPC response must pass `response["result"]`, while an adapter receiving the block directly passes that value unchanged.

The intended lifecycle is:

```rust
let parser = BlockParser::from_env()?;

for block in block_source {
    let events = parser.parse_block(&block)?;
    // Persist or publish events and instruction-scoped failures in order.
}
```

The future real-time source must not rebuild parser configuration or the registry for each block. Retry, reconnect, commitment, malformed-payload, and persistence policies belong to the source application, not to `BlockParser`.

`util/block_loader` is one file-oriented input adapter and uses the same `serde_json::Value` boundary. It is not a runtime dependency of `parser`; the parser crate uses it only in tests.

## Adding a Protocol Parser

To add a protocol to the standard configured pipeline:

1. Use `doc/DEX.md` and `doc/MEME.md` as candidate-discovery references when relevant, then follow `doc/protocol_parsers.md` and identify versioned official layouts plus successful on-chain fixtures.
2. Implement a crate under `programs/` that depends on `common` and implements `InstructionParser`.
3. Expose the protocol program ID and ensure `program_id()` returns it.
4. Add the protocol crate to the workspace and to `parser` dependencies.
5. Add its canonical configuration name to `ParserName` and strict string parsing.
6. Add its constructor to the registry factory.
7. Test configuration, program-ID dispatch, event decoding, malformed data, and real block or transaction fixtures.
8. Update `.env.example`, this document, and `ROADMAP.md` to reflect the newly supported name.

`ParserRegistry::from_parsers` can inject a custom implementation without steps 4 through 6, but that implementation will not be selectable through the standard `PARSERS` configuration.
