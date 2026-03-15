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
    pub price: f64,
    pub prev_close: f64,
    pub change_pct: f64,
}

pub async fn get_quote(
    http_client: &reqwest::Client,
    db: &PgPool,
    api_key: &str,
    symbol: &str,
) -> Result<StockQuote, String> {
    let symbol = symbol.to_uppercase();

    // Check cache first
    if let Ok(Some(cached)) = queries::get_cached_price(db, &symbol).await {
        return Ok(StockQuote {
            symbol: symbol.clone(),
            price: cached.price,
            prev_close: cached.prev_close,
            change_pct: cached.change_pct,
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

    // Cache the price
    let _ = queries::upsert_cached_price(db, &symbol, quote.c, quote.pc, quote.dp).await;

    Ok(StockQuote {
        symbol,
        price: quote.c,
        prev_close: quote.pc,
        change_pct: quote.dp,
    })
}
