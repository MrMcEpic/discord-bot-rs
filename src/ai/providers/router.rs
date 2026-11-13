//! Role-based provider routing ([`ProviderRouter`]) and startup validation.

use std::collections::{HashMap, HashSet};

use super::registry::{alias_target, defaults};
use super::{default_provider_registry, AiProvider, ConfiguredProvider, ProviderDef};

/// Resolved AI provider stack for a single bot instance.
///
/// Built once at startup from the merged registry (defaults + user
/// `[ai.providers]`) and the resolved role map (defaults + user
/// `[ai.routing]`). Holds only AVAILABLE providers — definitions whose
/// `api_key_env` resolved to an unset or empty env var are NOT in the map.
/// Callers look up by role; missing providers return `None` so chat.rs's
/// `Option<&dyn AiProvider>` handling degrades silently when a key is unset.
#[derive(Debug, Default)]
pub struct ProviderRouter {
	/// All available providers, keyed by canonical name.
	providers: HashMap<String, ConfiguredProvider>,
	/// Role → canonical provider name (aliases already resolved).
	/// `chat_role` is always set. `vision_role` / `reasoner_role` are `None`
	/// when the user set `[ai.routing]` without those keys ("graceful
	/// degrade"); `fast_role` `None` means "use chat".
	chat_role: String,
	vision_role: Option<String>,
	reasoner_role: Option<String>,
	fast_role: Option<String>,
}

/// Merge the baked default registry with user `[ai.providers]` (user wins on
/// name collision — whole-definition replace, no field-level merge).
fn merged_registry(ai_cfg: &crate::instance_config::AiConfig) -> HashMap<String, ProviderDef> {
	let mut merged: HashMap<String, ProviderDef> =
		default_provider_registry().into_iter().collect();
	for (name, def) in &ai_cfg.providers {
		merged.insert(name.clone(), def.clone());
	}
	merged
}

/// Resolve a user-facing name to a canonical key in `defs`. A key defined
/// under the literal name wins; otherwise default-registry aliases apply
/// (e.g. `deepseek_chat` → `deepseek_flash`). `None` = unknown name.
fn resolve_name<V>(name: &str, defs: &HashMap<String, V>) -> Option<String> {
	if defs.contains_key(name) {
		return Some(name.to_string());
	}
	alias_target(name)
		.filter(|t| defs.contains_key(*t))
		.map(str::to_string)
}

/// Role names before alias resolution: `[ai.routing]` if present, else the
/// defaults file's `[routing]` table.
fn requested_roles(
	ai_cfg: &crate::instance_config::AiConfig,
	strict: bool,
) -> (String, Option<String>, Option<String>, Option<String>) {
	match &ai_cfg.routing {
		Some(r) => {
			// Layer 2: section present, only user-set keys take effect.
			let chat = r.chat.clone().unwrap_or_else(|| {
				if strict {
					panic!(
						"[ai.routing] requires 'chat' to be set. Either set it (e.g. \
						 chat = \"deepseek_flash\") or remove the [ai.routing] section \
						 entirely to use defaults."
					)
				}
				defaults().routing.chat.clone()
			});
			(chat, r.vision.clone(), r.reasoner.clone(), r.fast.clone())
		}
		None => {
			// Layer 1: section absent, full default matrix.
			let d = &defaults().routing;
			(
				d.chat.clone(),
				Some(d.vision.clone()),
				Some(d.reasoner.clone()),
				Some(d.fast.clone()),
			)
		}
	}
}

impl ProviderRouter {
	/// Build a router from the default registry + default routing only —
	/// used by tests that don't need to exercise user definitions.
	///
	/// `env_lookup` is injected so tests can pass a fake without touching
	/// the process env. Production callers pass `|k| std::env::var(k).ok()`.
	#[allow(dead_code)] // test-only; called from mod tests — not reachable from bin
	pub fn from_defaults(env_lookup: impl Fn(&str) -> Option<String>) -> Self {
		Self::from_instance_config(&crate::instance_config::AiConfig::default(), env_lookup)
	}

	/// Build the router from instance config + env, without validation.
	/// Unknown role names silently resolve to "no provider". Test-only; the
	/// bot uses [`Self::from_instance_config_strict`].
	#[allow(dead_code)] // test-only; called from mod tests — not reachable from bin
	pub(crate) fn from_instance_config(
		ai_cfg: &crate::instance_config::AiConfig,
		env_lookup: impl Fn(&str) -> Option<String>,
	) -> Self {
		let merged = merged_registry(ai_cfg);
		let (chat, vision, reasoner, fast) = requested_roles(ai_cfg, false);
		let resolve = |n: &str| resolve_name(n, &merged).unwrap_or_else(|| n.to_string());

		let mut providers = HashMap::new();
		for (name, def) in merged.iter() {
			if let Some(p) = ConfiguredProvider::from_def(name.clone(), def.clone(), &env_lookup) {
				providers.insert(name.clone(), p);
			}
		}

		Self {
			providers,
			chat_role: resolve(&chat),
			vision_role: vision.as_deref().map(resolve),
			reasoner_role: reasoner.as_deref().map(resolve),
			fast_role: fast.as_deref().map(resolve),
		}
	}

