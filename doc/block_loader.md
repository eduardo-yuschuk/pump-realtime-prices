# `block_loader` Crate

`util/block_loader` is the shared block-loading library for the workspace. Its public functions are intended for consumption by the other Rust crates and must not be limited to a command-line or tool-specific interface.

## Scope

The crate loads Solana block data from the following sources:

- Uncompressed JSON files on disk.
- Gzip-compressed JSON files on disk, including complete blocks stored as `sample_data/<slot>.json.gz`.
- In-memory JSON strings.

It must expose the loaded data through an efficient traversable representation suitable for high-frequency indexing. The implementation must not require mapping every block field to a Rust struct. A generic JSON representation, or another representation with equivalent traversal and performance characteristics, is acceptable.

## Public API Requirements

The crate must provide public functions for each supported source:

- Load an uncompressed JSON file from a filesystem path.
- Load a gzip-compressed JSON file from a filesystem path.
- Load JSON from an in-memory string.

All functions must return the same traversable data representation so that consuming crates can process block data independently of its source.

The implemented API uses `serde_json::Value` and exposes `load_json_file`,
`load_gzip_json_file`, and `load_json_str`.

## Consumers

Other workspace crates use `block_loader` as their common data-loading boundary. Source-specific decompression and JSON parsing belong in this crate; consuming crates should receive already loaded block data and focus on indexing logic.
