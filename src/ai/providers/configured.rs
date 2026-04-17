//! Single concrete `AiProvider` implementation.
//!
//! Replaces the per-provider structs (`DeepSeekChat`, `DeepSeekReasoner`,
//! `Gemini`, `Grok`) with one struct whose fields come from either the baked
//! default registry (Rust) or `[ai.providers.*]` user toml. See
//! `docs/configuration/ai-providers.md` for the schema reference.

use serde::Deserialize;
use std::time::Duration;

use super::AiProvider;

/// Phase-1: only `OpenAi`. Phase-2 will add `Anthropic`. The phase-1
/// dispatcher in `chat.rs` errors at startup on anything other than
/// `OpenAi` so misconfigurations surface early.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderSpec {
	#[default]
	OpenAi,
	Anthropic,
}

/// Toml-deserialise shape. All optional fields use serde defaults — caller
/// constructs a [`ConfiguredProvider`] from this via [`ConfiguredProvider::from_def`].
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderDef {
	pub url: String,
	pub model: String,
	pub api_key_env: String,
	pub max_tokens: u32,
	#[serde(default = "default_timeout_secs")]
	pub timeout_secs: u64,
	#[serde(default)]
	pub supports_vision: bool,
	#[serde(default = "default_supports_tools")]
	pub supports_tools: bool,
	#[serde(default)]
	pub is_reasoner: bool,
	#[serde(default)]
	pub spec: ProviderSpec,
	/// Extra HTTP headers attached to every chat-completions request. Used for
	/// Anthropic's required `anthropic-version: 2023-06-01` header; extensible
	/// to any future provider that requires custom headers. Keys must be
	/// non-empty; values must contain only printable ASCII. Validated at
	/// startup — see `validate_provider_def_headers_and_auth`.
	#[serde(default)]
	pub headers: std::collections::HashMap<String, String>,
	/// Name of the auth header. Default `"Authorization"` works for every
	/// OpenAI-compatible endpoint (Bearer-token auth). Anthropic uses
	/// `"x-api-key"`. Must be non-empty after trim.
	#[serde(default = "default_auth_header")]
	pub auth_header: String,
	/// Prefix prepended to the API key in the auth header value. Default
	/// `"Bearer "` (note trailing space). Anthropic uses `""` (empty — the
	/// API key is the full header value).
	#[serde(default = "default_auth_scheme")]
	pub auth_scheme: String,
}

fn default_timeout_secs() -> u64 {
	30
}

fn default_supports_tools() -> bool {
	true
}

fn default_auth_header() -> String {
	"Authorization".to_string()
}

fn default_auth_scheme() -> String {
	"Bearer ".to_string()
}

/// Concrete provider — owned strings (name + url + model can come from user
/// config), `api_key` resolved at construction from the env var named in
/// [`ProviderDef::api_key_env`]. A provider whose env var is unset/empty is
/// considered "unavailable" — [`ConfiguredProvider::from_def`] returns `None`.
#[derive(Debug, Clone)]
pub struct ConfiguredProvider {
	name: String,
	url: String,
	model: String,
	api_key: String,
	pub max_tokens: u32,
	pub timeout: Duration,
	pub supports_vision: bool,
	pub supports_tools: bool,
	pub is_reasoner: bool,
	#[allow(dead_code)]
	pub spec: ProviderSpec,
	#[allow(dead_code)]
	pub headers: std::collections::HashMap<String, String>,
	#[allow(dead_code)]
	pub auth_header: String,
	#[allow(dead_code)]
	pub auth_scheme: String,
}

impl ConfiguredProvider {
	/// Build from a [`ProviderDef`] using `env_lookup` to resolve the API key.
	/// Returns `None` if the env var is unset or empty (after trim) — caller
	/// treats the provider as "defined but unavailable" and either warns
	/// (if it's referenced by routing/fallback) or quietly drops it.
	///
	/// `env_lookup` is injected so unit tests can pass a fake without touching
	/// the process env (which is unsafe to mutate concurrently in Rust tests).
	/// Production callers pass `|k| std::env::var(k).ok()`.
	pub fn from_def(
		name: impl Into<String>,
		def: ProviderDef,
		env_lookup: impl Fn(&str) -> Option<String>,
	) -> Option<Self> {
		let api_key = env_lookup(&def.api_key_env)?.trim().to_string();
		if api_key.is_empty() {
			return None;
		}
		Some(Self {
			name: name.into(),
			url: def.url,
			model: def.model,
			api_key,
			max_tokens: def.max_tokens,
			timeout: Duration::from_secs(def.timeout_secs),
			supports_vision: def.supports_vision,
			supports_tools: def.supports_tools,
			is_reasoner: def.is_reasoner,
			spec: def.spec,
			headers: def.headers,
			auth_header: def.auth_header,
			auth_scheme: def.auth_scheme,
		})
	}
}

impl AiProvider for ConfiguredProvider {
	fn name(&self) -> &str {
		&self.name
	}
	fn url(&self) -> &str {
		&self.url
	}
	fn model(&self) -> &str {
		&self.model
	}
	fn api_key(&self) -> &str {
		&self.api_key
	}
	fn supports_vision(&self) -> bool {
		self.supports_vision
	}
	fn supports_tools(&self) -> bool {
		self.supports_tools
	}
	fn is_reasoner(&self) -> bool {
		self.is_reasoner
	}
	fn max_tokens_limit(&self) -> u32 {
		self.max_tokens
	}
	fn timeout(&self) -> Duration {
		self.timeout
	}

	fn spec(&self) -> super::ProviderSpec {
		self.spec
	}
}
