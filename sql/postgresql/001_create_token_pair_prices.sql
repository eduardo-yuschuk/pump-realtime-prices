CREATE TABLE token_pair_prices (
    liquidity_provider_address VARCHAR(44) PRIMARY KEY,
    liquidity_provider_kind TEXT NOT NULL
        CHECK (liquidity_provider_kind IN ('amm', 'bonding_curve')),
    base_token_address VARCHAR(44) NOT NULL,
    quote_token_address VARCHAR(44) NOT NULL,
    base_amount NUMERIC(20, 0) NOT NULL CHECK (base_amount > 0),
    quote_amount NUMERIC(20, 0) NOT NULL CHECK (quote_amount > 0),
    base_token_decimals SMALLINT NOT NULL
        CHECK (base_token_decimals BETWEEN 0 AND 255),
    quote_token_decimals SMALLINT NOT NULL
        CHECK (quote_token_decimals BETWEEN 0 AND 255),
    price NUMERIC GENERATED ALWAYS AS (
        (quote_amount * POWER(10::NUMERIC, base_token_decimals))
        / (base_amount * POWER(10::NUMERIC, quote_token_decimals))
    ) STORED,
    CHECK (base_token_address <> quote_token_address)
);

COMMENT ON TABLE token_pair_prices IS
    'Latest token pair price for each AMM or bonding curve address.';
COMMENT ON COLUMN token_pair_prices.base_amount IS
    'Raw base token amount before applying mint decimals.';
COMMENT ON COLUMN token_pair_prices.quote_amount IS
    'Raw quote token amount before applying mint decimals.';
COMMENT ON COLUMN token_pair_prices.price IS
    'Quote token units per base token unit.';
