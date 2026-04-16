use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;

use crate::db::queries;

#[derive(Debug, Clone, Deserialize)]
struct FinnhubQuote {
	c: f64,  // current price
	pc: f64, // previous close
	dp: f64, // percent change
}

#[derive(Debug, Clone)]
pub struct StockQuote {
	pub symbol: String,
	pub price: Decimal,
	pub prev_close: Decimal,
	pub change_pct: Decimal,
}

/// Convert an f64 coming from the upstream API into a Decimal rounded to the
/// 4-decimal-place precision used by the `NUMERIC(18, 4)` portfolio columns.
/// Falls back to `Decimal::ZERO` if the float is NaN or infinite; callers
/// already treat a zero price as an upstream error.
fn f64_to_decimal(v: f64) -> Decimal {
	Decimal::from_f64(v)
		.map(|d| d.round_dp(4))
		.unwrap_or(Decimal::ZERO)
}

pub async fn get_quote(
	http_client: &reqwest::Client,
	db: &PgPool,
	api_key: &str,
	symbol: &str,
) -> Result<StockQuote, String> {
	let symbol = symbol.to_uppercase();

	// Check cache first. The cache table is still DOUBLE PRECISION — it only
	// feeds display and is never arithmetic-fed into portfolio columns without
	// going through `f64_to_decimal` first.
	if let Ok(Some(cached)) = queries::get_cached_price(db, &symbol).await {
		return Ok(StockQuote {
			symbol: symbol.clone(),
			price: f64_to_decimal(cached.price),
			prev_close: f64_to_decimal(cached.prev_close),
			change_pct: f64_to_decimal(cached.change_pct),
		});
	}

	// Fetch from Finnhub
	let resp = http_client
		.get("https://finnhub.io/api/v1/quote")
		.query(&[("symbol", symbol.as_str())])
		.header("X-Finnhub-Token", api_key)
		.send()
		.await
		.map_err(|e| format!("Failed to fetch stock price: {e}"))?;

	if !resp.status().is_success() {
		return Err(format!("Finnhub API returned status {}", resp.status()));
	}

	let quote: FinnhubQuote = resp
		.json()
		.await
		.map_err(|e| format!("Failed to parse Finnhub response: {e}"))?;

	if quote.c == 0.0 {
		return Err(format!(
            "Could not find stock symbol **{symbol}**. Make sure you're using a valid US stock ticker (e.g., AAPL, TSLA, MSFT)."
        ));
	}

	// Cache the raw f64 values — the cache table stays f64.
	let _ = queries::upsert_cached_price(db, &symbol, quote.c, quote.pc, quote.dp).await;

	Ok(StockQuote {
		symbol,
		price: f64_to_decimal(quote.c),
		prev_close: f64_to_decimal(quote.pc),
		change_pct: f64_to_decimal(quote.dp),
	})
}
