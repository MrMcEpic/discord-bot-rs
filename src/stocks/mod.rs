pub mod api;
pub mod embeds;

use rust_decimal::Decimal;

/// Starting cash balance for a fresh stock portfolio.
///
/// Also used as the baseline for total P/L calculations across the leaderboard
/// and portfolio embeds.
///
/// `Decimal::new` is `const`, so this stays a compile-time constant even after
/// the float → Decimal migration. The value is `1000.0000` (mantissa 10_000_000,
/// scale 4) to match the `NUMERIC(18, 4)` column definition for portfolio
/// money columns.
pub const STARTING_CASH: Decimal = Decimal::from_parts(10_000_000, 0, 0, false, 4);
