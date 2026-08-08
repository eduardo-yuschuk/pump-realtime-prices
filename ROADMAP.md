# Roadmap

## Development

- [x] Define the public traversable JSON data representation for `util/block_loader` without requiring a complete block struct mapping.
- [x] Implement a public `util/block_loader` function that loads an uncompressed block JSON file from disk.
- [x] Implement a public `util/block_loader` function that loads a gzip-compressed block JSON file from disk.
- [x] Implement a public `util/block_loader` function that loads a block from an in-memory JSON string.
- [x] Add tests for all public `util/block_loader` loading functions and their error handling.
- [x] Define a public interface for all the implementations of instruction parser (parse_instruction).
- [ ] Implement the Pump.fun parser.
- [ ] Add tests for all public Pump.fun parsing functions and their error handling.
- [ ] Implement the PumpSwap parser.
- [ ] Add tests for all public PumpSwap parsing functions and their error handling.

## Documentation

- [x] Ensure all repository documentation is written in English.
- [x] Document the complete-block compressed JSON storage convention.
- [x] Document the public `block_loader` crate contract.

## Repository Maintenance

- [x] Add Rust build artifact exclusions.
- [x] Add the `commit-push` workflow skill.
- [x] Add project-scoped Solana MCP server configuration.

## Utilities

- [x] Create the block_loader library crate.
