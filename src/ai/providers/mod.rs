//! AI provider abstraction.
//!
//! [`ConfiguredProvider`] is the single concrete `AiProvider` impl. The four
//! providers the bot ships with (DeepSeek chat, DeepSeek Reasoner, Gemini,
//! Grok) come from [`default_provider_registry()`] and use the same
//! per-provider URLs / model names / capability flags as releases prior to
//! 0.15.0. The free function [`complete`] does the OpenAI-compatible HTTP
//! work for any `&dyn AiProvider`; non-OpenAI-compatible providers (e.g.
//! a future native Anthropic dispatcher) get their own `complete_*` function
//! alongside this one — the trait stays metadata-only.
//!
//! Routing decisions live in [`ProviderRouter`]: pick by capability flag
//! (vision-capable / reasoner / default chat) rather than by model-name
//! string comparisons. The [`complete_with_cascade`] helper layers on top
//! to retry through alt providers when the primary returns the `CENSORED`
//! sentinel.

pub mod configured;

use std::time::Duration;

use serde_json::Value;

use crate::ai::dsml::parse_dsml;
use crate::ai::tools::tool_definitions;

pub use configured::{ConfiguredProvider, ProviderDef, ProviderSpec};

/// The four providers the bot ships with. Used as the base registry when no
/// `[ai.providers]` section is in instance config; user-defined providers
/// merge on top (user wins on name collision).
///
/// Field values here are pinned by `default_registry_*_matches_v0_14_0`
/// snapshot tests in this module. Drift breaks the test.
pub fn default_provider_registry() -> Vec<(&'static str, ProviderDef)> {
	vec![
		(
			"deepseek_chat",
			ProviderDef {
				url: "https://api.deepseek.com/chat/completions".to_string(),
				model: "deepseek-chat".to_string(),
				api_key_env: "DEEPSEEK_API_KEY".to_string(),
				max_tokens: 8192,
				timeout_secs: 30,
				supports_vision: false,
				supports_tools: true,
				is_reasoner: false,
				spec: ProviderSpec::OpenAi,
			},
		),
		(
			"deepseek_reasoner",
			ProviderDef {
				url: "https://api.deepseek.com/chat/completions".to_string(),
				model: "deepseek-reasoner".to_string(),
				api_key_env: "DEEPSEEK_API_KEY".to_string(),
				max_tokens: 32768,
				timeout_secs: 300,
				supports_vision: false,
				supports_tools: false,
				is_reasoner: true,
				spec: ProviderSpec::OpenAi,
			},
		),
		(
			"gemini_flash",
			ProviderDef {
				url: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
					.to_string(),
				model: "gemini-3-flash-preview".to_string(),
				api_key_env: "GEMINI_API_KEY".to_string(),
				max_tokens: 16384,
				timeout_secs: 30,
				supports_vision: true,
				supports_tools: true,
				is_reasoner: false,
				spec: ProviderSpec::OpenAi,
			},
		),
		(
			"grok",
			ProviderDef {
				url: "https://api.x.ai/v1/chat/completions".to_string(),
				model: "grok-3".to_string(),
				api_key_env: "GROK_API_KEY".to_string(),
				max_tokens: 16384,
				timeout_secs: 30,
				supports_vision: false,
				supports_tools: true,
				is_reasoner: false,
				spec: ProviderSpec::OpenAi,
			},
		),
	]
}

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
	fn name(&self) -> &str;

	/// HTTPS endpoint for the chat-completions request.
	fn url(&self) -> &str;

	/// Model identifier passed in the request body.
	fn model(&self) -> &str;

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

