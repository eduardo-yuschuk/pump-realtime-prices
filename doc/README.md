# Project Documentation

## Complete Block Storage

Complete Solana blocks are stored in `sample_data/` as gzip-compressed JSON files. The filename is the block slot followed by the `.json.gz` extension:

```text
sample_data/<slot>.json.gz
```

For example, slot `123456789` is stored in `sample_data/123456789.json.gz`.

`tools/get_json_block.sh` is the downloader for complete blocks and writes files that follow this convention.

## Crate Documentation

- [`block_loader`](block_loader.md): shared block-loading crate contract and public API requirements.
- [Parser architecture](parser.md): parser pipeline, protocol crate boundaries, configuration, ordering, output, and real-time input contract.

## Implementation Policies

- [Protocol parser source policy](protocol_parsers.md): required authoritative sources and real-data validation for protocol integrations.

## Protocol Research References

- [Solana DEX landscape](DEX.md): time-stamped DEX candidates, protocol models, and initial public-source discovery for future swap parsers.
- [Solana memecoin platform landscape](MEME.md): time-stamped launchpad and bonding-curve candidates for future token-discovery and swap parsers.

These externally contributed snapshots are prioritization references, not implementation specifications. Their market metrics and source-availability assessments must be refreshed, and every parser must still follow the protocol parser source policy.
