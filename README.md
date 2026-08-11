# pump_realtime_prices

Rust workspace for shared components and integrations with the Pump.fun and PumpSwap programs.

## Build

Build all workspace crates from the repository root:

```sh
cargo build --workspace
```

For an optimized build:

```sh
cargo build --workspace --release
```

## Tests

Run the unit and documentation tests for all crates:

```sh
cargo test --workspace
```

## Structure

```text
.
├── Cargo.toml
├── ROADMAP.md
├── common/
│   └── src/lib.rs
├── programs/
│   └── pump/
│       ├── pumpfun/
│       │   └── src/lib.rs
│       └── pumpswap/
│           └── src/lib.rs
├── sample_data/
│   ├── block_empty.json
│   ├── block_with_transaction.json
│   └── README.md
├── tools/
│   └── get_json_block.sh
├── util/
│   └── block_loader/
│       └── src/lib.rs
└── doc/
    └── README.md
```

- `common`: shared types and utilities for the integrations.
- `programs/pump/pumpfun`: crate for the Pump.fun integration.
- `programs/pump/pumpswap`: crate for the PumpSwap integration.
- `util/block_loader`: library crate for loading Solana blocks.
- `saver`: library crate that stores the latest parsed token pair price for each supported liquidity provider.

## Database Configuration

`saver` reads these PostgreSQL variables from the process environment. The standalone indexer loads them from `.env` during startup:

```dotenv
DB_PORT=5432
DB_USERNAME=
DB_NAME=
DB_PASSWORD=
DB_HOST=
DB_DISABLE_SSL=false
```

Set `DB_DISABLE_SSL=false` to require a TLS connection using WebPKI root certificates. Set it to `true` only for a trusted local PostgreSQL instance.

`saver` writes every parsed swap and the latest price for each liquidity provider to ClickHouse in batches per block. Configure its HTTP endpoint and credentials:

```dotenv
CLICKHOUSE_URL=http://localhost:8123
CLICKHOUSE_USERNAME=
CLICKHOUSE_DATABASE=default
CLICKHOUSE_PASSWORD=
```

For every `TokenSwap`, `saver` stores the input mint and raw input amount as the base side, and the output mint and raw output amount as the quote side.

## Sample Data

`sample_data/` contains JSON fixtures of Solana blocks for local testing. It includes one block without transactions and another with a minimal successful transaction. See `sample_data/README.md` for details about each fixture.

## Complete Block Storage

Complete blocks downloaded by `tools/get_json_block.sh` are stored in `sample_data/` as gzip-compressed JSON files. Each file is named after its Solana slot:

```text
sample_data/<slot>.json.gz
```

For example, the full block at slot `123456789` is stored as `sample_data/123456789.json.gz`. This naming convention makes the slot directly identifiable from the filename and distinguishes complete block files from the smaller JSON fixtures.

## Documentation

`doc/` contains general project documentation, including the complete-block storage convention.
