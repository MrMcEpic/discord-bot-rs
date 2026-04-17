//! AI provider abstraction.
//!
//! Each LLM endpoint we talk to (DeepSeek chat, DeepSeek Reasoner, Gemini,
//! Grok) is wrapped in an [`AiProvider`] impl that exposes its metadata +
//! capabilities. The free function [`complete`] does the HTTP work for any
//! `&dyn AiProvider` whose API matches OpenAI's `/chat/completions` shape
//! (which is all of them today). A future non-OpenAI-compatible provider
//! (e.g. native Anthropic) gets its own `complete_anthropic` function
//! alongside [`complete`] — the trait stays metadata-only.
//!
//! Routing decisions live in [`ProviderRouter`]: pick by capability flag
//! (vision-capable / reasoner / default chat) rather than by model-name string
//! comparisons sprinkled across the orchestration layer. The
//! [`complete_with_cascade`] helper layers on top of [`complete`] to retry
//! through alt providers when the primary returns the `CENSORED` sentinel.

pub mod deepseek;
pub mod gemini;
pub mod grok;

use std::time::Duration;

use serde_json::Value;

use crate::ai::dsml::parse_dsml;
use crate::ai::tools::tool_definitions;
use crate::config::Config;

pub use deepseek::{DeepSeekChat, DeepSeekReasoner};
pub use gemini::Gemini;
pub use grok::Grok;

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
	pub grok: Option<Grok>,
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
			grok: config.grok_api_key.as_ref().map(|k| Grok::new(k.clone())),
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

	/// Look up a provider by its short name as it appears in instance config
	/// (e.g. `[ai.fallback] on_censored = ["grok", "gemini"]`).
	///
	/// Returns `None` if the provider isn't configured (missing API key) or
	/// the name isn't recognised. Callers (the cascade resolver below) skip
	/// `None` entries with a warning at startup.
	pub fn named(&self, name: &str) -> Option<&dyn AiProvider> {
		match name {
			"grok" => self.grok.as_ref().map(|p| p as &dyn AiProvider),
			"gemini" => self.gemini.as_ref().map(|p| p as &dyn AiProvider),
			"deepseek" | "deepseek-chat" => self.chat(),
			_ => None,
		}
	}

	/// Resolve an ordered list of provider names into an ordered list of
	/// configured providers, skipping any that aren't set up.
	///
	/// Used by the CENSORED-cascade dispatcher: the instance config's
	/// `[ai.fallback] on_censored` field lists provider names by string;
	/// this resolves them once at startup so the request-path doesn't
	/// repeat the lookup.
	pub fn cascade_for(&self, names: &[String]) -> Vec<&dyn AiProvider> {
		let mut out = Vec::with_capacity(names.len());
		for n in names {
			match self.named(n) {
				Some(p) => out.push(p),
				None => tracing::warn!(
					"ai.fallback.on_censored: provider '{n}' is not configured (missing API key or unknown name); skipping"
				),
			}
		}
		out
	}
}

