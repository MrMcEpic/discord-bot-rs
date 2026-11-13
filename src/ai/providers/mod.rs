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

pub mod anthropic;
pub mod cascade;
pub mod configured;
pub mod openai;
pub mod registry;
pub mod router;

use std::time::Duration;

pub use anthropic::complete_anthropic;
pub use cascade::{
	complete_dispatch, complete_first_success, complete_with_cascade, is_soft_refusal, Cascade,
};
pub use configured::{ConfiguredProvider, ProviderDef, ProviderSpec};
pub use openai::complete;
pub use registry::default_provider_registry;
pub use router::ProviderRouter;

#[cfg(test)]
use anthropic::{
	parse_anthropic_response, parse_data_url, translate_messages_to_anthropic,
	translate_tool_defs_to_anthropic,
};

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
	/// Short human label for log lines (e.g. "deepseek_chat", "gemini_flash").
	fn name(&self) -> &str;

	/// HTTPS endpoint for the chat-completions request.
	fn url(&self) -> &str;

	/// Model identifier passed in the request body.
	fn model(&self) -> &str;

	/// Bearer token for the `Authorization` header.
	fn api_key(&self) -> &str;

	/// Whether the model accepts image content parts in messages.
	fn supports_vision(&self) -> bool;

	/// Whether the model accepts a `tools` array. This alone decides whether
	/// tools are sent (since 0.19.0 `is_reasoner` no longer strips them).
	fn supports_tools(&self) -> bool {
		true
	}

	/// Whether this is a slow reasoning-class model (routing metadata only —
	/// used for startup sanity warnings on the `reasoner` role).
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

	/// Name of the HTTP auth header. Default `"Authorization"`. Anthropic
	/// uses `"x-api-key"`. Override by setting `auth_header` in the
	/// provider's `ProviderDef`.
	fn auth_header(&self) -> &str {
		"Authorization"
	}

	/// Prefix prepended to the API key in the auth header value. Default
	/// `"Bearer "` (note trailing space). Anthropic uses `""` (empty).
	fn auth_scheme(&self) -> &str {
		"Bearer "
	}

	/// Extra HTTP headers attached to every request. Default empty.
	/// Anthropic uses `{"anthropic-version": "2023-06-01"}`.
	fn extra_headers(&self) -> &std::collections::HashMap<String, String> {
		use std::sync::LazyLock;
		static EMPTY: LazyLock<std::collections::HashMap<String, String>> =
			LazyLock::new(std::collections::HashMap::new);
		&EMPTY
	}

	/// Extra fields merged into every request body (e.g. `reasoning_effort`,
	/// `thinking`). Default empty.
	fn extra_body(&self) -> &serde_json::Map<String, serde_json::Value> {
		use std::sync::LazyLock;
		static EMPTY: LazyLock<serde_json::Map<String, serde_json::Value>> =
			LazyLock::new(serde_json::Map::new);
		&EMPTY
	}

	/// Error-body substrings that mean "blocked by the content filter".
	/// Default empty.
	fn refusal_markers(&self) -> &[String] {
		&[]
	}
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

	// --- Default registry (defaults/providers.toml) ----------------------

	fn def_by_name<'a>(registry: &'a [(String, ProviderDef)], name: &str) -> &'a ProviderDef {
		registry
			.iter()
			.find(|(n, _)| n == name)
			.map(|(_, def)| def)
			.unwrap_or_else(|| panic!("default registry missing provider: {name}"))
	}

	const ALL_KEYS: &[(&str, &str)] = &[
		("DEEPSEEK_API_KEY", "d"),
		("GEMINI_API_KEY", "g"),
		("GROK_API_KEY", "x"),
	];

	#[test]
	fn defaults_toml_parses_and_is_self_consistent() {
		// Forces the LazyLock: routing targets and alias targets must all be
		// defined providers, and no alias may shadow a provider. A bad edit
		// to defaults/providers.toml fails here instead of at bot startup.
		let d = registry::defaults();
		for target in [
			&d.routing.chat,
			&d.routing.vision,
			&d.routing.reasoner,
			&d.routing.fast,
		] {
			assert!(d.providers.contains_key(target), "{target} undefined");
		}
		for (alias, target) in &d.aliases {
			assert!(d.providers.contains_key(target), "{alias} -> {target}");
		}
	}

	#[test]
	fn default_registry_provider_set_matches_v0_19_0() {
		let r = default_provider_registry();
		let mut names: Vec<&str> = r.iter().map(|(n, _)| n.as_str()).collect();
		names.sort_unstable();
		assert_eq!(
			names,
			vec![
				"deepseek_flash",
				"deepseek_flash_fast",
				"deepseek_flash_max",
				"deepseek_pro",
				"gemini_flash",
				"grok",
			],
			"default registry provider set changed"
		);
	}

	#[test]
	fn default_routing_matrix_matches_v0_19_0() {
		let d = &registry::defaults().routing;
		assert_eq!(d.chat, "deepseek_flash");
		assert_eq!(d.vision, "deepseek_flash");
		assert_eq!(d.reasoner, "deepseek_flash_max");
		assert_eq!(d.fast, "deepseek_flash_fast");
	}

	#[test]
	fn default_registry_deepseek_flash_matches_v0_19_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "deepseek_flash");
		assert_eq!(def.url, "https://api.deepseek.com/chat/completions");
		assert_eq!(def.model, "deepseek-flash");
		assert_eq!(def.api_key_env, "DEEPSEEK_API_KEY");
		assert_eq!(def.max_tokens, 32768);
		assert_eq!(def.timeout_secs, 90);
		assert!(def.supports_vision, "V4.1 Flash accepts images");
		assert!(def.supports_tools);
		assert!(!def.is_reasoner);
		assert_eq!(def.spec, ProviderSpec::OpenAi);
		assert_eq!(def.extra_body["reasoning_effort"], "high");
		assert_eq!(def.refusal_markers, vec!["Content Exists Risk"]);
		assert_eq!(def.auth_header, "Authorization");
		assert_eq!(def.auth_scheme, "Bearer ");
		assert!(def.headers.is_empty());
	}

	#[test]
	fn default_registry_deepseek_flash_max_matches_v0_19_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "deepseek_flash_max");
		assert_eq!(def.model, "deepseek-flash");
		assert_eq!(def.max_tokens, 65536);
		assert_eq!(def.timeout_secs, 300);
		assert!(def.supports_vision);
		assert!(def.supports_tools, "max-effort Flash keeps tools");
		assert!(def.is_reasoner);
		assert_eq!(def.extra_body["reasoning_effort"], "max");
	}

	#[test]
	fn default_registry_deepseek_flash_fast_disables_thinking() {
		// The classifier asks for max_tokens = 10. With thinking on, the
		// whole budget goes to reasoning and the content comes back empty —
		// the pre-0.19 bug that silently disabled reasoner routing.
		let r = default_provider_registry();
		let def = def_by_name(&r, "deepseek_flash_fast");
		assert_eq!(def.model, "deepseek-flash");
		assert_eq!(def.extra_body["thinking"]["type"], "disabled");
		assert!(
			def.supports_tools,
			"pre-flight search for tool-less reasoners"
		);
		assert!(!def.is_reasoner);
	}

	#[test]
	fn default_registry_deepseek_pro_is_text_only_reasoner() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "deepseek_pro");
		assert_eq!(def.model, "deepseek-v4-pro");
		assert!(!def.supports_vision);
		assert!(def.is_reasoner);
		assert!(def.extra_body.is_empty());
	}

	#[test]
	fn default_registry_gemini_flash_matches_v0_19_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "gemini_flash");
		assert_eq!(
			def.url,
			"https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
		);
		assert_eq!(def.model, "gemini-3.8-flash");
		assert_eq!(def.api_key_env, "GEMINI_API_KEY");
		assert_eq!(def.max_tokens, 32768);
		assert!(def.supports_vision);
		assert!(def.supports_tools);
		assert!(!def.is_reasoner);
	}

	#[test]
	fn default_registry_grok_matches_v0_19_0() {
		let r = default_provider_registry();
		let def = def_by_name(&r, "grok");
		assert_eq!(def.url, "https://api.x.ai/v1/chat/completions");
		assert_eq!(def.model, "grok-4.7");
		assert_eq!(def.api_key_env, "GROK_API_KEY");
		assert_eq!(def.max_tokens, 32768);
		assert!(def.supports_vision, "grok-4.7 accepts images");
		assert!(def.supports_tools);
	}

	// --- ProviderRouter::from_defaults -----------------------------------

	#[test]
	fn router_with_no_keys_picks_nothing() {
		let r = ProviderRouter::from_defaults(empty_env());
		assert!(r.chat().is_none());
		assert!(r.reasoner().is_none());
		assert!(r.vision().is_none());
		assert!(r.fast().is_none());
	}

	#[test]
	fn router_with_deepseek_only_has_every_role() {
		// Since 0.19.0 every role is DeepSeek V4.1 Flash, so one key lights
		// up chat, vision, reasoner and fast.
		let r = ProviderRouter::from_defaults(env_with(&[("DEEPSEEK_API_KEY", "k")]));
		assert_eq!(r.chat().unwrap().name(), "deepseek_flash");
		assert_eq!(r.vision().unwrap().name(), "deepseek_flash");
		assert_eq!(r.reasoner().unwrap().name(), "deepseek_flash_max");
		assert_eq!(r.fast().unwrap().name(), "deepseek_flash_fast");
	}

	#[test]
	fn router_with_gemini_only_has_no_roles_but_named_works() {
		let r = ProviderRouter::from_defaults(env_with(&[("GEMINI_API_KEY", "k")]));
		assert!(r.chat().is_none());
		assert!(r.vision().is_none());
		assert!(r.reasoner().is_none());
		assert!(r.fast().is_none());
		assert!(
			r.named("gemini_flash").is_some(),
			"still usable as fallback"
		);
	}

	#[test]
	fn fast_falls_back_to_chat_when_unrouted() {
		let user_def = def_for("http://localhost/v1/chat", "LOCAL");
		let cfg = ai_cfg(
			vec![("my_local", user_def)],
			Some(RoutingConfig {
				chat: Some("my_local".to_string()),
				vision: None,
				reasoner: None,
				fast: None,
			}),
		);
		let r = ProviderRouter::from_instance_config(&cfg, env_with(&[("LOCAL", "k")]));
		assert_eq!(r.fast().unwrap().name(), "my_local");
	}

	#[test]
	fn capability_flags_match_expected_shape() {
		let r = ProviderRouter::from_defaults(env_with(ALL_KEYS));

		let chat = r.named("deepseek_flash").unwrap();
		assert!(chat.supports_vision());
		assert!(chat.supports_tools());
		assert!(!chat.is_reasoner());

		let max = r.named("deepseek_flash_max").unwrap();
		assert!(max.supports_tools());
		assert!(max.is_reasoner());

		let pro = r.named("deepseek_pro").unwrap();
		assert!(!pro.supports_vision());
		assert!(pro.is_reasoner());

		let gemini = r.named("gemini_flash").unwrap();
		assert!(gemini.supports_vision());
		assert!(!gemini.is_reasoner());

		let grok = r.named("grok").unwrap();
		assert!(grok.supports_vision());
		assert!(grok.supports_tools());
	}

	#[test]
	fn reasoner_has_longer_timeout_than_chat() {
		let r = ProviderRouter::from_defaults(env_with(&[("DEEPSEEK_API_KEY", "k")]));
		assert!(r.reasoner().unwrap().timeout() > r.chat().unwrap().timeout());
	}

	#[test]
	fn extra_body_and_refusal_markers_reach_the_trait() {
		let r = ProviderRouter::from_defaults(env_with(&[("DEEPSEEK_API_KEY", "k")]));
		let fast = r.fast().unwrap();
		assert_eq!(fast.extra_body()["thinking"]["type"], "disabled");
		assert_eq!(fast.refusal_markers(), ["Content Exists Risk".to_string()]);
	}

	// --- named() / cascade_for resolution -------------------------------

	#[test]
	fn router_named_resolves_configured_provider_strings() {
		let r = ProviderRouter::from_defaults(env_with(ALL_KEYS));
		for n in [
			"grok",
			"gemini_flash",
			"deepseek_flash",
			"deepseek_flash_max",
			"deepseek_flash_fast",
			"deepseek_pro",
		] {
			assert_eq!(r.named(n).map(|p| p.name()), Some(n));
		}
		// Unknown name → None, not a panic.
		assert!(r.named("anthropic").is_none());
		assert!(r.named("").is_none());
	}

	#[test]
	fn router_named_returns_none_when_provider_unconfigured() {
		let r = ProviderRouter::from_defaults(env_with(&[("DEEPSEEK_API_KEY", "d")]));
		assert!(r.named("grok").is_none());
		assert!(r.named("gemini_flash").is_none());
	}

	#[test]
	fn cascade_for_preserves_order_and_skips_unconfigured() {
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
	fn router_named_resolves_legacy_aliases_for_backward_compat() {
		// Instance configs in the wild name providers by their 0.14-0.18
		// canonical names or shorthands in [ai.routing] / on_censored.
		let r = ProviderRouter::from_defaults(env_with(ALL_KEYS));
		for (alias, target) in [
			("gemini", "gemini_flash"),
			("xai", "grok"),
			("deepseek", "deepseek_flash"),
			("deepseek_chat", "deepseek_flash"),
			("deepseek-chat", "deepseek_flash"),
			("deepseek-flash", "deepseek_flash"),
			("deepseek-v4", "deepseek_flash"),
			("deepseek-v4-flash", "deepseek_flash"),
			("deepseek_reasoner", "deepseek_flash_max"),
			("deepseek-reasoner", "deepseek_flash_max"),
			("deepseek-v4-pro", "deepseek_pro"),
		] {
			assert_eq!(r.named(alias).map(|p| p.name()), Some(target), "{alias}");
		}
		assert!(r.named("not_a_provider").is_none());
	}

	#[test]
	fn legacy_role_names_in_routing_resolve_and_validate() {
		// [ai.routing] chat = "deepseek_chat" (the pre-0.19 default name)
		// must not crash-loop the bot at startup.
		let cfg = ai_cfg(
			vec![],
			Some(RoutingConfig {
				chat: Some("deepseek_chat".to_string()),
				vision: Some("gemini".to_string()),
				reasoner: Some("deepseek_reasoner".to_string()),
				fast: None,
			}),
		);
		let r = ProviderRouter::from_instance_config_strict(&cfg, env_with(ALL_KEYS));
		assert_eq!(r.chat().unwrap().name(), "deepseek_flash");
		assert_eq!(r.vision().unwrap().name(), "gemini_flash");
		assert_eq!(r.reasoner().unwrap().name(), "deepseek_flash_max");
		assert_eq!(
			r.fast().unwrap().name(),
			"deepseek_flash",
			"fast unset -> chat"
		);
	}

	#[test]
	fn router_named_aliases_dont_apply_to_user_definitions() {
		// Aliases ONLY map "gemini" → "gemini_flash" etc. for the default
		// registry. A user with a provider literally named "deepseek" is found
		// directly; the alias path doesn't fire and isn't needed.
		//
		// Build a router with a user-defined provider literally named "deepseek"
		// (distinct from the alias target "deepseek_flash"). Direct lookup wins.
		let user_def = def_for("https://user-deepseek.example/v1/chat", "USER_DEEPSEEK_KEY");
		let cfg = ai_cfg(vec![("deepseek", user_def)], None);
		let r = ProviderRouter::from_instance_config(
			&cfg,
			env_with(&[("USER_DEEPSEEK_KEY", "secret")]),
		);
		// Direct match for "deepseek" returns the USER's provider URL, not the
		// alias-routed deepseek_flash.
		let p = r.named("deepseek").unwrap();
		assert_eq!(p.url(), "https://user-deepseek.example/v1/chat");
		assert_eq!(p.name(), "deepseek");
	}

	#[test]
	fn cascade_for_dedupes_preserving_first_occurrence() {
		// "grok" and its alias "xai" are the same provider; trying it twice
		// in one cascade just doubles the latency of a failure.
		let r = ProviderRouter::from_defaults(env_with(ALL_KEYS));
		let names = vec![
			"grok".to_string(),
			"gemini_flash".to_string(),
			"xai".to_string(),
		];
		let resolved = r.cascade_for(&names);
		assert_eq!(resolved.len(), 2);
		assert_eq!(resolved[0].name(), "grok");
		assert_eq!(resolved[1].name(), "gemini_flash");
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
			extra_body: serde_json::Map::new(),
			refusal_markers: Vec::new(),
		}
	}

	#[test]
	fn from_instance_config_no_user_input_matches_from_defaults() {
		// AiConfig empty + only GEMINI_API_KEY set: no routed role is
		// available (all default roles are DeepSeek), same as from_defaults.
		let r = ProviderRouter::from_instance_config(
			&AiConfig::default(),
			env_with(&[("GEMINI_API_KEY", "k")]),
		);
		assert!(r.chat().is_none());
		assert!(r.vision().is_none());
		assert!(r.reasoner().is_none());
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
				fast: None,
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
		assert_eq!(r.chat().unwrap().name(), "deepseek_flash");
		assert_eq!(r.vision().unwrap().name(), "deepseek_flash");
		assert_eq!(r.reasoner().unwrap().name(), "deepseek_flash_max");
		assert_eq!(r.fast().unwrap().name(), "deepseek_flash_fast");
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
				fast: None,
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
				fast: None,
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
				fast: None,
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
				fast: None,
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
				fast: None,
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

	// --- parse_data_url --------------------------------------------------

	#[test]
	fn parse_data_url_extracts_media_type_and_data() {
		let (mt, data) = parse_data_url(
			"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8Dw/z8ABf4C/1vvdCkAAAAASUVORK5CYII=",
		)
		.expect("parse");
		assert_eq!(mt, "image/png");
		assert!(data.starts_with("iVBORw0KG"));
	}

	#[test]
	fn parse_data_url_handles_jpeg_media_type() {
		let (mt, data) = parse_data_url("data:image/jpeg;base64,/9j/4AAQSkZJRg==").expect("parse");
		assert_eq!(mt, "image/jpeg");
		assert_eq!(data, "/9j/4AAQSkZJRg==");
	}

	#[test]
	fn parse_data_url_rejects_non_data_scheme() {
		assert!(parse_data_url("https://example.com/pic.png").is_err());
	}

	#[test]
	fn parse_data_url_rejects_missing_base64_marker() {
		assert!(parse_data_url("data:image/png,raw-text-not-base64").is_err());
	}

	// --- Anthropic input translation -------------------------------------

	#[test]
	fn anthropic_extracts_system_prompt_from_first_system_message() {
		let msgs = vec![
			serde_json::json!({"role": "system", "content": "You are helpful."}),
			serde_json::json!({"role": "user", "content": "Hi"}),
		];
		let (system, translated) = translate_messages_to_anthropic(&msgs).expect("translate");
		assert_eq!(system.as_deref(), Some("You are helpful."));
		assert_eq!(translated.len(), 1);
		assert_eq!(translated[0]["role"], "user");
	}

	#[test]
	fn anthropic_passes_user_assistant_messages_through_with_string_content() {
		let msgs = vec![
			serde_json::json!({"role": "user", "content": "hi"}),
			serde_json::json!({"role": "assistant", "content": "hello"}),
		];
		let (_, translated) = translate_messages_to_anthropic(&msgs).expect("translate");
		assert_eq!(translated.len(), 2);
		assert_eq!(translated[0]["content"], "hi");
		assert_eq!(translated[1]["content"], "hello");
	}

	#[test]
	fn anthropic_translates_image_content_part_data_url_to_base64_source() {
		let msgs = vec![serde_json::json!({
			"role": "user",
			"content": [
				{"type": "image_url", "image_url": {"url": "data:image/png;base64,ABCD"}},
				{"type": "text", "text": "describe this"},
			],
		})];
		let (_, translated) = translate_messages_to_anthropic(&msgs).expect("translate");
		let content = translated[0]["content"].as_array().expect("array");
		assert_eq!(content.len(), 2);
		assert_eq!(content[0]["type"], "image");
		assert_eq!(content[0]["source"]["type"], "base64");
		assert_eq!(content[0]["source"]["media_type"], "image/png");
		assert_eq!(content[0]["source"]["data"], "ABCD");
		assert_eq!(content[1]["type"], "text");
		assert_eq!(content[1]["text"], "describe this");
	}

	#[test]
	fn anthropic_drops_subsequent_system_messages_keeping_only_first() {
		let msgs = vec![
			serde_json::json!({"role": "system", "content": "First system prompt."}),
			serde_json::json!({"role": "system", "content": "Second system prompt — should be dropped."}),
			serde_json::json!({"role": "user", "content": "hi"}),
		];
		let (system, translated) = translate_messages_to_anthropic(&msgs).expect("translate");
		assert_eq!(
			system.as_deref(),
			Some("First system prompt."),
			"first-system-wins rule violated"
		);
		// Only the user message remains.
		assert_eq!(translated.len(), 1);
		assert_eq!(translated[0]["role"], "user");
		// Neither system message is in the translated array.
		for msg in &translated {
			assert_ne!(msg["role"], "system");
		}
	}

	#[test]
	fn anthropic_translates_image_gif_media_type() {
		let msgs = vec![serde_json::json!({
			"role": "user",
			"content": [
				{"type": "image_url", "image_url": {"url": "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"}},
			],
		})];
		let (_, translated) = translate_messages_to_anthropic(&msgs).expect("translate");
		let content = translated[0]["content"].as_array().expect("array");
		assert_eq!(content[0]["type"], "image");
		assert_eq!(content[0]["source"]["media_type"], "image/gif");
	}

	#[test]
	fn anthropic_translates_image_webp_media_type() {
		let msgs = vec![serde_json::json!({
			"role": "user",
			"content": [
				{"type": "image_url", "image_url": {"url": "data:image/webp;base64,UklGRhoAAABXRUJQVlA4TA0AAAAvAAAAEAcQERGIiP4HAA=="}},
			],
		})];
		let (_, translated) = translate_messages_to_anthropic(&msgs).expect("translate");
		let content = translated[0]["content"].as_array().expect("array");
		assert_eq!(content[0]["type"], "image");
		assert_eq!(content[0]["source"]["media_type"], "image/webp");
	}

	#[test]
	fn anthropic_translates_tool_result_role_message_to_user_content_block() {
		let msgs = vec![serde_json::json!({
			"role": "tool",
			"tool_call_id": "call_abc",
			"content": "the tool result",
		})];
		let (_, translated) = translate_messages_to_anthropic(&msgs).expect("translate");
		assert_eq!(translated.len(), 1);
		assert_eq!(translated[0]["role"], "user");
		let content = translated[0]["content"].as_array().expect("array");
		assert_eq!(content.len(), 1);
		assert_eq!(content[0]["type"], "tool_result");
		assert_eq!(content[0]["tool_use_id"], "call_abc");
		assert_eq!(content[0]["content"], "the tool result");
	}

	#[test]
	fn anthropic_rejects_non_data_url_image() {
		let msgs = vec![serde_json::json!({
			"role": "user",
			"content": [
				{"type": "image_url", "image_url": {"url": "https://example.com/p.png"}},
			],
		})];
		assert!(translate_messages_to_anthropic(&msgs).is_err());
	}

	// --- Tool definition translation -------------------------------------

	#[test]
	fn anthropic_translates_tool_definitions_function_wrap_to_flat_shape() {
		let openai_shape = vec![serde_json::json!({
			"type": "function",
			"function": {
				"name": "music_play",
				"description": "Play a song",
				"parameters": {
					"type": "object",
					"properties": {"query": {"type": "string"}},
					"required": ["query"],
				},
			},
		})];
		let anthropic_shape = translate_tool_defs_to_anthropic(&openai_shape);
		assert_eq!(anthropic_shape.len(), 1);
		assert_eq!(anthropic_shape[0]["name"], "music_play");
		assert_eq!(anthropic_shape[0]["description"], "Play a song");
		assert_eq!(anthropic_shape[0]["input_schema"]["type"], "object");
		assert_eq!(
			anthropic_shape[0]["input_schema"]["properties"]["query"]["type"],
			"string"
		);
		// No top-level "type": "function" wrapper.
		assert!(anthropic_shape[0]["type"].is_null());
		assert!(anthropic_shape[0]["function"].is_null());
	}

	// --- Anthropic response parsing --------------------------------------

	#[test]
	fn anthropic_parses_single_text_content_block() {
		let body = serde_json::json!({
			"id": "msg_01",
			"role": "assistant",
			"content": [{"type": "text", "text": "hello!"}],
			"stop_reason": "end_turn",
		});
		let resp = parse_anthropic_response(&body).expect("parse");
		assert_eq!(resp.content.as_deref(), Some("hello!"));
		assert!(resp.tool_calls.is_empty());
	}

	#[test]
	fn anthropic_concatenates_multiple_text_blocks() {
		let body = serde_json::json!({
			"content": [
				{"type": "text", "text": "part one"},
				{"type": "text", "text": " part two"},
			],
			"stop_reason": "end_turn",
		});
		let resp = parse_anthropic_response(&body).expect("parse");
		assert_eq!(resp.content.as_deref(), Some("part one part two"));
	}

	#[test]
	fn anthropic_extracts_tool_use_blocks_into_tool_calls() {
		let body = serde_json::json!({
			"content": [
				{"type": "text", "text": "I'll search."},
				{
					"type": "tool_use",
					"id": "toolu_abc",
					"name": "web_search",
					"input": {"query": "rust async"},
				},
			],
			"stop_reason": "tool_use",
		});
		let resp = parse_anthropic_response(&body).expect("parse");
		assert_eq!(resp.content.as_deref(), Some("I'll search."));
		assert_eq!(resp.tool_calls.len(), 1);
		assert_eq!(resp.tool_calls[0].id, "toolu_abc");
		assert_eq!(resp.tool_calls[0].name, "web_search");
		// arguments is the stringified JSON of the input object.
		let parsed: serde_json::Value =
			serde_json::from_str(&resp.tool_calls[0].arguments).expect("json");
		assert_eq!(parsed["query"], "rust async");
	}

	#[test]
	fn anthropic_handles_empty_content_array() {
		let body = serde_json::json!({
			"content": [],
			"stop_reason": "end_turn",
		});
		let resp = parse_anthropic_response(&body).expect("parse");
		assert!(resp.content.is_none());
		assert!(resp.tool_calls.is_empty());
	}

	#[test]
	fn anthropic_parses_dsml_embedded_tool_calls_in_text_content() {
		// Closing tag is <｜DSML｜/invoke> (slash before invoke, after the bar).
		let body = serde_json::json!({
			"content": [{
				"type": "text",
				"text": "before <\u{ff5c}DSML\u{ff5c}invoke name=\"stub\"><\u{ff5c}DSML\u{ff5c}/invoke> after",
			}],
			"stop_reason": "end_turn",
		});
		let resp = parse_anthropic_response(&body).expect("parse");
		// DSML call extracted into tool_calls.
		assert!(resp.tool_calls.iter().any(|t| t.name == "stub"));
		// DSML text stripped from content.
		assert!(!resp.content.as_deref().unwrap_or("").contains("DSML"));
	}

	// --- ConfiguredProvider trait method reflection ----------------------

	#[test]
	fn complete_uses_configurable_auth_header_and_scheme() {
		// The actual HTTP call isn't exercised here (no network in tests).
		// This test just verifies that a ConfiguredProvider built from a
		// ProviderDef with custom auth fields correctly reflects them through
		// the trait.
		let mut def = def_for("https://example.invalid/v1/chat", "KEY");
		def.auth_header = "x-custom-auth".to_string();
		def.auth_scheme = "Token ".to_string();
		def.headers
			.insert("x-req-id".to_string(), "abc".to_string());
		let cfg = ai_cfg(vec![("custom", def)], None);
		let r = ProviderRouter::from_instance_config_strict(&cfg, env_with(&[("KEY", "k")]));
		let p = r.named("custom").unwrap();
		assert_eq!(p.auth_header(), "x-custom-auth");
		assert_eq!(p.auth_scheme(), "Token ");
		assert_eq!(
			p.extra_headers().get("x-req-id").map(String::as_str),
			Some("abc")
		);
	}

	// --- Dispatch --------------------------------------------------------

	#[test]
	fn provider_spec_trait_default_is_openai() {
		#[derive(Debug)]
		struct StubOpenAi;
		impl AiProvider for StubOpenAi {
			fn name(&self) -> &str {
				"stub"
			}
			fn url(&self) -> &str {
				"https://example.invalid"
			}
			fn model(&self) -> &str {
				"m"
			}
			fn api_key(&self) -> &str {
				"k"
			}
			fn supports_vision(&self) -> bool {
				false
			}
			fn max_tokens_limit(&self) -> u32 {
				100
			}
			// Don't override spec() — verify default.
		}
		let s = StubOpenAi;
		assert_eq!(s.spec(), ProviderSpec::OpenAi);
	}

	#[test]
	fn configured_provider_spec_reflects_stored_value() {
		let mut def = def_for("https://example.invalid/v1/chat", "KEY");
		def.spec = ProviderSpec::Anthropic;
		let cfg = ai_cfg(vec![("claude", def)], None);
		// from_instance_config_strict would also work here; using non-strict
		// to keep the test focused on spec storage, not routing.
		let r = ProviderRouter::from_instance_config(&cfg, env_with(&[("KEY", "k")]));
		let p = r.named("claude").unwrap();
		assert_eq!(p.spec(), ProviderSpec::Anthropic);
	}

	#[test]
	fn anthropic_spec_no_longer_panics_after_phase2() {
		// The phase-1 startup gate that rejected spec = "anthropic" is gone.
		// from_instance_config_strict should successfully build a router
		// containing an anthropic-spec provider.
		let mut def = def_for("https://api.anthropic.com/v1/messages", "ANTHROPIC_API_KEY");
		def.spec = ProviderSpec::Anthropic;
		def.auth_header = "x-api-key".to_string();
		def.auth_scheme = "".to_string();
		def.headers
			.insert("anthropic-version".to_string(), "2023-06-01".to_string());
		let cfg = ai_cfg(vec![("claude", def)], None);
		let r = ProviderRouter::from_instance_config_strict(
			&cfg,
			env_with(&[("ANTHROPIC_API_KEY", "k")]),
		);
		let p = r.named("claude").unwrap();
		assert_eq!(p.spec(), ProviderSpec::Anthropic);
		assert_eq!(p.auth_header(), "x-api-key");
		assert_eq!(p.auth_scheme(), "");
	}
}
