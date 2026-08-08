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

## Pump.fun Baseline

The Pump.fun parser uses the official [`pump-fun/pump-public-docs`](https://github.com/pump-fun/pump-public-docs) repository, including `idl/pump.json`, as its primary specification. Its supported layouts must also be checked against real successful Pump.fun instructions before they are considered complete.
