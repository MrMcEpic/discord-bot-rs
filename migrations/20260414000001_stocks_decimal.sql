-- Migrate stock portfolio money columns from DOUBLE PRECISION to NUMERIC(18, 4).
--
-- Float math drifts cents over many fractional-share trades — Tier 1.2 fixed
-- the reset/buy/sell row-locking race, but `avg_cost * quantity + price * qty`
-- still accumulated rounding error on every trade. Switching to NUMERIC makes
-- the arithmetic exact at the database layer; the Rust side now uses
-- `rust_decimal::Decimal` end-to-end so values round-trip without ever touching
-- a binary float.
--
-- The chosen scale is `(18, 4)`: 14 digits of integer part (ample for any
-- realistic virtual portfolio, even with hyperinflated meme tickers) and 4
-- decimal places (penny granularity plus a bit of headroom for sub-penny
-- fractional-share quotients like `dollar_amount / quote.price`).
--
-- `USING <col>::NUMERIC(18, 4)` lets PostgreSQL re-cast existing
-- DOUBLE PRECISION values losslessly into the new type. Defaults are restated
-- as typed NUMERIC literals so future inserts use the new representation.
--
-- The `stock_price_cache` table is intentionally NOT migrated. It is a
-- short-lived (60s TTL) display cache that never feeds portfolio arithmetic
-- without first going through `f64_to_decimal` at the Rust API boundary, so
-- holding it as DOUBLE PRECISION costs nothing and avoids needing to migrate
-- the `change_pct` percentage column too.

ALTER TABLE stock_portfolios
    ALTER COLUMN cash_balance TYPE NUMERIC(18, 4)
        USING cash_balance::NUMERIC(18, 4),
    ALTER COLUMN cash_balance SET DEFAULT 1000.0000;

ALTER TABLE stock_holdings
    ALTER COLUMN quantity TYPE NUMERIC(18, 4)
        USING quantity::NUMERIC(18, 4),
    ALTER COLUMN quantity SET DEFAULT 0.0000,
    ALTER COLUMN avg_cost TYPE NUMERIC(18, 4)
        USING avg_cost::NUMERIC(18, 4),
    ALTER COLUMN avg_cost SET DEFAULT 0.0000;

ALTER TABLE stock_transactions
    ALTER COLUMN quantity TYPE NUMERIC(18, 4)
        USING quantity::NUMERIC(18, 4),
    ALTER COLUMN price_per_share TYPE NUMERIC(18, 4)
        USING price_per_share::NUMERIC(18, 4),
    ALTER COLUMN total_amount TYPE NUMERIC(18, 4)
        USING total_amount::NUMERIC(18, 4);
