use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct NytResponse {
	status: String,
	id: i64,
	print_date: String,
	categories: Vec<NytCategory>,
}

#[derive(Debug, Deserialize)]
struct NytCategory {
	title: String,
	cards: Vec<NytCard>,
}

#[derive(Debug, Deserialize)]
struct NytCard {
	content: String,
}

#[derive(Debug, Clone)]
pub struct ConnectionsPuzzle {
	pub id: i64,
	pub date: String,
	pub categories: Vec<PuzzleCategory>,
}

#[derive(Debug, Clone)]
pub struct PuzzleCategory {
	pub title: String,
	pub words: Vec<String>,
	pub difficulty: u8, // 0=yellow, 1=green, 2=blue, 3=purple
}

pub async fn fetch_puzzle(
	http_client: &reqwest::Client,
	date: &str,
) -> Result<ConnectionsPuzzle, String> {
	let url = format!("https://www.nytimes.com/svc/connections/v2/{date}.json");

	let resp = http_client
		.get(&url)
		.send()
		.await
		.map_err(|e| format!("Failed to fetch puzzle: {e}"))?;

	if !resp.status().is_success() {
		return Err(format!("No puzzle found for date **{date}**. Make sure the date is valid (YYYY-MM-DD) and after 2023-06-12."));
	}

	let nyt: NytResponse = resp
		.json()
		.await
		.map_err(|e| format!("Failed to parse puzzle data: {e}"))?;

	if nyt.status != "OK" {
		return Err("NYT API returned an error.".into());
	}

	let categories: Vec<PuzzleCategory> = nyt
		.categories
		.into_iter()
		.enumerate()
		.map(|(i, cat)| PuzzleCategory {
			title: cat.title,
			words: cat.cards.into_iter().map(|c| c.content).collect(),
			difficulty: i as u8,
		})
		.collect();

	Ok(ConnectionsPuzzle {
		id: nyt.id,
		date: nyt.print_date,
		categories,
	})
}

/// Returns a random date between 2023-06-12 and today.
pub fn random_puzzle_date() -> String {
	use rand::RngExt;
	let start = chrono::NaiveDate::from_ymd_opt(2023, 6, 12).unwrap();
	let today = chrono::Utc::now().date_naive();
	let days_range = (today - start).num_days();
	if days_range <= 0 {
		return today.format("%Y-%m-%d").to_string();
	}
	let random_offset = rand::rng().random_range(0..=days_range);
	let date = start + chrono::Duration::days(random_offset);
	date.format("%Y-%m-%d").to_string()
}

pub fn today_puzzle_date() -> String {
	chrono::Utc::now().format("%Y-%m-%d").to_string()
}
