use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct NytWordle {
	solution: String,
	print_date: String,
}

#[derive(Debug, Clone)]
pub struct WordlePuzzle {
	pub solution: String,
	pub date: String,
}

pub async fn fetch_puzzle(
	http_client: &reqwest::Client,
	date: &str,
) -> Result<WordlePuzzle, String> {
	let url = format!("https://www.nytimes.com/svc/wordle/v2/{date}.json");

	let resp = http_client
		.get(&url)
		.send()
		.await
		.map_err(|e| format!("Failed to fetch Wordle: {e}"))?;

	if !resp.status().is_success() {
		return Err(format!(
			"No Wordle found for date **{date}**. Use YYYY-MM-DD format, after 2021-06-19."
		));
	}

	let nyt: NytWordle = resp
		.json()
		.await
		.map_err(|e| format!("Failed to parse Wordle data: {e}"))?;

	Ok(WordlePuzzle {
		solution: nyt.solution.to_lowercase(),
		date: nyt.print_date,
	})
}

pub fn random_puzzle_date() -> String {
	use rand::RngExt;
	let start = chrono::NaiveDate::from_ymd_opt(2021, 6, 19).unwrap();
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
