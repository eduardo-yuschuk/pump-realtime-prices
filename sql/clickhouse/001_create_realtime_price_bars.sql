CREATE TABLE IF NOT EXISTS real_time_prices (
    liquidity_provider_address String,
    liquidity_provider_kind Enum8('amm' = 1, 'bonding_curve' = 2),
    base_token_address String,
    quote_token_address String,
    price Decimal(38, 18),
    volume Decimal(38, 18),
    time DateTime64(3, 'UTC'),
    sequence UInt64
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(time)
ORDER BY (liquidity_provider_address, time, sequence);

CREATE TABLE IF NOT EXISTS price_bars_1m (
    liquidity_provider_address String,
    liquidity_provider_kind Enum8('amm' = 1, 'bonding_curve' = 2),
    base_token_address String,
    quote_token_address String,
    time DateTime('UTC'),
    open AggregateFunction(
        argMin,
        Decimal(38, 18),
        Tuple(DateTime64(3, 'UTC'), UInt64)
    ),
    high AggregateFunction(max, Decimal(38, 18)),
    low AggregateFunction(min, Decimal(38, 18)),
    close AggregateFunction(
        argMax,
        Decimal(38, 18),
        Tuple(DateTime64(3, 'UTC'), UInt64)
    ),
    volume AggregateFunction(sum, Decimal(38, 18))
)
ENGINE = AggregatingMergeTree
PARTITION BY toYYYYMM(time)
ORDER BY (
    liquidity_provider_address,
    liquidity_provider_kind,
    base_token_address,
    quote_token_address,
    time
);

CREATE MATERIALIZED VIEW IF NOT EXISTS real_time_prices_to_price_bars_1m
TO price_bars_1m
AS
SELECT
    liquidity_provider_address,
    liquidity_provider_kind,
    base_token_address,
    quote_token_address,
    toDateTime(toStartOfMinute(real_time_prices.time), 'UTC') AS time,
    argMinState(price, tuple(real_time_prices.time, sequence)) AS open,
    maxState(price) AS high,
    minState(price) AS low,
    argMaxState(price, tuple(real_time_prices.time, sequence)) AS close,
    sumState(volume) AS volume
FROM real_time_prices
GROUP BY
    liquidity_provider_address,
    liquidity_provider_kind,
    base_token_address,
    quote_token_address,
    toDateTime(toStartOfMinute(real_time_prices.time), 'UTC');
