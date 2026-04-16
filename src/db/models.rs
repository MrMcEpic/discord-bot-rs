use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::FromRow;

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct Tempban {
	pub id: i32,
	pub guild_id: String,
	pub user_id: String,
	pub moderator_id: String,
	pub reason: Option<String>,
	pub banned_at: DateTime<Utc>,
	pub expires_at: DateTime<Utc>,
	pub unbanned: bool,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct GuildSettings {
	pub guild_id: String,
	pub audit_log_channel_id: Option<String>,
	pub dj_role_id: Option<String>,
	pub dj_mode_enabled: bool,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct StockPortfolio {
	pub guild_id: String,
	pub user_id: String,
	pub cash_balance: Decimal,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct StockHolding {
	pub id: i32,
	pub guild_id: String,
	pub user_id: String,
	pub symbol: String,
	pub quantity: Decimal,
	pub avg_cost: Decimal,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct StockTransaction {
	pub id: i32,
	pub guild_id: String,
	pub user_id: String,
	pub symbol: String,
	pub action: String,
	pub quantity: Decimal,
	pub price_per_share: Decimal,
	pub total_amount: Decimal,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct StockPriceCache {
	pub symbol: String,
	pub price: f64,
	pub prev_close: f64,
	pub change_pct: f64,
	pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
pub struct MemberActivity {
	pub guild_id: String,
	pub user_id: String,
	pub message_count: i32,
	pub first_seen: DateTime<Utc>,
	pub promoted: bool,
}