	/// Production constructor. Same as [`Self::from_instance_config`] plus
	/// startup validation:
	///
	/// - **Panics** on whitespace-bearing provider names, bad header/auth
	///   fields, `[ai.routing]` without `chat`, and role targets that are
	///   neither a defined provider nor a known alias (typo case).
	/// - **Warns** (non-fatal) on providers referenced by routing or fallback
	///   whose API key env var is unset, and on capability mismatches
	///   (vision role without `supports_vision`, etc.).
	pub fn from_instance_config_strict(
		ai_cfg: &crate::instance_config::AiConfig,
		env_lookup: impl Fn(&str) -> Option<String>,
	) -> Self {
		for name in ai_cfg.providers.keys() {
			validate_provider_name(name);
		}
		let merged = merged_registry(ai_cfg);
		for (name, def) in &merged {
			validate_provider_def_headers_and_auth(name, def);
		}

		let (chat, vision, reasoner, fast) = requested_roles(ai_cfg, true);
		let chat_role = validate_role_target("chat", &chat, &merged);
		let vision_role = vision.map(|v| validate_role_target("vision", &v, &merged));
		let reasoner_role = reasoner.map(|r| validate_role_target("reasoner", &r, &merged));
		let fast_role = fast.map(|f| validate_role_target("fast", &f, &merged));

		let mut providers = HashMap::new();
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
		let referenced: HashSet<String> = std::iter::once(chat_role.clone())
			.chain(vision_role.clone())
			.chain(reasoner_role.clone())
			.chain(fast_role.clone())
			.chain(
				ai_cfg
					.fallback
					.on_censored
					.iter()
					.map(|n| resolve_name(n, &merged).unwrap_or_else(|| n.clone())),
			)
			.collect();
		unavailable.sort_unstable();
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
		if let Some(p) = vision_role.as_ref().and_then(|v| providers.get(v)) {
			if !p.supports_vision {
				tracing::warn!(
					"[ai.routing] vision = '{}' but provider has supports_vision=false; \
					 image messages may not be handled correctly",
					p.name()
				);
			}
		}
		if let Some(p) = reasoner_role.as_ref().and_then(|r| providers.get(r)) {
			if !p.is_reasoner {
				tracing::warn!(
					"[ai.routing] reasoner = '{}' but provider has is_reasoner=false; \
					 the slow-thinking model routing may not behave as intended",
					p.name()
				);
			}
		}

		Self {
			providers,
			chat_role,
			vision_role,
			reasoner_role,
			fast_role,
		}
	}

	fn get(&self, name: &str) -> Option<&dyn AiProvider> {
		self.providers.get(name).map(|p| p as &dyn AiProvider)
	}

	/// The vision-capable provider (None if no vision role configured OR the
	/// configured provider is unavailable).
	pub fn vision(&self) -> Option<&dyn AiProvider> {
		self.get(self.vision_role.as_deref()?)
	}

	/// The default text-chat provider (None if unavailable).
	pub fn chat(&self) -> Option<&dyn AiProvider> {
		self.get(&self.chat_role)
	}

	/// The reasoning-class provider (None if no reasoner role configured OR
	/// the configured provider is unavailable).
	pub fn reasoner(&self) -> Option<&dyn AiProvider> {
		self.get(self.reasoner_role.as_deref()?)
	}

	/// The cheap utility provider for classifier / refusal-check calls.
	/// Falls back to [`Self::chat`] when no fast role is configured or the
	/// configured one is unavailable.
	pub fn fast(&self) -> Option<&dyn AiProvider> {
		self.fast_role
			.as_deref()
			.and_then(|f| self.get(f))
			.or_else(|| self.chat())
	}

	/// Look up a provider by name. Returns `None` for unknown names AND
	/// unavailable providers. Accepts canonical names plus the aliases in
	/// `defaults/providers.toml` `[aliases]` (e.g. `deepseek_chat`,
	/// `gemini`, `deepseek-v4-flash`). A provider defined under the literal
	/// name always wins over an alias.
	pub fn named(&self, name: &str) -> Option<&dyn AiProvider> {
		self.get(&resolve_name(name, &self.providers)?)
	}

	/// Resolve an ordered list of provider names into an ordered Vec of
	/// available providers, skipping unknown / unavailable names with a
	/// `tracing::warn!`.
	pub fn cascade_for(&self, names: &[String]) -> Vec<&dyn AiProvider> {
		let mut out: Vec<&dyn AiProvider> = Vec::with_capacity(names.len());
		for n in names {
			match self.named(n) {
				Some(p) => {
					if !out.iter().any(|q| q.name() == p.name()) {
						out.push(p);
					}
				}
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

/// Validate a role target and return its canonical provider name. Accepts
/// defined provider names and default-registry aliases; panics on anything
/// else (typo case) so a bad config fails loudly at startup.
pub(super) fn validate_role_target(
	role: &str,
	target_name: &str,
	merged: &HashMap<String, ProviderDef>,
) -> String {
	resolve_name(target_name, merged).unwrap_or_else(|| {
		let mut known: Vec<&str> = merged.keys().map(|s| s.as_str()).collect();
		known.sort_unstable();
		panic!(
			"[ai.routing] {role} = '{target_name}' is an unknown provider name. 			 Configured names: {known:?}. Add a [ai.providers.{target_name}] section 			 or fix the typo."
		)
	})
}
