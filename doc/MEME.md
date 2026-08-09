# Solana Memecoin Platform Landscape

Research snapshot contributed on 2026-08-09. Market-share estimates were attributed to launchpad trading-volume summaries from OKX, Coin Bureau, and DEXTools in August 2026; no immutable source dataset is retained in this repository.

## Purpose and Limitations

This document identifies launchpad and bonding-curve protocols that may require future token-discovery or swap parsers. Market-share estimates, project names, graduation routes, and documentation availability are volatile and must be rechecked before implementation.

An "indexer-friendly" rating is preliminary discovery evidence only. It does not replace an official versioned IDL, program source, deployed program ID verification, or successful on-chain fixtures required by the [protocol parser source policy](protocol_parsers.md).

## Market-Share Snapshot

| # | Platform | Estimated market share | Model | Indexer-friendly reference |
|---|---|---|---|---|
| 1 | **pump.fun** | Approximately 73-77% | Fixed bonding curve that graduates to PumpSwap | Confirmed: [pump-public-docs](https://github.com/pump-fun/pump-public-docs/tree/main/idl) |
| 2 | **LetsBonk** or **bonk.fun** | Approximately 8-15% | BONK ecosystem launchpad built on Raydium LaunchLab and graduating to Raydium CPMM | Confirmed: [LaunchLab documentation](https://docs.raydium.io/products/launchlab/overview); an IDL was reported in Raydium SDK v2 under `src/raydium/launchpad/` |
| 3 | **Meteora DBC** | Approximately 8% | Configurable dynamic bonding-curve infrastructure used by launchpads and projects | Confirmed: [program repository](https://github.com/MeteoraAg/dynamic-bonding-curve), [SDK](https://github.com/MeteoraAg/dynamic-bonding-curve-sdk), [documentation](https://docs.meteora.ag/integration/dynamic-bonding-curve-dbc-integration/) |
| 4 | **Believe** | Niche and reported growing | Token creation initiated through commands on X | Not found: no official on-chain program IDL or instruction documentation identified |
| 5 | **Moonshot**, rebranded as **Moonit** | Niche | Simplified, mobile-oriented token launch application | Confirmed: [moonit-sdk](https://github.com/gomoonit/moonit-sdk), [documentation](https://docs.moon.it/docs/bot-sdk) |
| 6 | **Boop** | Niche | Launchpad with alternative bonding-curve mechanics | Partial: IDLs were reported only in third-party libraries; no official source was confirmed |

## Platform Notes

### pump.fun

- The snapshot identifies pump.fun as the dominant launchpad by trading volume.
- Tokens that reach the protocol threshold graduate from the bonding curve to PumpSwap liquidity.
- Pump.fun token-discovery and PumpSwap trade parsers are already implemented in this repository.

### LetsBonk and bonk.fun

- The platform uses the BONK ecosystem and Raydium LaunchLab.
- Its graduation route makes both launchpad events and destination Raydium liquidity relevant to indexing.

### Meteora DBC

- DBC is configurable launch infrastructure rather than only an end-user launchpad.
- A future integration may need to distinguish protocol-level curve events from application-specific behavior built on top of DBC.

### Believe

- Token creation is initiated through social commands, so part of the workflow may occur off-chain before an on-chain transaction is submitted.
- The absence of an official program-level source in this snapshot makes independent on-chain discovery necessary before parser work.

### Moonshot and Moonit

- The platform is associated with Dexscreener and emphasizes a simplified mobile experience.
- The linked SDK and documentation are discovery starting points whose exact program versions still require verification.

### Boop

- Third-party IDLs may help identify candidate program addresses and layouts.
- They cannot be treated as authoritative parser specifications without official confirmation or documented discrepancies plus extensive on-chain validation.

## Relationship to DEX Protocols

Launchpads commonly route graduated liquidity to an AMM:

- pump.fun routes to PumpSwap.
- LetsBonk routes to Raydium.
- Other launchpads may route to Raydium or Meteora depending on their configuration.

This relationship means launchpad indexing and DEX indexing should share token and pool identities while preserving protocol-specific event provenance. See the [Solana DEX landscape](DEX.md) for candidate destination protocols.

## Parser Research Guidance

Before adding a launchpad parser:

1. Confirm whether token creation, curve trading, migration, and graduation use one program or several programs.
2. Identify authoritative events for token discovery and executed trades rather than relying on outer instruction limits.
3. Determine the destination DEX and whether migration events expose the destination pool.
4. Pin official source versions and validate each modeled event against successful on-chain data.
5. Keep off-chain platform behavior separate from facts derived from Solana instructions and accounts.

## Open Research Questions

- Decide whether this reference should cover only launchpads or also notable memecoin assets.
- Decide whether protocol-risk topics such as rug pulls, sniping, and bonding-curve manipulation belong in indexing scope or in separate operational documentation.
