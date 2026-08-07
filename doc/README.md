# Project Documentation

## Complete Block Storage

Complete Solana blocks are stored in `sample_data/` as gzip-compressed JSON files. The filename is the block slot followed by the `.json.gz` extension:

```text
sample_data/<slot>.json.gz
```

For example, slot `123456789` is stored in `sample_data/123456789.json.gz`.

`tools/get_json_block.sh` is the downloader for complete blocks and writes files that follow this convention.
