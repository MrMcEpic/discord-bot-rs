//! Baked-in default provider registry, routing matrix, and name aliases.
//!
//! All of it lives in `defaults/providers.toml`, compiled into the binary via
//! `include_str!` and parsed once on first use. Keeping the defaults as data
//! (not Rust literals) means a model bump is a one-line TOML edit, and the
//! file doubles as a copy-paste reference for instance `config.toml`
//! overrides, which use the exact same schema.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::ProviderDef;

/// Raw contents of `defaults/providers.toml`.
pub const DEFAULTS_TOML: &str = include_str!("../../../defaults/providers.toml");

/// Parsed shape of `defaults/providers.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
	pub routing: DefaultRouting,
	#[serde(default)]
	pub aliases: BTreeMap<String, String>,
	pub providers: BTreeMap<String, ProviderDef>,
}

/// Default role → provider-name matrix (used when an instance has no
/// `[ai.routing]` section).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultRouting {
	pub chat: String,
	pub vision: String,
	pub reasoner: String,
	pub fast: String,
}

static DEFAULTS: LazyLock<Defaults> = LazyLock::new(|| {
	let d: Defaults = toml::from_str(DEFAULTS_TOML)
		.unwrap_or_else(|e| panic!("defaults/providers.toml is invalid: {e}"));
	// Internal consistency: every role and alias target must name a provider
	// defined in the same file. Pinned by a unit test, so a bad edit fails CI
	// rather than the bot at startup.
	for (role, target) in [
		("chat", &d.routing.chat),
		("vision", &d.routing.vision),
		("reasoner", &d.routing.reasoner),
		("fast", &d.routing.fast),
	] {
		assert!(
			d.providers.contains_key(target),
			"defaults/providers.toml: routing.{role} = '{target}' is not a defined provider"
		);
	}
	for (alias, target) in &d.aliases {
		assert!(
			d.providers.contains_key(target),
			"defaults/providers.toml: alias '{alias}' → '{target}' is not a defined provider"
		);
		assert!(
			!d.providers.contains_key(alias),
			"defaults/providers.toml: alias '{alias}' shadows a provider of the same name"
		);
	}
	d
});

/// The parsed defaults (providers + routing + aliases).
pub fn defaults() -> &'static Defaults {
	&DEFAULTS
}

/// The providers the bot ships with, as `(name, definition)` pairs. Used as
/// the base registry; user-defined `[ai.providers]` merge on top (user wins
/// on name collision).
pub fn default_provider_registry() -> Vec<(String, ProviderDef)> {
	DEFAULTS
		.providers
		.iter()
		.map(|(k, v)| (k.clone(), v.clone()))
		.collect()
}

/// Resolve a default-registry alias (e.g. `"deepseek_chat"` →
/// `"deepseek_flash"`). Returns `None` for names that aren't aliases.
pub fn alias_target(name: &str) -> Option<&'static str> {
	DEFAULTS.aliases.get(name).map(String::as_str)
}
