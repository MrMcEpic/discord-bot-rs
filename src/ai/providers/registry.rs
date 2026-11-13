//! Baked-in default provider registry.

use super::{ProviderDef, ProviderSpec};

/// The four providers the bot ships with. Used as the base registry when no
/// `[ai.providers]` section is in instance config; user-defined providers
/// merge on top (user wins on name collision).
///
/// Field values here are pinned by `default_registry_*_matches_v0_18_0`
/// snapshot tests in this module. Drift breaks the test.
pub fn default_provider_registry() -> Vec<(&'static str, ProviderDef)> {
	vec![
		(
			"deepseek_chat",
			ProviderDef {
				url: "https://api.deepseek.com/chat/completions".to_string(),
				model: "deepseek-v4-flash".to_string(),
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
				model: "deepseek-v4-pro".to_string(),
				api_key_env: "DEEPSEEK_API_KEY".to_string(),
				max_tokens: 65536,
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
