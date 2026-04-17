use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct InstanceConfig {
	pub bot_name: String,
	pub command_prefix: String,
	/// Parent command name. Defaults to `"m"` so users invoke `<prefix>m <subcommand>`
	/// (the historical form). Set to a different word for multi-bot guilds where one
	/// bot answers to `<prefix>m play` and another to `<prefix>staff play`. Set to
	/// the empty string `""` to skip the parent entirely so commands are flat —
	/// `<prefix>play`, `<prefix>skip`, etc.
	#[serde(default = "default_command_root")]
	pub command_root: String,
	#[serde(default = "default_personality_file")]
	pub personality_file: String,
	#[serde(default)]
	pub features: Features,
	#[serde(default)]
	pub ai: AiConfig,
	pub auto_role: Option<AutoRoleConfig>,
	pub minecraft: Option<MinecraftConfig>,
	pub join_role: Option<JoinRoleConfig>,
	pub welcome: Option<WelcomeConfig>,
}

/// AI-related per-instance settings. Default = empty (existing behaviour: no
/// fallback cascade on CENSORED, snarky-message canned reply fires).
#[derive(Debug, Deserialize, Default, Clone)]
pub struct AiConfig {
	/// User-defined providers, keyed by name. Merged with the baked default
	/// registry at startup — user names win on collision.
	#[serde(default)]
	pub providers: std::collections::HashMap<String, crate::ai::providers::ProviderDef>,

	/// Optional role overrides. When the entire section is absent, the
	/// router uses defaults (chat = "deepseek_chat", vision = "gemini_flash",
	/// reasoner = "deepseek_reasoner"). When present, only the keys you set
	/// take effect — omitted vision/reasoner means "graceful degrade".
	pub routing: Option<RoutingConfig>,

	#[serde(default)]
	pub fallback: AiFallbackConfig,
}