/// Resolved AI provider stack for a single bot instance.
///
/// Built once at startup from the merged registry (defaults + user `[ai.providers]`)
/// and the resolved role map (defaults + user `[ai.routing]`). Holds only
/// AVAILABLE providers — definitions whose `api_key_env` resolved to an unset
/// or empty env var are NOT in the map. Callers look up by role name; missing
/// names return `None` so the existing chat.rs `Option<&dyn AiProvider>`
/// handling keeps working (silent disable when the relevant env var is unset).
#[derive(Debug, Default)]
pub struct ProviderRouter {
	/// All available providers, keyed by name.
	providers: std::collections::HashMap<String, ConfiguredProvider>,
	/// Role → provider-name lookups. `chat_role` is always set after
	/// construction (defaulted to `"deepseek_chat"` if the user didn't
	/// override). `vision_role` / `reasoner_role` are `None` when the user
	/// explicitly opted out by setting `[ai.routing]` without those keys.
	chat_role: String,
	vision_role: Option<String>,
	reasoner_role: Option<String>,
}

/// Default routing matrix when `[ai.routing]` is absent from instance config.
const DEFAULT_CHAT_ROLE: &str = "deepseek_chat";
const DEFAULT_VISION_ROLE: &str = "gemini_flash";
const DEFAULT_REASONER_ROLE: &str = "deepseek_reasoner";

impl ProviderRouter {
	/// Build a router from the default registry only — used by the no-config
	/// path and by tests that don't need to exercise user definitions.
	///
	/// `env_lookup` is injected so tests can pass a fake without touching
	/// the process env. Production callers pass `|k| std::env::var(k).ok()`.
	pub fn from_defaults(env_lookup: impl Fn(&str) -> Option<String>) -> Self {
		let mut providers = std::collections::HashMap::new();
		for (name, def) in default_provider_registry() {
			if let Some(p) = ConfiguredProvider::from_def(name, def, &env_lookup) {
				providers.insert(p.name().to_string(), p);
			}
		}
		Self {
			providers,
			chat_role: DEFAULT_CHAT_ROLE.to_string(),
			vision_role: Some(DEFAULT_VISION_ROLE.to_string()),
			reasoner_role: Some(DEFAULT_REASONER_ROLE.to_string()),
		}
	}

	/// Pick a vision-capable provider (None if no vision role configured OR
	/// configured provider is unavailable).
	pub fn vision(&self) -> Option<&dyn AiProvider> {
		self.providers
			.get(self.vision_role.as_deref()?)
			.map(|p| p as &dyn AiProvider)
	}

	/// Pick the default text-chat provider (None if unavailable).
	pub fn chat(&self) -> Option<&dyn AiProvider> {
		self.providers
			.get(&self.chat_role)
			.map(|p| p as &dyn AiProvider)
	}

	/// Pick the reasoning-class provider (None if no reasoner role configured
	/// OR configured provider is unavailable).
	pub fn reasoner(&self) -> Option<&dyn AiProvider> {
		self.providers
			.get(self.reasoner_role.as_deref()?)
			.map(|p| p as &dyn AiProvider)
	}

	/// Look up a provider by name. Returns `None` for unknown names AND
	/// unavailable providers (used by `cascade_for` and external direct access).
	///
	/// Accepts the canonical default-registry names (`deepseek_chat`,
	/// `deepseek_reasoner`, `gemini_flash`, `grok`) plus the short aliases
	/// supported by 0.14.0's `named()` — `"gemini"`, `"deepseek"`,
	/// `"deepseek-chat"` — for backward compat with instance configs written
	/// before 0.15.0. User-defined provider names always go through the
	/// canonical lookup path; aliases only apply to the default registry.
	pub fn named(&self, name: &str) -> Option<&dyn AiProvider> {
		// Direct lookup wins (covers canonical default names + all user-defined).
		if let Some(p) = self.providers.get(name) {
			return Some(p as &dyn AiProvider);
		}
		// Then 0.14.0 aliases for default-registry providers.
		let aliased = match name {
			"gemini" => "gemini_flash",
			"deepseek" | "deepseek-chat" => "deepseek_chat",
			_ => return None,
		};
		self.providers.get(aliased).map(|p| p as &dyn AiProvider)
	}

