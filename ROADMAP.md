# Roadmap

## Development

- [x] Define the public traversable JSON data representation for `util/block_loader` without requiring a complete block struct mapping.
- [x] Implement a public `util/block_loader` function that loads an uncompressed block JSON file from disk.
- [x] Implement a public `util/block_loader` function that loads a gzip-compressed block JSON file from disk.
- [x] Implement a public `util/block_loader` function that loads a block from an in-memory JSON string.
- [x] Add tests for all public `util/block_loader` loading functions and their error handling.
- [x] Define shared token discovery and token swap outputs and a public interface for all instruction parsers (`parse_instruction`).
- [x] Implement the Pump.fun parser. It must return the applicable shared `ParsedEvent` variant, or `Ok(None)` when the instruction produces no modeled event.
- [x] Add tests for all public Pump.fun parsing functions and their error handling.
- [ ] Create a `parser` workspace crate with `block`, `transaction`, `instruction`, `registry`, `config`, `event`, and `error` modules.
- [ ] Refactor the protocol `InstructionParser` interface to expose its program ID through an object-safe method so configured protocol parsers can be stored as trait objects.
- [ ] Define the public ordered parser output model: `BlockEvents`, `TransactionEvents`, `InstructionEvents`, and instruction-scoped parse failures. Include the transaction signature, transaction index, outer and inner instruction indexes, CPI `stackHeight`, and a transaction-wide execution ordinal.
- [ ] Implement strict parser configuration loading with `dotenvy`. `BlockParser::from_env` must load `PARSERS` once, reject a missing or empty value, unknown parser names, and duplicate names, and support explicit configuration injection for tests and non-environment callers.
- [ ] Implement the protocol parser registry and factories for every supported `PARSERS` name. Instantiate each selected protocol parser once and index the resulting trait objects by program ID.
- [ ] Implement `InstructionDispatcher` to accept one normalized instruction, select its configured protocol parser by program ID, and return either its detected event, no event, or an instruction-scoped parse failure without aborting the remaining block.
- [ ] Implement `TransactionParser` for one transaction represented as `serde_json::Value`. Ignore failed transactions, resolve account indexes as static keys followed by loaded writable and loaded readonly addresses, decode base58 instruction data, and invoke `InstructionDispatcher` for outer and inner instructions in validator evaluation order.
- [ ] Preserve validator instruction order by visiting outer instructions in message order and each outer instruction's recorded inner instructions in order, retaining `stackHeight` and assigning a monotonically increasing execution ordinal across the transaction.
- [ ] Implement `BlockParser` for a block represented as `serde_json::Value`. Iterate transactions in block order, delegate successful transactions to `TransactionParser`, and return only transactions and instructions containing detected events or parse failures while preserving their source indexes and execution order.
- [ ] Add configuration and registry tests covering `.env` loading, explicit injection, all supported parser names, missing configuration, unknown names, duplicates, and program-ID dispatch.
- [ ] Add `InstructionDispatcher` tests covering configured and unconfigured programs, event propagation, no-event results, and recoverable protocol parser failures.
- [ ] Add `TransactionParser` tests covering legacy and versioned transactions, loaded address resolution, base58 decoding, nested CPI ordering by `stackHeight`, failed transaction filtering, malformed transaction data, and partial event/error collection.
- [ ] Add `BlockParser` tests against real block fixtures covering transaction order, instruction execution order, multiple protocol events, filtered empty transactions, and continued parsing after instruction-scoped failures.
- [ ] Implement the PumpSwap parser. It must return the applicable shared `ParsedEvent` variant, or `Ok(None)` when the instruction produces no modeled event.
- [ ] Add tests for all public PumpSwap parsing functions and their error handling.

## Documentation

- [x] Ensure all repository documentation is written in English.
- [x] Document the complete-block compressed JSON storage convention.
- [x] Document the public `block_loader` crate contract.
- [ ] Document the block parsing architecture, `PARSERS` configuration, parser registry lifecycle, validator instruction ordering, ordered event/error output contract, and the boundary through which a future real-time block stream will invoke `BlockParser`.
- [ ] Add an English `.env.example` documenting the strict comma-separated `PARSERS=pumpfun,pumpswap` configuration.

## Repository Maintenance

- [x] Add Rust build artifact exclusions.
- [x] Add the `commit-push` workflow skill.
- [x] Add project-scoped Solana MCP server configuration.

## Utilities

- [x] Create the block_loader library crate.
