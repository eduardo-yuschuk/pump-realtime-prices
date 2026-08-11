SELECT time, price, volume
FROM real_time_prices
WHERE liquidity_provider_address = 'H2xHTeXwyGDtiuxS6JCfRxh4G5PKQQct1XTgGmySbd6F'
AND quote_token_address = 'So11111111111111111111111111111111111111112'
ORDER BY time ASC;
