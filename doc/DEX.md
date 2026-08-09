# Solana DEX Landscape

Research snapshot contributed on 2026-08-09. TVL figures were attributed to the [DeFiLlama Solana page](https://defillama.com/chain/Solana).

## Purpose and Limitations

This document is a discovery and prioritization reference for future protocol parsers. Rankings, TVL figures, project activity, and documentation availability are time-sensitive and must be rechecked before planning an integration.

An "indexer-friendly" rating means that the contribution identified a public IDL, source repository, SDK, or instruction documentation. It does not prove that a source is official, current, complete, or compatible with observed on-chain data. Every implementation must independently satisfy the [protocol parser source policy](protocol_parsers.md).

## TVL Snapshot

Indexer-friendly status:

- **Confirmed:** a potentially useful public IDL or program-level source was identified.
- **Partial:** only general documentation, an SDK, or an unconfirmed source was identified.
- **Not found:** no public program-level source was identified in this research snapshot.

| # | Protocol | Model | Solana TVL | Indexer-friendly reference |
|---|---|---|---|---|
| 1 | **Raydium** | Hybrid AMM with pools and OpenBook integration | $839M | Confirmed: [raydium-idl](https://github.com/raydium-io/raydium-idl), [documentation](https://docs.raydium.io/sdk-api/anchor-idl) |
| 2 | **PumpSwap** | Native pump.fun post-bonding-curve AMM | $249M | Confirmed: [pump-public-docs](https://github.com/pump-fun/pump-public-docs/tree/main/idl) |
| 3 | **Orca** | Concentrated liquidity AMM through Whirlpools | $243M | Confirmed: [whirlpools](https://github.com/orca-so/whirlpools), [documentation](https://docs.orca.so/), [IDL resources](https://dev.orca.so/More%20Resources/IDL/) |
| 4 | **Meteora** | DLMM, DAMM v1/v2, and vault suite | $172M DLMM plus $58M DAMM and vaults | Confirmed: [dlmm-sdk](https://github.com/MeteoraAg/dlmm-sdk), [documentation](https://docs.meteora.ag/developer-guide/guides/dlmm/overview) |
| 5 | **Saber** | Stable-swap AMM for pegged assets | $4.3M | Confirmed: [stable-swap](https://github.com/saber-hq/stable-swap); project reported inactive |
| 6 | **FluxBeam** | AMM with Token-2022 support | $1.8M | Partial: [general documentation](https://docs.fluxbeam.xyz/); no program IDL confirmed |
| 7 | **OpenBook** | On-chain central limit order book derived from Serum | $1.1M | Confirmed: [openbook-v2](https://github.com/openbook-dex/openbook-v2), [IDL](https://github.com/openbook-dex/openbook-v2/blob/master/idl/openbook_v2.json) |
| 8 | **Phoenix** | On-chain central limit order book | $0.9M | Confirmed: [phoenix-v1](https://github.com/Ellipsis-Labs/phoenix-v1), [phoenix-sdk](https://github.com/Ellipsis-Labs/phoenix-sdk) |
| 9 | **Aldrin** | AMM | $0.4M | Partial: [aldrin-sdk](https://github.com/aldrin-labs/aldrin-sdk); project reported inactive since 2023 and no current IDL confirmed |
| 10 | **Crema Finance** | Concentrated liquidity AMM | $0.12M | Confirmed: [crema-clmm-sdk-v2](https://github.com/jup-ag/crema-clmm-sdk-v2), including `clmmpool.json` |
| 11 | **Cropper** | AMM and concentrated liquidity AMM | $0.11M | Partial: [general documentation](https://docs.cropper.finance/cropperfinance); no public IDL confirmed |
| 12 | **Lifinity** | Oracle-based proactive market maker | $0.07M | Not found: no public instruction IDL identified in this snapshot |
| 13 | **Invariant** | Concentrated liquidity AMM | $0.03M | Confirmed: [organization](https://github.com/invariant-labs), [documentation](https://docs.invariant.app) |
| 14 | **GooseFX** | AMM | Approximately $0 | Confirmed: [organization](https://github.com/GooseFX1), [documentation](https://docs.goosefx.io/) |

## Protocol Notes

### Raydium

- The snapshot identifies Raydium as the largest Solana AMM by TVL.
- Raydium is also relevant to memecoin liquidity and graduation flows.

### PumpSwap

- PumpSwap was launched by the pump.fun team to retain post-bonding-curve liquidity within its ecosystem.
- Its parser is already implemented in this repository.

### Orca

- Orca's Whirlpools implement concentrated liquidity on Solana.
- Its public source and integration documentation make it a candidate for parser research.

### Meteora

- Meteora's DLMM uses discrete liquidity bins and is frequently associated with new-token pools.
- Meteora DBC also appears in the [memecoin platform landscape](MEME.md) as launch infrastructure.

### OpenBook and Phoenix

- Both protocols implement on-chain central limit order books.
- TVL is not directly comparable with AMMs because order books do not lock capital in the same way.

### Legacy and Long-Tail Protocols

Saber, Aldrin, Crema, Cropper, Lifinity, Invariant, and GooseFX may be relevant for historical coverage or inherited integrations even when current activity is limited.

## Parser Research Guidance

The references above are candidate discovery links, not parser specifications. Before implementation:

1. Confirm the deployed program IDs and active versions.
2. Locate an official, immutable IDL, source commit, or release.
3. Identify which instructions or events map to the shared event model.
4. Validate layouts against successful mainnet transactions or blocks.
5. Record unsupported versions and discrepancies before writing decoding assumptions.

Third-party or reverse-engineered IDLs may help discover layouts, but they remain secondary evidence and do not satisfy the repository policy by themselves.

## Open Research Questions

- Decide whether prioritization should use technical integration cost, fees and liquidity design, trading volume, or market share.
- Decide whether aggregators such as Jupiter belong in this DEX reference or in a separate routing-layer reference.
