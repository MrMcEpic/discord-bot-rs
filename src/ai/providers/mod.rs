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
				headers: std::collections::HashMap::new(),
				auth_header: "Authorization".to_string(),
				auth_scheme: "Bearer ".to_string(),
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
				headers: std::collections::HashMap::new(),
				auth_header: "Authorization".to_string(),
				auth_scheme: "Bearer ".to_string(),
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
				headers: std::collections::HashMap::new(),
				auth_header: "Authorization".to_string(),
				auth_scheme: "Bearer ".to_string(),
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
				headers: std::collections::HashMap::new(),
				auth_header: "Authorization".to_string(),
				auth_scheme: "Bearer ".to_string(),
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

	/// Which API spec this provider speaks. Determines the dispatcher path in
	/// `chat.rs`: `ProviderSpec::OpenAi` → `complete()`,
	/// `ProviderSpec::Anthropic` → `complete_anthropic()`. Default is
	/// `OpenAi` for trait objects that don't override — every provider
	/// shipped today is OpenAI-compatible.
	fn spec(&self) -> crate::ai::providers::ProviderSpec {
		crate::ai::providers::ProviderSpec::OpenAi
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

	/// Build the router from instance config + env. Merges the baked default
	/// registry with user `[ai.providers]` (user wins on name collision),
	/// resolves each definition into a [`ConfiguredProvider`] via env lookup,
	/// then resolves routing roles per Layer 2 rules from the spec
	/// (section absent → default matrix; section present → only user-set
	/// keys take effect, omitted vision/reasoner mean "graceful degrade").
	///
	/// Validation (panic on typos, warn on unavailable, capability sanity)
	/// is added separately in Commit 3; this constructor just builds the
	/// state.
	#[allow(dead_code)] // test-only; called from mod tests — not reachable from bin
	pub(crate) fn from_instance_config(
		ai_cfg: &crate::instance_config::AiConfig,
		env_lookup: impl Fn(&str) -> Option<String>,
	) -> Self {
		// Build the merged registry: defaults first, then user definitions
		// override on name collision.
		let mut merged: std::collections::HashMap<String, ProviderDef> =
			std::collections::HashMap::new();
		for (name, def) in default_provider_registry() {
			merged.insert(name.to_string(), def);
		}
		for (name, def) in &ai_cfg.providers {
			merged.insert(name.clone(), def.clone());
		}

		// Resolve each definition; only the available ones make it into the
		// providers HashMap.
		let mut providers = std::collections::HashMap::new();
		for (name, def) in merged {
			if let Some(p) = ConfiguredProvider::from_def(name.clone(), def, &env_lookup) {
				providers.insert(name, p);
			}
		}

		// Resolve routing roles per the spec's Layer 1 / Layer 2 rules.
		let (chat_role, vision_role, reasoner_role) = match &ai_cfg.routing {
			Some(r) => {
				// Layer 2: section present, only user-set keys take effect.
				// `chat` is required at this layer (validated in Commit 3).
				// For now, fall back to the default if missing so the build
				// is well-defined.
				let chat = r
					.chat
					.clone()
					.unwrap_or_else(|| DEFAULT_CHAT_ROLE.to_string());
				(chat, r.vision.clone(), r.reasoner.clone())
			}
			None => (
				// Layer 1: section absent, full default matrix.
				DEFAULT_CHAT_ROLE.to_string(),
				Some(DEFAULT_VISION_ROLE.to_string()),
				Some(DEFAULT_REASONER_ROLE.to_string()),
			),
		};

		Self {
			providers,
			chat_role,
			vision_role,
			reasoner_role,
		}
	}

	/// Same as [`Self::from_instance_config`] but with startup validation:
	/// panics on unknown provider names referenced by `[ai.routing]`, on
	/// `[ai.routing]` without `chat`, on whitespace-bearing provider names,
	/// and on phase-2 spec values. Warns (non-fatal) on unavailable
	/// providers referenced by routing or fallback, and on capability
	/// mismatches (vision role pointing at `supports_vision = false`, etc.).
	///
	/// This is the production constructor — `from_instance_config` exists
	/// only so tests can build a router without exercising validation.
	pub fn from_instance_config_strict(
		ai_cfg: &crate::instance_config::AiConfig,
		env_lookup: impl Fn(&str) -> Option<String>,
	) -> Self {
		// Validate provider names before building anything.
		for name in ai_cfg.providers.keys() {
			validate_provider_name(name);
		}
		// Build the merged definition map (defaults + user, user wins).
		let mut merged: std::collections::HashMap<String, ProviderDef> =
			std::collections::HashMap::new();
		for (name, def) in default_provider_registry() {
			merged.insert(name.to_string(), def);
		}
		for (name, def) in &ai_cfg.providers {
			merged.insert(name.clone(), def.clone());
		}

		// Validate headers + auth fields on every (user + default) provider.
		for (name, def) in &merged {
			validate_provider_def_headers_and_auth(name, def);
		}

		// Phase-1 spec gate: any non-OpenAi provider definition is a
		// configuration error today.
		for (name, def) in &merged {
			if def.spec != ProviderSpec::OpenAi {
				panic!(
					"Provider '{name}' has spec={:?} but only spec=\"openai\" is supported \
					 in this release. Anthropic-spec dispatcher is phase 2 of issue #28.",
					def.spec
				);
			}
		}

		// Resolve routing roles + validate they reference known names.
		let (chat_role, vision_role, reasoner_role) = match &ai_cfg.routing {
			Some(r) => {
				let chat = r.chat.clone().unwrap_or_else(|| {
					panic!(
						"[ai.routing] requires 'chat' to be set. Either set it (e.g. \
						 chat = \"deepseek_chat\") or remove the [ai.routing] section \
						 entirely to use defaults."
					)
				});
				validate_role_target("chat", &chat, &merged);
				if let Some(v) = &r.vision {
					validate_role_target("vision", v, &merged);
				}
				if let Some(rs) = &r.reasoner {
					validate_role_target("reasoner", rs, &merged);
				}
				(chat, r.vision.clone(), r.reasoner.clone())
			}
			None => (
				DEFAULT_CHAT_ROLE.to_string(),
				Some(DEFAULT_VISION_ROLE.to_string()),
				Some(DEFAULT_REASONER_ROLE.to_string()),
			),
		};

		// Resolve definitions into available providers; warn for any
		// referenced-but-unavailable name.
		let mut providers = std::collections::HashMap::new();
		let mut unavailable: Vec<String> = Vec::new();
		for (name, def) in merged.iter() {
			match ConfiguredProvider::from_def(name.clone(), def.clone(), &env_lookup) {
				Some(p) => {
					providers.insert(name.clone(), p);
				}
				None => unavailable.push(name.clone()),
			}
		}

		// Warn on unavailable providers REFERENCED by routing or fallback.
		let referenced: std::collections::HashSet<String> = std::iter::once(chat_role.clone())
			.chain(vision_role.clone())
			.chain(reasoner_role.clone())
			.chain(ai_cfg.fallback.on_censored.iter().cloned())
			.collect();
		for name in &unavailable {
			if referenced.contains(name) {
				let env_var = merged
					.get(name)
					.map(|d| d.api_key_env.as_str())
					.unwrap_or("?");
				tracing::warn!(
					"AI provider '{name}' is referenced by routing or fallback but its \
					 API key env var '{env_var}' is unset; provider unavailable"
				);
			}
		}

		// Capability sanity warnings (non-fatal).
		if let Some(v) = &vision_role {
			if let Some(p) = providers.get(v) {
				if !p.supports_vision {
					tracing::warn!(
						"[ai.routing] vision = '{v}' but provider has supports_vision=false; \
						 image messages may not be handled correctly"
					);
				}
			}
		}
		if let Some(rs) = &reasoner_role {
			if let Some(p) = providers.get(rs) {
				if !p.is_reasoner {
					tracing::warn!(
						"[ai.routing] reasoner = '{rs}' but provider has is_reasoner=false; \
						 the slow-thinking model routing may not behave as intended"
					);
				}
			}
		}

		Self {
			providers,
			chat_role,
			vision_role,
			reasoner_role,
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

fn validate_provider_def_headers_and_auth(name: &str, def: &ProviderDef) {
	if def.auth_header.trim().is_empty() {
		panic!(
			"Provider '{name}' has auth_header = \"{}\" (empty or whitespace). \
			 auth_header must be a non-empty header name like \"Authorization\" or \"x-api-key\".",
			def.auth_header
		);
	}
	for (key, value) in &def.headers {
		if key.trim().is_empty() {
			panic!(
				"Provider '{name}' has an empty header key in its headers map. \
				 HTTP header names must be non-empty."
			);
		}
		if value.chars().any(|c| c.is_ascii_control() || !c.is_ascii()) {
			panic!(
				"Provider '{name}' header '{key}' has a value containing non-printable \
				 or non-ASCII characters. HTTP header values must be printable ASCII."
			);
		}
	}
}

fn validate_provider_name(name: &str) {
	let trimmed = name.trim();
	if trimmed.is_empty() {
		panic!("Provider name must not be empty (after trim)");
	}
	if name.chars().any(|c| c.is_whitespace()) {
		panic!(
			"Provider name '{name}' contains whitespace. Use underscores or \
			 hyphens for separators (e.g. 'my_local' or 'my-local')"
		);
	}
}

fn validate_role_target(
	role: &str,
	target_name: &str,
	merged: &std::collections::HashMap<String, ProviderDef>,
) {
	if !merged.contains_key(target_name) {
		let mut known: Vec<&str> = merged.keys().map(|s| s.as_str()).collect();
		known.sort_unstable();
		panic!(
			"[ai.routing] {role} = '{target_name}' is an unknown provider name. \
			 Configured names: {known:?}. Add a [ai.providers.{target_name}] section \
			 or fix the typo."
		);
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
	fn router_named_aliases_dont_apply_to_user_definitions() {
		// Aliases ONLY map "gemini" → "gemini_flash" etc. for the default
		// registry. A user with a provider literally named "deepseek" is found
		// directly; the alias path doesn't fire and isn't needed.
		//
		// Build a router with a user-defined provider literally named "deepseek"
		// (distinct from the alias target "deepseek_chat"). Direct lookup wins.
		let user_def = def_for("https://user-deepseek.example/v1/chat", "USER_DEEPSEEK_KEY");
		let cfg = ai_cfg(vec![("deepseek", user_def)], None);
		let r = ProviderRouter::from_instance_config(
			&cfg,
			env_with(&[("USER_DEEPSEEK_KEY", "secret")]),
		);
		// Direct match for "deepseek" returns the USER's provider URL, not the
		// alias-routed deepseek_chat.
		let p = r.named("deepseek").unwrap();
		assert_eq!(p.url(), "https://user-deepseek.example/v1/chat");
		assert_eq!(p.name(), "deepseek");
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

	// --- from_instance_config merge + routing ---------------------------

	use crate::instance_config::{AiConfig, AiFallbackConfig, RoutingConfig};

	fn ai_cfg(providers: Vec<(&str, ProviderDef)>, routing: Option<RoutingConfig>) -> AiConfig {
		AiConfig {
			providers: providers
				.into_iter()
				.map(|(n, d)| (n.to_string(), d))
				.collect(),
			routing,
			fallback: AiFallbackConfig::default(),
		}
	}

	fn def_for(url: &str, key_env: &str) -> ProviderDef {
		ProviderDef {
			url: url.to_string(),
			model: "test-model".to_string(),
			api_key_env: key_env.to_string(),
			max_tokens: 4096,
			timeout_secs: 30,
			supports_vision: false,
			supports_tools: true,
			is_reasoner: false,
			spec: ProviderSpec::OpenAi,
			headers: std::collections::HashMap::new(),
			auth_header: "Authorization".to_string(),
			auth_scheme: "Bearer ".to_string(),
		}
	}

	#[test]
	fn from_instance_config_no_user_input_matches_from_defaults() {
		// AiConfig empty + only DEEPSEEK_API_KEY set → behaves like
		// from_defaults: chat and reasoner available, vision unavailable
		// (both deepseek_chat and deepseek_reasoner share DEEPSEEK_API_KEY;
		// gemini_flash needs GEMINI_API_KEY which is not set here).
		let r = ProviderRouter::from_instance_config(
			&AiConfig::default(),
			env_with(&[("DEEPSEEK_API_KEY", "k")]),
		);
		assert!(r.chat().is_some());
		assert!(r.vision().is_none());
		assert!(r.reasoner().is_some());
	}

	#[test]
	fn from_instance_config_user_provider_overrides_default_name() {
		// User redefines gemini_flash with a different URL — user wins.
		let user_def = def_for(
			"https://my-fork-of-gemini.example/v1/chat",
			"GEMINI_API_KEY",
		);
		let cfg = ai_cfg(vec![("gemini_flash", user_def)], None);
		let r = ProviderRouter::from_instance_config(&cfg, env_with(&[("GEMINI_API_KEY", "k")]));
		let gemini = r.named("gemini_flash").unwrap();
		assert_eq!(gemini.url(), "https://my-fork-of-gemini.example/v1/chat");
	}

	#[test]
	fn from_instance_config_routing_chat_only_is_one_model_setup() {
		// Single user-defined provider, only chat routed — vision/reasoner
		// gracefully degrade.
		let user_def = def_for(
			"http://localhost:11434/v1/chat/completions",
			"LOCAL_LLM_KEY",
		);
		let cfg = ai_cfg(
			vec![("my_local", user_def)],
			Some(RoutingConfig {
				chat: Some("my_local".to_string()),
				vision: None,
				reasoner: None,
			}),
		);
		let r = ProviderRouter::from_instance_config(
			&cfg,
			env_with(&[("LOCAL_LLM_KEY", "anything-non-empty")]),
		);
		assert!(r.chat().is_some());
		assert_eq!(r.chat().unwrap().name(), "my_local");
		assert!(r.vision().is_none(), "vision opted out");
		assert!(r.reasoner().is_none(), "reasoner opted out");
	}

	#[test]
	fn from_instance_config_section_absent_uses_default_routing() {
		// AiConfig::default has routing = None → router routes per the
		// hardcoded default matrix.
		let r = ProviderRouter::from_instance_config(
			&AiConfig::default(),
			env_with(&[("DEEPSEEK_API_KEY", "d"), ("GEMINI_API_KEY", "g")]),
		);
		assert_eq!(r.chat().unwrap().name(), "deepseek_chat");
		assert_eq!(r.vision().unwrap().name(), "gemini_flash");
		assert_eq!(r.reasoner().unwrap().name(), "deepseek_reasoner");
	}

	// --- Validation tests -----------------------------------------------

	#[test]
	#[should_panic(expected = "unknown provider")]
	fn validation_panics_on_unknown_chat_role() {
		let cfg = ai_cfg(
			vec![],
			Some(RoutingConfig {
				chat: Some("nonexistent_provider".to_string()),
				vision: None,
				reasoner: None,
			}),
		);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("DEEPSEEK_API_KEY", "k")]));
	}

	#[test]
	#[should_panic(expected = "unknown provider")]
	fn validation_panics_on_unknown_vision_role() {
		let cfg = ai_cfg(
			vec![],
			Some(RoutingConfig {
				chat: Some("deepseek_chat".to_string()),
				vision: Some("typo_provider".to_string()),
				reasoner: None,
			}),
		);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("DEEPSEEK_API_KEY", "k")]));
	}

	#[test]
	#[should_panic(expected = "unknown provider")]
	fn validation_panics_on_unknown_reasoner_role() {
		let cfg = ai_cfg(
			vec![],
			Some(RoutingConfig {
				chat: Some("deepseek_chat".to_string()),
				vision: None,
				reasoner: Some("typo_reasoner".to_string()),
			}),
		);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("DEEPSEEK_API_KEY", "k")]));
	}

	#[test]
	#[should_panic(expected = "[ai.routing] requires 'chat'")]
	fn validation_panics_on_routing_section_without_chat() {
		let cfg = ai_cfg(
			vec![],
			Some(RoutingConfig {
				chat: None,
				vision: Some("gemini_flash".to_string()),
				reasoner: None,
			}),
		);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("GEMINI_API_KEY", "k")]));
	}

	#[test]
	fn validation_no_panic_on_unavailable_default_routing() {
		// No env keys set at all → all default providers unavailable.
		// Today's silent-disable behaviour is preserved: no panic, just
		// warns. chat() etc return None.
		let r = ProviderRouter::from_instance_config_strict(&AiConfig::default(), empty_env());
		assert!(r.chat().is_none());
		assert!(r.vision().is_none());
		assert!(r.reasoner().is_none());
	}

	#[test]
	fn validation_no_panic_on_unavailable_explicit_routing() {
		// User explicitly routes chat to deepseek_chat but no env key.
		// Per corrected spec: warn, don't panic. chat() returns None.
		let cfg = ai_cfg(
			vec![],
			Some(RoutingConfig {
				chat: Some("deepseek_chat".to_string()),
				vision: None,
				reasoner: None,
			}),
		);
		let r = ProviderRouter::from_instance_config_strict(&cfg, empty_env());
		assert!(r.chat().is_none(), "chat unavailable, returns None");
	}

	#[test]
	#[should_panic(expected = "whitespace")]
	fn validation_panics_on_provider_name_with_whitespace() {
		// Defined via toml so the user clearly intended this name.
		let bad_def = def_for("u", "K");
		let cfg = ai_cfg(vec![("bad name", bad_def)], None);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("K", "k")]));
	}

	#[test]
	#[should_panic(expected = "phase 2")]
	fn validation_panics_on_anthropic_spec() {
		// Phase 1 only handles spec="openai". Anthropic dispatcher is phase 2.
		// A user setting spec="anthropic" at this point would fail at request
		// time with a confusing error — better to surface it at startup.
		let mut def = def_for("u", "K");
		def.spec = ProviderSpec::Anthropic;
		let cfg = ai_cfg(vec![("claude", def)], None);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("K", "k")]));
	}

	#[test]
	#[should_panic(expected = "auth_header")]
	fn validation_panics_on_empty_auth_header() {
		let mut def = def_for("https://example.invalid/v1/chat", "KEY");
		def.auth_header = "".to_string();
		let cfg = ai_cfg(vec![("bad", def)], None);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("KEY", "k")]));
	}

	#[test]
	#[should_panic(expected = "auth_header")]
	fn validation_panics_on_whitespace_only_auth_header() {
		let mut def = def_for("https://example.invalid/v1/chat", "KEY");
		def.auth_header = "   ".to_string();
		let cfg = ai_cfg(vec![("bad", def)], None);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("KEY", "k")]));
	}

	#[test]
	#[should_panic(expected = "empty header key")]
	fn validation_panics_on_empty_header_key() {
		let mut def = def_for("https://example.invalid/v1/chat", "KEY");
		def.headers.insert("".to_string(), "value".to_string());
		let cfg = ai_cfg(vec![("bad", def)], None);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("KEY", "k")]));
	}

	#[test]
	#[should_panic(expected = "non-printable")]
	fn validation_panics_on_non_printable_header_value() {
		let mut def = def_for("https://example.invalid/v1/chat", "KEY");
		// Control character (null byte) in header value.
		def.headers
			.insert("x-bad".to_string(), "\0invalid".to_string());
		let cfg = ai_cfg(vec![("bad", def)], None);
		ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("KEY", "k")]));
	}
}