/// Try `primary.complete(...)`. If it returns the `CENSORED` sentinel, replay
/// the same `messages` through each provider in `cascade` in order. Returns
/// the first non-CENSORED success along with the provider name that produced
/// it (useful for log lines and optional debug footers).
///
/// Non-CENSORED errors from the primary short-circuit (per the issue brief:
/// "Don't cascade on non-content errors — they're transient and a different
/// provider may have the same problem"). Errors from cascade members are
/// treated as best-effort — we try the next one rather than bail, since the
/// fallback path is meant to maximise the chance of *some* answer.
///
/// Returns `Err("CENSORED")` if every provider in `[primary, ...cascade]` was
/// CENSORED — callers fall back to the existing snarky-message behaviour in
/// that case.
pub async fn complete_with_cascade(
	primary: &dyn AiProvider,
	cascade: &[&dyn AiProvider],
	client: &reqwest::Client,
	messages: &[Value],
	use_tools: bool,
	max_tokens: u32,
) -> Result<(ApiResponse, &'static str), String> {
	match complete(primary, client, messages, use_tools, max_tokens).await {
		Ok(r) => return Ok((r, primary.name())),
		Err(e) if e != "CENSORED" => return Err(e),
		Err(_) => {
			if cascade.is_empty() {
				return Err("CENSORED".to_string());
			}
			tracing::info!(
				"Primary provider {} returned CENSORED; cascading through {} alt(s)",
				primary.name(),
				cascade.len()
			);
		}
	}

	for alt in cascade {
		match complete(*alt, client, messages, use_tools, max_tokens).await {
			Ok(r) => {
				tracing::info!("Cascade succeeded via {}", alt.name());
				return Ok((r, alt.name()));
			}
			Err(e) if e == "CENSORED" => {
				tracing::info!("Cascade member {} also CENSORED; trying next", alt.name());
				continue;
			}
			Err(e) => {
				tracing::warn!("Cascade member {} errored ({e}); trying next", alt.name());
				continue;
			}
		}
	}

	Err("CENSORED".to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::Config;

	fn config(deepseek: Option<&str>, gemini: Option<&str>) -> Config {
		config_full(deepseek, gemini, None)
	}

	fn config_full(deepseek: Option<&str>, gemini: Option<&str>, grok: Option<&str>) -> Config {
		Config {
			token: "t".to_string(),
			client_id: "c".to_string(),
			guild_id: "g".to_string(),
			deepseek_api_key: deepseek.map(String::from),
			gemini_api_key: gemini.map(String::from),
			grok_api_key: grok.map(String::from),
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
		assert_eq!(Grok::new("k".to_string()).max_tokens_limit(), 16384);
	}

	#[test]
	fn router_named_resolves_configured_provider_strings() {
		let r = ProviderRouter::from_config(&config_full(Some("d"), Some("g"), Some("x")));
		assert!(r.named("grok").is_some());
		assert!(r.named("gemini").is_some());
		assert!(r.named("deepseek").is_some());
		assert!(r.named("deepseek-chat").is_some());
		// Unknown name → None, not a panic.
		assert!(r.named("anthropic").is_none());
		assert!(r.named("").is_none());
	}

	#[test]
	fn router_named_returns_none_when_provider_unconfigured() {
		// "grok" is a recognised name but the router has no Grok key — must
		// return None so cascade_for can skip it cleanly.
		let r = ProviderRouter::from_config(&config_full(Some("d"), None, None));
		assert!(r.named("grok").is_none());
		assert!(r.named("gemini").is_none());
	}

	#[test]
	fn cascade_for_preserves_order_and_skips_unconfigured() {
		// Only Grok configured; Gemini listed in cascade should be silently
		// dropped, Grok kept. Order from input list must be preserved.
		let r = ProviderRouter::from_config(&config_full(Some("d"), None, Some("x")));
		let names = vec!["gemini".to_string(), "grok".to_string()];
		let resolved = r.cascade_for(&names);
		assert_eq!(resolved.len(), 1, "gemini drops out, grok stays");
		assert_eq!(resolved[0].name(), "grok");
	}

	#[test]
	fn cascade_for_empty_names_returns_empty_vec() {
		let r = ProviderRouter::from_config(&config_full(Some("d"), Some("g"), Some("x")));
		assert!(r.cascade_for(&[]).is_empty());
	}

	#[test]
	fn cascade_for_unknown_names_returns_empty_vec() {
		let r = ProviderRouter::from_config(&config_full(Some("d"), Some("g"), Some("x")));
		let names = vec!["claude".to_string(), "llama".to_string()];
		// All names unknown — cascade is empty (caller falls back to canned reply).
		assert!(r.cascade_for(&names).is_empty());
	}

	#[test]
	fn cascade_for_keeps_duplicates_in_input_order() {
		// Caller bug? Maybe. But we don't dedupe — pinning current behaviour
		// so a future change is intentional, not accidental.
		let r = ProviderRouter::from_config(&config_full(Some("d"), Some("g"), Some("x")));
		let names = vec!["grok".to_string(), "gemini".to_string(), "grok".to_string()];
		let resolved = r.cascade_for(&names);
		assert_eq!(resolved.len(), 3);
		assert_eq!(resolved[0].name(), "grok");
		assert_eq!(resolved[1].name(), "gemini");
		assert_eq!(resolved[2].name(), "grok");
	}
}