/// User overrides for which configured provider plays which role.
#[derive(Debug, Deserialize, Clone)]
pub struct RoutingConfig {
	/// Required when the section is present. Panic at startup if missing.
	pub chat: Option<String>,
	pub vision: Option<String>,
	pub reasoner: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AiFallbackConfig {
	/// Ordered provider names to retry through when the primary provider hits
	/// a content-moderation refusal (DeepSeek's `"Content Exists Risk"` →
	/// `Err("CENSORED")`). Recognised names: `"grok"`, `"gemini"`, `"deepseek"`.
	/// First non-CENSORED success wins; if every entry also CENSORS, the bot
	/// falls back to its existing snarky-reply canned message.
	///
	/// Default empty (opt-in) — strict-moderation servers want the snarky
	/// reply behaviour preserved. Names that resolve to a missing API key or
	/// an unknown name are skipped at startup with a warning.
	#[serde(default)]
	pub on_censored: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Features {
	#[serde(default)]
	pub minecraft: bool,
	#[serde(default)]
	pub auto_role: bool,
	#[serde(default)]
	pub join_role: bool,
	#[serde(default)]
	pub welcome: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MinecraftConfig {
	#[serde(default = "default_true")]
	pub verify: bool,
	#[serde(default)]
	pub donator_sync: bool,
	#[serde(default)]
	pub chargeback: bool,
	pub donator_sync_config: Option<DonatorSyncConfig>,
	pub chargeback_config: Option<ChargebackConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChargebackConfig {
	pub staff_channel: String,
	pub restricted_role: String,
	#[serde(default)]
	pub staff_roles: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AutoRoleConfig {
	pub from_role: String,
	pub to_role: String,
	#[serde(default = "default_min_age")]
	pub min_age: String,
	#[serde(default = "default_min_messages")]
	pub min_messages: i64,
	#[serde(default)]
	pub require_all: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DonatorSyncConfig {
	pub supporter_role: String,
	pub premium_role: String,
	#[serde(default = "default_check_interval")]
	pub check_interval: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JoinRoleConfig {
	pub role: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WelcomeConfig {
	pub channel: String,
	#[serde(default = "default_welcome_prompt_file")]
	pub prompt_file: String,
}

fn default_true() -> bool {
	true
}

fn default_check_interval() -> u64 {
	300
}

fn default_min_age() -> String {
	"3d".to_string()
}

fn default_min_messages() -> i64 {
	20
}

fn default_personality_file() -> String {
	"personality.txt".to_string()
}

fn default_command_root() -> String {
	"m".to_string()
}

/// Reject command_root values that wouldn't parse as a single command word.
/// Empty is allowed (means: register subcommands at the root). Anything with
/// whitespace breaks poise's prefix parser; the leading `<prefix>` is what
/// the parser splits on.
pub fn validate_command_root(s: &str) -> Result<(), String> {
	if s.chars().any(char::is_whitespace) {
		return Err(format!(
			"command_root '{s}' contains whitespace; must be a single token (or empty for flat commands)"
		));
	}
	Ok(())
}

fn default_welcome_prompt_file() -> String {
	"welcome_prompt.txt".to_string()
}

impl InstanceConfig {
	pub fn load(config_dir: &Path) -> Self {
		let config_path = config_dir.join("config.toml");
		let content = std::fs::read_to_string(&config_path)
			.unwrap_or_else(|e| panic!("Failed to read {}: {e}", config_path.display()));
		let cfg: InstanceConfig = toml::from_str(&content)
			.unwrap_or_else(|e| panic!("Failed to parse {}: {e}", config_path.display()));
		validate_command_root(&cfg.command_root)
			.unwrap_or_else(|e| panic!("Invalid command_root in {}: {e}", config_path.display()));
		cfg
	}

	pub fn load_personality(&self, config_dir: &Path) -> String {
		let path = config_dir.join(&self.personality_file);
		let content = std::fs::read_to_string(&path)
			.unwrap_or_else(|e| panic!("Failed to read personality file {}: {e}", path.display()));
		if content.trim().is_empty() {
			panic!("Personality file {} is empty", path.display());
		}
		content
	}

	pub fn load_welcome_prompt(&self, config_dir: &Path) -> Option<String> {
		let wc = self.welcome.as_ref()?;
		let path = config_dir.join(&wc.prompt_file);
		match std::fs::read_to_string(&path) {
			Ok(content) if !content.trim().is_empty() => Some(content),
			Ok(_) => {
				tracing::warn!("Welcome prompt file {} is empty", path.display());
				None
			}
			Err(e) => {
				tracing::warn!("Failed to read welcome prompt file {}: {e}", path.display());
				None
			}
		}
	}

	pub fn config_dir() -> PathBuf {
		let dir = std::env::var("CONFIG_DIR").unwrap_or_else(|_| ".".to_string());
		PathBuf::from(dir)
	}
}

#[cfg(test)]
mod tests {
	use super::{validate_command_root, AiConfig, InstanceConfig};

	#[test]
	fn validate_command_root_accepts_default() {
		assert!(validate_command_root("m").is_ok());
	}

	#[test]
	fn validate_command_root_accepts_alt_names() {
		assert!(validate_command_root("bot").is_ok());
		assert!(validate_command_root("staff").is_ok());
		assert!(validate_command_root("Bot_42").is_ok());
	}

	#[test]
	fn validate_command_root_accepts_empty_for_flat_commands() {
		assert!(validate_command_root("").is_ok());
	}

	#[test]
	fn validate_command_root_rejects_whitespace() {
		assert!(validate_command_root("my bot").is_err());
		assert!(validate_command_root("bot ").is_err());
		assert!(validate_command_root(" bot").is_err());
		assert!(validate_command_root("a\tb").is_err());
	}

	// --- AI provider schema parsing -------------------------------------

	fn parse_ai_config(toml_str: &str) -> AiConfig {
		// Parse a complete InstanceConfig with the given [ai] section to
		// avoid having to fabricate top-level required fields.
		let full = format!(
			r#"
bot_name = "Test Bot"
command_prefix = "!"

{toml_str}
"#
		);
		let cfg: InstanceConfig = toml::from_str(&full).expect("toml must parse");
		cfg.ai
	}

	#[test]
	fn ai_section_absent_yields_empty_default() {
		let ai = parse_ai_config("");
		assert!(ai.providers.is_empty());
		assert!(ai.routing.is_none());
		assert!(ai.fallback.on_censored.is_empty());
	}

	#[test]
	fn ai_providers_minimal_user_definition_parses() {
		let ai = parse_ai_config(
			r#"
[ai.providers.my_local]
url = "http://localhost:11434/v1/chat/completions"
model = "llama3.1:70b"
api_key_env = "LOCAL_LLM_KEY"
max_tokens = 8192
"#,
		);
		assert_eq!(ai.providers.len(), 1);
		let def = &ai.providers["my_local"];
		assert_eq!(def.url, "http://localhost:11434/v1/chat/completions");
		assert_eq!(def.model, "llama3.1:70b");
		assert_eq!(def.api_key_env, "LOCAL_LLM_KEY");
		assert_eq!(def.max_tokens, 8192);
		// Optional fields default correctly.
		assert_eq!(def.timeout_secs, 30);
		assert!(!def.supports_vision);
		assert!(def.supports_tools);
		assert!(!def.is_reasoner);
	}

	#[test]
	fn ai_providers_optional_fields_parse() {
		let ai = parse_ai_config(
			r#"
[ai.providers.fancy]
url = "https://example.com/v1/chat/completions"
model = "fancy-model"
api_key_env = "FANCY_KEY"
max_tokens = 32000
timeout_secs = 120
supports_vision = true
supports_tools = false
is_reasoner = true
spec = "openai"
"#,
		);
		let def = &ai.providers["fancy"];
		assert_eq!(def.timeout_secs, 120);
		assert!(def.supports_vision);
		assert!(!def.supports_tools);
		assert!(def.is_reasoner);
	}

	#[test]
	fn ai_routing_section_with_chat_only_parses() {
		let ai = parse_ai_config(
			r#"
[ai.routing]
chat = "my_local"
"#,
		);
		let routing = ai.routing.expect("routing section present");
		assert_eq!(routing.chat.as_deref(), Some("my_local"));
		assert!(routing.vision.is_none());
		assert!(routing.reasoner.is_none());
	}

	#[test]
	fn ai_routing_section_full_parses() {
		let ai = parse_ai_config(
			r#"
[ai.routing]
chat = "x"
vision = "y"
reasoner = "z"
"#,
		);
		let routing = ai.routing.expect("routing section present");
		assert_eq!(routing.chat.as_deref(), Some("x"));
		assert_eq!(routing.vision.as_deref(), Some("y"));
		assert_eq!(routing.reasoner.as_deref(), Some("z"));
	}

	#[test]
	fn ai_fallback_unchanged_from_v0_14_0() {
		let ai = parse_ai_config(
			r#"
[ai.fallback]
on_censored = ["grok", "gemini_flash"]
"#,
		);
		assert_eq!(ai.fallback.on_censored, vec!["grok", "gemini_flash"]);
	}

	#[test]
	fn ai_unknown_fields_in_provider_def_tolerated() {
		// No deny_unknown_fields — extra keys in a provider def parse cleanly.
		// Forward-compat for phase 2 (e.g. `headers = { ... }`).
		let ai = parse_ai_config(
			r#"
[ai.providers.x]
url = "u"
model = "m"
api_key_env = "K"
max_tokens = 1000
some_phase_2_field = "ignored"
"#,
		);
		assert!(ai.providers.contains_key("x"));
	}
}
