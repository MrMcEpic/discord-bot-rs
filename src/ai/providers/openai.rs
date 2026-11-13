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

	if use_tools && provider.supports_tools() {
		body["tools"] = Value::Array(tool_definitions());
	}
	merge_extra_body(&mut body, provider.extra_body());

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
		if is_censored_error(provider, &err_body) {
			return Err("CENSORED".to_string());
		}
		return Err(format!("API returned {status}"));
	}

	let data: Value = response
		.json()
		.await
		.map_err(|e| format!("Failed to parse API response: {e}"))?;

	let finish_reason = data["choices"][0]["finish_reason"].as_str().unwrap_or("");
	if finish_reason == "content_filter" {
		tracing::info!("{} finished with content_filter", provider.name());
		return Err("CENSORED".to_string());
	}
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

	// Thinking models count reasoning tokens against max_tokens. If the
	// budget ran out before any visible output, say so instead of returning
	// an empty response that callers would read as "no answer".
	if content.is_none() && tool_calls.is_empty() && finish_reason == "length" {
		return Err(format!(
			"{} hit max_tokens before producing output (finish_reason=length)",
			provider.name()
		));
	}

	Ok(ApiResponse {
		content,
		tool_calls,
	})
}

/// Merge a provider's `extra_body` into a request body. Top-level keys
/// overwrite; nested objects are replaced wholesale (not deep-merged).
pub(super) fn merge_extra_body(body: &mut Value, extra: &serde_json::Map<String, Value>) {
	if let Some(obj) = body.as_object_mut() {
		for (k, v) in extra {
			obj.insert(k.clone(), v.clone());
		}
	}
}

/// Does a non-2xx error body mean "blocked by the content filter"? True for
/// DeepSeek's long-standing `Content Exists Risk` sentinel (kept global so
/// user-defined DeepSeek providers without `refusal_markers` still match)
/// and for any of the provider's configured `refusal_markers`.
pub(super) fn is_censored_error(provider: &dyn AiProvider, err_body: &str) -> bool {
	crate::ai::chat::is_censored_body(err_body)
		|| provider
			.refusal_markers()
			.iter()
			.any(|m| !m.is_empty() && err_body.contains(m.as_str()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ai::providers::{ConfiguredProvider, ProviderDef};

	fn provider(markers: &[&str]) -> ConfiguredProvider {
		let def: ProviderDef = toml::from_str(&format!(
			"url = \"u\"
model = \"m\"
api_key_env = \"K\"
max_tokens = 1
refusal_markers = {markers:?}
"
		))
		.unwrap();
		ConfiguredProvider::from_def("p", def, |_| Some("k".to_string())).unwrap()
	}

	#[test]
	fn extra_body_merges_and_overrides_top_level_keys() {
		let mut body = serde_json::json!({ "model": "a", "max_tokens": 10 });
		let mut extra = serde_json::Map::new();
		extra.insert("reasoning_effort".into(), "max".into());
		extra.insert("max_tokens".into(), 99.into());
		merge_extra_body(&mut body, &extra);
		assert_eq!(body["reasoning_effort"], "max");
		assert_eq!(body["max_tokens"], 99);
		assert_eq!(body["model"], "a");
	}

	#[test]
	fn censored_error_matches_legacy_sentinel_and_configured_markers() {
		let none = provider(&[]);
		assert!(is_censored_error(
			&none,
			r#"{"error":"Content Exists Risk"}"#
		));
		assert!(!is_censored_error(
			&none,
			r#"{"error":"blocked by policy"}"#
		));
		let custom = provider(&["blocked by policy"]);
		assert!(is_censored_error(
			&custom,
			r#"{"error":"blocked by policy"}"#
		));
		assert!(!is_censored_error(&custom, r#"{"error":"rate limited"}"#));
		// An empty marker must not match every body.
		let empty = provider(&[""]);
		assert!(!is_censored_error(&empty, "anything"));
	}
}