	/// Resolve an ordered list of provider names into an ordered Vec of
	/// `&dyn AiProvider`, skipping unknown / unavailable names with a
	/// `tracing::warn!`.
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
) -> Result<(ApiResponse, String), String> {
	match complete(primary, client, messages, use_tools, max_tokens).await {
		Ok(r) => return Ok((r, primary.name().to_string())),
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
				return Ok((r, alt.name().to_string()));
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

	// --- Helpers ---------------------------------------------------------

	/// Env-lookup factory: returns `Some(val)` for the given keys, `None` for
	/// anything else. Mirrors how production reads `std::env::var`.
	fn env_with<'a>(keys: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
		let owned: Vec<(String, String)> = keys
			.iter()
			.map(|(k, v)| (k.to_string(), v.to_string()))
			.collect();
		move |k: &str| {
			owned
				.iter()
				.find(|(key, _)| key == k)
				.map(|(_, v)| v.clone())
		}
	}

	/// Empty env — every provider definition will be unavailable.
	fn empty_env() -> impl Fn(&str) -> Option<String> {
		|_| None
	}

	// --- Default registry snapshot tests --------------------------------

	fn def_by_name<'a>(registry: &'a [(&'static str, ProviderDef)], name: &str) -> &'a ProviderDef {
		registry
			.iter()
			.find(|(n, _)| *n == name)
			.map(|(_, def)| def)
			.unwrap_or_else(|| panic!("default registry missing provider: {name}"))
	}

	#[test]
	fn default_registry_has_exactly_four_providers() {
		let r = default_provider_registry();
		let names: Vec<&str> = r.iter().map(|(n, _)| *n).collect();
		assert_eq!(names.len(), 4, "default registry size changed");
		assert!(names.contains(&"deepseek_chat"));
		assert!(names.contains(&"deepseek_reasoner"));
		assert!(names.contains(&"gemini_flash"));
		assert!(names.contains(&"grok"));
	}

	#[test]
	fn default_registry_deepseek_chat_matches_v0_14_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "deepseek_chat");
		assert_eq!(def.url, "https://api.deepseek.com/chat/completions");
		assert_eq!(def.model, "deepseek-chat");
		assert_eq!(def.api_key_env, "DEEPSEEK_API_KEY");
		assert_eq!(def.max_tokens, 8192);
		assert_eq!(def.timeout_secs, 30);
		assert!(!def.supports_vision);
		assert!(def.supports_tools);
		assert!(!def.is_reasoner);
		assert_eq!(def.spec, ProviderSpec::OpenAi);
	}

	#[test]
	fn default_registry_deepseek_reasoner_matches_v0_14_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "deepseek_reasoner");
		assert_eq!(def.url, "https://api.deepseek.com/chat/completions");
		assert_eq!(def.model, "deepseek-reasoner");
		assert_eq!(def.api_key_env, "DEEPSEEK_API_KEY");
		assert_eq!(def.max_tokens, 32768);
		assert_eq!(def.timeout_secs, 300);
		assert!(!def.supports_vision);
		assert!(!def.supports_tools, "reasoner does not accept tools");
		assert!(def.is_reasoner);
		assert_eq!(def.spec, ProviderSpec::OpenAi);
	}

	#[test]
	fn default_registry_gemini_flash_matches_v0_14_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "gemini_flash");
		assert_eq!(
			def.url,
			"https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
		);
		assert_eq!(def.model, "gemini-3-flash-preview");
		assert_eq!(def.api_key_env, "GEMINI_API_KEY");
		assert_eq!(def.max_tokens, 16384);
		assert_eq!(def.timeout_secs, 30);
		assert!(def.supports_vision, "gemini is the vision provider");
		assert!(def.supports_tools);
		assert!(!def.is_reasoner);
		assert_eq!(def.spec, ProviderSpec::OpenAi);
	}

	#[test]
	fn default_registry_grok_matches_v0_14_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "grok");
		assert_eq!(def.url, "https://api.x.ai/v1/chat/completions");
		assert_eq!(def.model, "grok-3");
		assert_eq!(def.api_key_env, "GROK_API_KEY");
		assert_eq!(def.max_tokens, 16384);
		assert_eq!(def.timeout_secs, 30);
		assert!(!def.supports_vision);
		assert!(def.supports_tools);
		assert!(!def.is_reasoner);
		assert_eq!(def.spec, ProviderSpec::OpenAi);
	}

	// --- ProviderRouter::from_defaults -----------------------------------

	#[test]
	fn router_with_no_keys_picks_nothing() {
		let r = ProviderRouter::from_defaults(empty_env());
		assert!(r.chat().is_none());
		assert!(r.reasoner().is_none());
		assert!(r.vision().is_none());
	}

	#[test]
	fn router_with_deepseek_only_has_no_vision() {
		let r = ProviderRouter::from_defaults(env_with(&[("DEEPSEEK_API_KEY", "k")]));
		assert!(r.chat().is_some());
		assert!(r.reasoner().is_some());
		assert!(r.vision().is_none(), "vision needs Gemini key");
	}

	#[test]
	fn router_with_gemini_only_has_vision_but_no_text() {
		let r = ProviderRouter::from_defaults(env_with(&[("GEMINI_API_KEY", "k")]));
		assert!(r.chat().is_none());
		assert!(r.reasoner().is_none());
		assert!(r.vision().is_some());
	}

	#[test]
	fn router_with_all_keys_has_everything() {
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "d"),
			("GEMINI_API_KEY", "g"),
			("GROK_API_KEY", "x"),
		]));
		assert!(r.chat().is_some());
		assert!(r.reasoner().is_some());
		assert!(r.vision().is_some());
		assert!(r.named("grok").is_some());
	}

	#[test]
	fn capability_flags_match_expected_shape() {
		// Capability matrix sanity — pin the default-registry providers'
		// trait-method outputs so a copy-paste mistake in the registry can't
		// silently re-route requests (e.g. Gemini accidentally claiming
		// reasoner).
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "d"),
			("GEMINI_API_KEY", "g"),
			("GROK_API_KEY", "x"),
		]));

		let chat = r.named("deepseek_chat").unwrap();
		assert_eq!(chat.name(), "deepseek_chat");
		assert!(!chat.supports_vision());
		assert!(chat.supports_tools());
		assert!(!chat.is_reasoner());

		let reasoner = r.named("deepseek_reasoner").unwrap();
		assert_eq!(reasoner.name(), "deepseek_reasoner");
		assert!(!reasoner.supports_vision());
		assert!(!reasoner.supports_tools());
		assert!(reasoner.is_reasoner());

		let gemini = r.named("gemini_flash").unwrap();
		assert_eq!(gemini.name(), "gemini_flash");
		assert!(gemini.supports_vision());
		assert!(gemini.supports_tools());
		assert!(!gemini.is_reasoner());

		let grok = r.named("grok").unwrap();
		assert_eq!(grok.name(), "grok");
		assert!(!grok.supports_vision());
		assert!(grok.supports_tools());
		assert!(!grok.is_reasoner());
	}

	#[test]
	fn reasoner_has_longer_timeout_than_chat() {
		let r = ProviderRouter::from_defaults(env_with(&[("DEEPSEEK_API_KEY", "k")]));
		let chat = r.named("deepseek_chat").unwrap();
		let reasoner = r.named("deepseek_reasoner").unwrap();
		assert!(
			reasoner.timeout() > chat.timeout(),
			"reasoner needs more time"
		);
	}

	#[test]
	fn max_tokens_limit_per_provider_matches_documented_caps() {
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "k"),
			("GEMINI_API_KEY", "k"),
			("GROK_API_KEY", "k"),
		]));
		assert_eq!(r.named("deepseek_chat").unwrap().max_tokens_limit(), 8192);
		assert_eq!(
			r.named("deepseek_reasoner").unwrap().max_tokens_limit(),
			32768
		);
		assert_eq!(r.named("gemini_flash").unwrap().max_tokens_limit(), 16384);
		assert_eq!(r.named("grok").unwrap().max_tokens_limit(), 16384);
	}

	// --- named() / cascade_for resolution -------------------------------

	#[test]
	fn router_named_resolves_configured_provider_strings() {
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "d"),
			("GEMINI_API_KEY", "g"),
			("GROK_API_KEY", "x"),
		]));
		assert!(r.named("grok").is_some());
		assert!(r.named("gemini_flash").is_some());
		assert!(r.named("deepseek_chat").is_some());
		assert!(r.named("deepseek_reasoner").is_some());
		// Unknown name → None, not a panic.
		assert!(r.named("anthropic").is_none());
		assert!(r.named("").is_none());
	}

	#[test]
	fn router_named_returns_none_when_provider_unconfigured() {
		// "grok" is a recognised registry name but env var unset — must
		// return None so cascade_for can skip it cleanly.
		let r = ProviderRouter::from_defaults(env_with(&[("DEEPSEEK_API_KEY", "d")]));
		assert!(r.named("grok").is_none());
		assert!(r.named("gemini_flash").is_none());
	}

	#[test]
	fn cascade_for_preserves_order_and_skips_unconfigured() {
		// Only Grok configured; Gemini listed in cascade should be silently
		// dropped, Grok kept. Order from input list must be preserved.
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "d"),
			("GROK_API_KEY", "x"),
		]));
		let names = vec!["gemini_flash".to_string(), "grok".to_string()];
		let resolved = r.cascade_for(&names);
		assert_eq!(resolved.len(), 1, "gemini drops out, grok stays");
		assert_eq!(resolved[0].name(), "grok");
	}

	#[test]
	fn cascade_for_empty_names_returns_empty_vec() {
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "d"),
			("GEMINI_API_KEY", "g"),
			("GROK_API_KEY", "x"),
		]));
		assert!(r.cascade_for(&[]).is_empty());
	}

	#[test]
	fn cascade_for_unknown_names_returns_empty_vec() {
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "d"),
			("GEMINI_API_KEY", "g"),
			("GROK_API_KEY", "x"),
		]));
		let names = vec!["claude".to_string(), "llama".to_string()];
		assert!(r.cascade_for(&names).is_empty());
	}

	#[test]
	fn router_named_resolves_v0_14_0_short_aliases_for_backward_compat() {
		// 0.14.0's named() accepted these short forms; instance configs in the
		// wild may use them in [ai.fallback] on_censored. Don't silently break
		// them.
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "k"),
			("GEMINI_API_KEY", "k"),
			("GROK_API_KEY", "k"),
		]));
		assert_eq!(r.named("gemini").map(|p| p.name()), Some("gemini_flash"));
		assert_eq!(r.named("deepseek").map(|p| p.name()), Some("deepseek_chat"));
		assert_eq!(
			r.named("deepseek-chat").map(|p| p.name()),
			Some("deepseek_chat")
		);
		// Sanity: unknown names still return None.
		assert!(r.named("not_a_provider").is_none());
	}

	#[test]
	#[ignore]
	// TODO: enable after Commit 2 lands from_instance_config on ProviderRouter.
	fn router_named_aliases_dont_apply_to_user_definitions() {
		// Aliases ONLY map "gemini" → "gemini_flash" etc. for the default
		// registry. A user with a provider literally named "deepseek" is found
		// directly; the alias path doesn't fire and isn't needed.
		//
		// Requires ProviderRouter::from_instance_config which is added in Commit 2.
	}

	#[test]
	fn cascade_for_keeps_duplicates_in_input_order() {
		// Caller bug? Maybe. But we don't dedupe — pinning current behaviour
		// so a future change is intentional, not accidental.
		let r = ProviderRouter::from_defaults(env_with(&[
			("DEEPSEEK_API_KEY", "d"),
			("GEMINI_API_KEY", "g"),
			("GROK_API_KEY", "x"),
		]));
		let names = vec![
			"grok".to_string(),
			"gemini_flash".to_string(),
			"grok".to_string(),
		];
		let resolved = r.cascade_for(&names);
		assert_eq!(resolved.len(), 3);
		assert_eq!(resolved[0].name(), "grok");
		assert_eq!(resolved[1].name(), "gemini_flash");
		assert_eq!(resolved[2].name(), "grok");
	}
}
