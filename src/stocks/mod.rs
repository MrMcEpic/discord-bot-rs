pub mod api;
pub mod embeds;

/// Starting cash balance for a fresh stock portfolio.
///
/// Also used as the baseline for total P/L calculations across the leaderboard
/// and portfolio embeds.
pub const STARTING_CASH: f64 = 1000.0;
