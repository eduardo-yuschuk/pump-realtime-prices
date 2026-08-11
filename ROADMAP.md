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
- [x] Create a `parser` workspace crate with `block`, `transaction`, `instruction`, `registry`, `config`, `event`, and `error` modules.
- [x] Refactor the protocol `InstructionParser` interface to expose its program ID through an object-safe method so configured protocol parsers can be stored as trait objects.
- [x] Define the public ordered parser output model: `BlockEvents`, `TransactionEvents`, `InstructionEvents`, and instruction-scoped parse failures. Include the transaction signature, transaction index, outer and inner instruction indexes, CPI `stackHeight`, and a transaction-wide execution ordinal.
- [x] Implement strict parser configuration loading with `dotenvy`. `BlockParser::from_env` must load `PARSERS` once, reject a missing or empty value, unknown parser names, and duplicate names, and support explicit configuration injection for tests and non-environment callers.
- [x] Implement the protocol parser registry and factories for every supported `PARSERS` name. Instantiate each selected protocol parser once and index the resulting trait objects by program ID.
- [x] Implement `InstructionDispatcher` to accept one normalized instruction, select its configured protocol parser by program ID, and return either its detected event, no event, or an instruction-scoped parse failure without aborting the remaining block.
- [x] Implement `TransactionParser` for one transaction represented as `serde_json::Value`. Ignore failed transactions, resolve account indexes as static keys followed by loaded writable and loaded readonly addresses, decode base58 instruction data, and invoke `InstructionDispatcher` for outer and inner instructions in validator evaluation order.
- [x] Preserve validator instruction order by visiting outer instructions in message order and each outer instruction's recorded inner instructions in order, retaining `stackHeight` and assigning a monotonically increasing execution ordinal across the transaction.
- [x] Implement `BlockParser` for a block represented as `serde_json::Value`. Iterate transactions in block order, delegate successful transactions to `TransactionParser`, and return only transactions and instructions containing detected events or parse failures while preserving their source indexes and execution order.
- [x] Add configuration and registry tests covering `.env` loading, explicit injection, all supported parser names, missing configuration, unknown names, duplicates, and program-ID dispatch.
- [x] Add `InstructionDispatcher` tests covering configured and unconfigured programs, event propagation, no-event results, and recoverable protocol parser failures.
- [x] Add `TransactionParser` tests covering legacy and versioned transactions, loaded address resolution, base58 decoding, nested CPI ordering by `stackHeight`, failed transaction filtering, malformed transaction data, and partial event/error collection.
- [x] Add `BlockParser` tests against real block fixtures covering transaction order, instruction execution order, multiple protocol events, filtered empty transactions, and continued parsing after instruction-scoped failures.
- [x] Implement `tools/get_json_transaction.sh` to accept a transaction signature, fetch its full JSON transaction from Solana RPC, and store the formatted response as `sample_data/<signature>.json.gz`.
- [x] Create a `util/transaction_loader` workspace library crate with the same public traversable `serde_json::Value` boundary used by `block_loader`.
- [x] Implement a public `transaction_loader` function that loads a gzip-compressed transaction JSON file from disk.
- [x] Implement a public `transaction_loader` function that loads an uncompressed transaction JSON file from disk.
- [x] Implement a public `transaction_loader` function that loads a transaction from an in-memory JSON string.
- [x] Add tests for all public `transaction_loader` functions and their error handling using real and malformed transaction fixtures.
- [x] Add parser configuration resolution that accepts an optional comma-separated protocol-list override. A provided override must take precedence over `PARSERS`; when absent, configuration must continue loading from `.env`.
- [x] Add tests for protocol-list override precedence, strict validation, and `.env` fallback.
- [x] Create a visual inspection binary crate in the workspace for blocks and transactions. Its initial transaction mode must accept a transaction signature and protocol name, load `sample_data/<signature>.json.gz` through `transaction_loader`, parse it with only the requested protocol enabled, and print the detected ordered events and parse failures.
- [x] Add tests for the visual transaction inspector covering argument validation, missing fixtures, protocol selection, and rendered parsed events.
- [x] Implement the PumpSwap parser. It must return the applicable shared `ParsedEvent` variant, or `Ok(None)` when the instruction produces no modeled event.
- [x] Add tests for all public PumpSwap parsing functions and their error handling.
- [x] Organize PostgreSQL and ClickHouse schemas for latest token-pair prices and real-time one-minute bars.
- [x] Create a standalone WebSocket indexer that subscribes to finalized full blocks, reconnects, parses them, and prints result summaries with parsing time.
- [x] Add a root script to run the standalone indexer.
- [x] Create a `saver` workspace library that persists parsed `TokenSwap` events as latest token pair prices in PostgreSQL.
- [x] Enrich parser transaction output with token mint decimals from RPC token balances and integrate `saver` into the standalone indexer.
- [x] Batch PostgreSQL token pair price upserts per block while preserving the last swap for each liquidity provider.
- [x] Include PostgreSQL batch write duration in standalone indexer block summaries.
- [x] Batch all parsed swap prices and the latest price per liquidity provider into ClickHouse alongside PostgreSQL latest-price upserts.

## Documentation

- [x] Ensure all repository documentation is written in English.
- [x] Document the complete-block compressed JSON storage convention.
- [x] Document the public `block_loader` crate contract.
- [x] Document the block parsing architecture, `PARSERS` configuration, parser registry lifecycle, validator instruction ordering, ordered event/error output contract, and the boundary through which a future real-time block stream will invoke `BlockParser`.
- [x] Add an English `.env.example` documenting the strict comma-separated `PARSERS=pumpfun,pumpswap` configuration.
- [x] Define Solana JSON-RPC and WebSocket endpoint variables in `.env.example`.
- [x] Add externally contributed DEX and memecoin protocol landscape references for prioritizing future parser integrations.
- [x] Document PostgreSQL and ClickHouse environment configuration for price persistence.

## Repository Maintenance

- [x] Rename the project to `pump_realtime_prices`.
- [x] Add Rust build artifact exclusions.
- [x] Add the `commit-push` workflow skill.
- [x] Add project-scoped Solana MCP server configuration.

## Utilities

- [x] Create the block_loader library crate.
