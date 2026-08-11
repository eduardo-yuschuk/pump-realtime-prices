SELECT
    liquidity_provider_address,
    liquidity_provider_kind,
    base_token_address,
    quote_token_address,
    time,
    argMinMerge(open) AS open,
    maxMerge(high) AS high,
    minMerge(low) AS low,
    argMaxMerge(close) AS close,
    sumMerge(volume) AS volume
FROM price_bars_1m
GROUP BY
    liquidity_provider_address,
    liquidity_provider_kind,
    base_token_address,
    quote_token_address,
    time
ORDER BY
    time ASC,
    base_token_address ASC,
    quote_token_address ASC,
    liquidity_provider_address ASC
LIMIT 100;
