//! OpenAI-compatible `/chat/completions` dispatcher.

use serde_json::Value;

use super::{AiProvider, ApiResponse, ToolCall};
use crate::ai::dsml::parse_dsml;
use crate::ai::tools::tool_definitions;

/// Send an OpenAI-compatible chat-completions request and return the parsed
/// response.
///
/// Reads provider metadata + capabilities from `provider`, builds the request,
/// posts it, and parses the response into [`ApiResponse`]. Returns the
/// dedicated `Err("CENSORED")` sentinel when the body matches DeepSeek's
/// content-moderation refusal shape (see `is_censored_body` in the chat
/// module). Non-OpenAI-compatible providers (e.g. future native Anthropic)
/// would get their own dedicated free function alongside this one.
pub async fn complete(
	provider: &dyn AiProvider,
	client: &reqwest::Client,
	messages: &[Value],
	use_tools: bool,
	max_tokens: u32,
) -> Result<ApiResponse, String> {
	let clamped_tokens = max_tokens.min(provider.max_tokens_limit());

	let mut body = serde_json::json!({
		"model": provider.model(),
		"messages": messages,
		"max_tokens": clamped_tokens,
	});

	// Reasoner models don't accept tools regardless of caller intent.
	if use_tools && provider.supports_tools() && !provider.is_reasoner() {
		body["tools"] = Value::Array(tool_definitions());
	}

	let mut req = client
		.post(provider.url())
		.header("Content-Type", "application/json")
		.header(
			provider.auth_header(),
			format!("{}{}", provider.auth_scheme(), provider.api_key()),
		)
		.timeout(provider.timeout())
		.json(&body);

	for (k, v) in provider.extra_headers() {
		req = req.header(k.as_str(), v.as_str());
	}

	let response = req
		.send()
		.await
		.map_err(|e| format!("API request failed: {e}"))?;

	if !response.status().is_success() {
		let status = response.status();
		let err_body = response.text().await.unwrap_or_default();
		tracing::error!("{} API {status}: {err_body}", provider.model());
		if crate::ai::chat::is_censored_body(&err_body) {
			return Err("CENSORED".to_string());
		}
		return Err(format!("API returned {status}"));
	}

	let data: Value = response
		.json()
		.await
		.map_err(|e| format!("Failed to parse API response: {e}"))?;

	let choice = &data["choices"][0]["message"];

	// OpenAI-shape native tool calls.
	let mut tool_calls: Vec<ToolCall> = choice["tool_calls"]
		.as_array()
		.unwrap_or(&vec![])
		.iter()
		.filter_map(|tc| {
			Some(ToolCall {
				id: tc["id"].as_str()?.to_string(),
				name: tc["function"]["name"].as_str()?.to_string(),
				arguments: tc["function"]["arguments"].as_str()?.to_string(),
			})
		})
		.collect();

	// DSML-embedded tool calls in the content string.
	let mut content = choice["content"].as_str().map(|s| s.trim().to_string());
	if let Some(ref text) = content {
		let (dsml_calls, cleaned) = parse_dsml(text);
		for dsml in dsml_calls {
			let args_json = serde_json::to_string(&dsml.arguments).unwrap_or_default();
			tool_calls.push(ToolCall {
				id: format!(
					"dsml_{}_{}",
					chrono::Utc::now().timestamp_millis(),
					rand::random::<u32>()
				),
				name: dsml.name,
				arguments: args_json,
			});
		}
		content = if cleaned.trim().is_empty() {
			None
		} else {
			Some(cleaned)
		};
	}

	Ok(ApiResponse {
		content,
		tool_calls,
	})
}
