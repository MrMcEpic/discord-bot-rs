//! AI provider abstraction.
//!
//! Each LLM endpoint we talk to (DeepSeek chat, DeepSeek Reasoner, Gemini) is
//! wrapped in an [`AiProvider`] impl that exposes its metadata + capabilities.
//! The shared [`openai_compat_complete`] helper does the actual HTTP work for
//! providers whose API matches OpenAI's `/chat/completions` shape (which is
//! all of them today). A future Anthropic provider would override the default
//! `complete` impl since Anthropic isn't OpenAI-compatible.
//!
//! Routing decisions live in [`ProviderRouter`]: pick by capability flag
//! (vision-capable / reasoner / default chat) rather than by model-name string
//! comparisons sprinkled across the orchestration layer.

pub mod deepseek;
pub mod gemini;

use std::time::Duration;

use serde_json::Value;

use crate::ai::dsml::parse_dsml;
use crate::ai::tools::tool_definitions;
use crate::config::Config;

pub use deepseek::{DeepSeekChat, DeepSeekReasoner};
pub use gemini::Gemini;

/// A single message returned by the model — text content plus any tool calls.
#[derive(Debug, Default)]
pub struct ApiResponse {
	pub content: Option<String>,
	pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
	pub id: String,
	pub name: String,
	pub arguments: String,
}

/// Capability + metadata surface for an LLM endpoint.
///
/// Implementations are intentionally thin: just the provider URL, model name,
/// API key, and capability flags. All HTTP work happens in the free function
/// [`complete`], which takes any `&dyn AiProvider`. Keeping the trait
/// metadata-only avoids async-trait + object-safety friction and lets us add
/// non-OpenAI-compatible providers (e.g. native Anthropic) by writing a new
/// `complete_*` function alongside this one.
pub trait AiProvider: Send + Sync + std::fmt::Debug {
	/// Short human label for log lines (e.g. "deepseek-chat", "gemini").
	fn name(&self) -> &'static str;

	/// HTTPS endpoint for the chat-completions request.
	fn url(&self) -> &'static str;

	/// Model identifier passed in the request body.
	fn model(&self) -> &'static str;

	/// Bearer token for the `Authorization` header.
	fn api_key(&self) -> &str;

	/// Whether the model accepts image content parts in messages.
	fn supports_vision(&self) -> bool;

	/// Whether the model accepts a `tools` array. DeepSeek Reasoner doesn't.
	fn supports_tools(&self) -> bool {
		true
	}

	/// Whether this is a slow reasoning model that needs a longer timeout
	/// budget and skips tools regardless of `supports_tools`.
	fn is_reasoner(&self) -> bool {
		false
	}

	/// Per-provider hard cap on `max_tokens`. Callers ask for whatever they
	/// want; we clamp here.
	fn max_tokens_limit(&self) -> u32;

