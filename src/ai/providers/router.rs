//! Role-based provider routing ([`ProviderRouter`]) and startup validation.

use super::{default_provider_registry, AiProvider, ConfiguredProvider, ProviderDef};

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
	/// Validation (panic on typos, warn on unavailable, capability sanity) is
	/// handled by `from_instance_config_strict`; this constructor just builds
	/// the state without validation and is intended for test use only.
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
				// chat is required when [ai.routing] is present; validated in
				// from_instance_config_strict. For now, fall back to the default
				// if missing so the non-strict path remains well-defined.
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
	/// `[ai.routing]` without `chat`, and on whitespace-bearing provider names.
	/// Warns (non-fatal) on unavailable providers referenced by routing or
	/// fallback, and on capability mismatches (vision role pointing at
	/// `supports_vision = false`, etc.).
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
	/// `deepseek_reasoner`, `gemini_flash`, `grok`) plus these short aliases:
	///
	/// - 0.14.0: `"gemini"`, `"deepseek"`, `"deepseek-chat"`
	/// - 0.18.0: `"deepseek-v4"`, `"deepseek-v4-flash"`, `"deepseek-v4-pro"`,
	///   `"deepseek-reasoner"`
	///
	/// User-defined provider names always go through the canonical lookup
	/// path; aliases only apply to the default registry.
	pub fn named(&self, name: &str) -> Option<&dyn AiProvider> {
		// Direct lookup wins (covers canonical default names + all user-defined).
		if let Some(p) = self.providers.get(name) {
			return Some(p as &dyn AiProvider);
		}
		// Then 0.14.0 aliases for default-registry providers.
		let aliased = match name {
			"gemini" => "gemini_flash",
			"deepseek" | "deepseek-chat" | "deepseek-v4" | "deepseek-v4-flash" => "deepseek_chat",
			"deepseek-reasoner" | "deepseek-v4-pro" => "deepseek_reasoner",
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

pub(super) fn validate_provider_def_headers_and_auth(name: &str, def: &ProviderDef) {
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

pub(super) fn validate_provider_name(name: &str) {
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

pub(super) fn validate_role_target(
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
