use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct VerifyRequest {
	pub code: String,
	pub discord_id: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyResponse {
	pub success: bool,
	#[serde(default)]
	pub username: Option<String>,
	#[serde(default)]
	pub uuid: Option<String>,
	#[serde(default)]
	pub error: Option<String>,
}

pub async fn verify(
	http_client: &reqwest::Client,
	base_url: &str,
	secret: &str,
	code: &str,
	discord_id: &str,
) -> Result<VerifyResponse, String> {
	let url = format!("{}/api/verify", base_url.trim_end_matches('/'));

	let request = VerifyRequest {
		code: code.to_string(),
		discord_id: discord_id.to_string(),
	};

	let resp = http_client
		.post(&url)
		.header("Authorization", format!("Bearer {secret}"))
		.json(&request)
		.send()
		.await
		.map_err(|e| format!("Failed to reach MC server: {e}"))?;

	let body = resp
		.json::<VerifyResponse>()
		.await
		.map_err(|e| format!("Invalid response from MC server: {e}"))?;

	Ok(body)
}