	/// Per-provider HTTP request timeout.
	fn timeout(&self) -> Duration {
		Duration::from_secs(30)
	}
}

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

	let response = client
		.post(provider.url())
		.header("Content-Type", "application/json")
		.header("Authorization", format!("Bearer {}", provider.api_key()))
		.timeout(provider.timeout())
		.json(&body)
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
		content = if cleaned.is_empty() {
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

/// Holds the configured providers + picks one based on the request shape.
///
/// Built once at startup from [`Config`]. Each `Option<...>` is `Some` iff the
/// matching API key is set in the environment. Picker methods return
/// `Option<&dyn AiProvider>` so callers can decide what to do when no provider
/// is available (text path: bail, vision path: fall through to text).
#[derive(Debug, Default)]
pub struct ProviderRouter {
	pub deepseek_chat: Option<DeepSeekChat>,
	pub deepseek_reasoner: Option<DeepSeekReasoner>,
	pub gemini: Option<Gemini>,
}

impl ProviderRouter {
	pub fn from_config(config: &Config) -> Self {
		Self {
			deepseek_chat: config
				.deepseek_api_key
				.as_ref()
				.map(|k| DeepSeekChat::new(k.clone())),
			deepseek_reasoner: config
				.deepseek_api_key
				.as_ref()
				.map(|k| DeepSeekReasoner::new(k.clone())),
			gemini: config
				.gemini_api_key
				.as_ref()
				.map(|k| Gemini::new(k.clone())),
		}
	}

	/// Pick a vision-capable provider. Today: Gemini.
	pub fn vision(&self) -> Option<&dyn AiProvider> {
		self.gemini.as_ref().map(|g| g as &dyn AiProvider)
	}

	/// Pick the default text-chat provider. Today: DeepSeek V3 chat.
	pub fn chat(&self) -> Option<&dyn AiProvider> {
		self.deepseek_chat.as_ref().map(|p| p as &dyn AiProvider)
	}

	/// Pick the reasoning-class provider. Today: DeepSeek Reasoner.
	pub fn reasoner(&self) -> Option<&dyn AiProvider> {
		self.deepseek_reasoner
			.as_ref()
			.map(|p| p as &dyn AiProvider)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::Config;

	fn config(deepseek: Option<&str>, gemini: Option<&str>) -> Config {
		Config {
			token: "t".to_string(),
			client_id: "c".to_string(),
			guild_id: "g".to_string(),
			deepseek_api_key: deepseek.map(String::from),
			gemini_api_key: gemini.map(String::from),
			finnhub_api_key: None,
			mc_verify_url: None,
			mc_verify_secret: None,
			db_schema: "public".to_string(),
			mcp_port: 9090,
			mcp_bind_addr: "127.0.0.1".to_string(),
			mcp_auth_token: String::new(),
			database_url: "postgres://x".to_string(),
		}
	}

	#[test]
	fn router_with_no_keys_picks_nothing() {
		let r = ProviderRouter::from_config(&config(None, None));
		assert!(r.chat().is_none());
		assert!(r.reasoner().is_none());
		assert!(r.vision().is_none());
	}

	#[test]
	fn router_with_deepseek_only_has_no_vision() {
		let r = ProviderRouter::from_config(&config(Some("k"), None));
		assert!(r.chat().is_some());
		assert!(r.reasoner().is_some());
		assert!(r.vision().is_none(), "vision needs Gemini key");
	}

	#[test]
	fn router_with_gemini_only_has_vision_but_no_text() {
		let r = ProviderRouter::from_config(&config(None, Some("k")));
		assert!(r.chat().is_none());
		assert!(r.reasoner().is_none());
		assert!(r.vision().is_some());
	}

	#[test]
	fn router_with_both_keys_has_everything() {
		let r = ProviderRouter::from_config(&config(Some("d"), Some("g")));
		assert!(r.chat().is_some());
		assert!(r.reasoner().is_some());
		assert!(r.vision().is_some());
	}

	#[test]
	fn capability_flags_match_expected_shape() {
		// Capability matrix is the single source of truth for routing — pin it
		// down so a copy-paste mistake in a provider impl can't silently
		// re-route requests (e.g. Gemini accidentally claiming reasoner).
		let chat = DeepSeekChat::new("k".to_string());
		assert!(!chat.supports_vision());
		assert!(chat.supports_tools());
		assert!(!chat.is_reasoner());

		let reasoner = DeepSeekReasoner::new("k".to_string());
		assert!(!reasoner.supports_vision());
		assert!(!reasoner.supports_tools());
		assert!(reasoner.is_reasoner());

		let gemini = Gemini::new("k".to_string());
		assert!(gemini.supports_vision());
		assert!(gemini.supports_tools());
		assert!(!gemini.is_reasoner());
	}

	#[test]
	fn reasoner_has_longer_timeout_than_chat() {
		// The 300s reasoner budget vs 30s chat budget is load-bearing — a
		// short timeout drops reasoner mid-thought.
		let chat = DeepSeekChat::new("k".to_string());
		let reasoner = DeepSeekReasoner::new("k".to_string());
		assert!(
			reasoner.timeout() > chat.timeout(),
			"reasoner needs more time"
		);
	}

	#[test]
	fn max_tokens_limit_per_provider_matches_documented_caps() {
		// Provider hard caps from public docs at time of writing.
		assert_eq!(DeepSeekChat::new("k".to_string()).max_tokens_limit(), 8192);
		assert_eq!(
			DeepSeekReasoner::new("k".to_string()).max_tokens_limit(),
			32768
		);
		assert_eq!(Gemini::new("k".to_string()).max_tokens_limit(), 16384);
	}
}
