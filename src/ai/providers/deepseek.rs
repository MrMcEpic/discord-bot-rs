//! DeepSeek providers: V3 chat (default) and Reasoner (slow, no tools).
//!
//! Both share the same base URL and API key but differ on model name + every
//! capability flag. Splitting them into two structs lets [`super::ProviderRouter`]
//! pick by capability without any model-name string compares.

use std::time::Duration;

use super::AiProvider;

const DEEPSEEK_URL: &str = "https://api.deepseek.com/chat/completions";
const DEEPSEEK_CHAT_MODEL: &str = "deepseek-chat";
const DEEPSEEK_REASONER_MODEL: &str = "deepseek-reasoner";

#[derive(Debug, Clone)]
pub struct DeepSeekChat {
	api_key: String,
}

impl DeepSeekChat {
	pub fn new(api_key: String) -> Self {
		Self { api_key }
	}
}

impl AiProvider for DeepSeekChat {
	fn name(&self) -> &'static str {
		"deepseek-chat"
	}
	fn url(&self) -> &'static str {
		DEEPSEEK_URL
	}
	fn model(&self) -> &'static str {
		DEEPSEEK_CHAT_MODEL
	}
	fn api_key(&self) -> &str {
		&self.api_key
	}
	fn supports_vision(&self) -> bool {
		false
	}
	fn max_tokens_limit(&self) -> u32 {
		8192
	}
}

#[derive(Debug, Clone)]
pub struct DeepSeekReasoner {
	api_key: String,
}

impl DeepSeekReasoner {
	pub fn new(api_key: String) -> Self {
		Self { api_key }
	}
}

impl AiProvider for DeepSeekReasoner {
	fn name(&self) -> &'static str {
		"deepseek-reasoner"
	}
	fn url(&self) -> &'static str {
		DEEPSEEK_URL
	}
	fn model(&self) -> &'static str {
		DEEPSEEK_REASONER_MODEL
	}
	fn api_key(&self) -> &str {
		&self.api_key
	}
	fn supports_vision(&self) -> bool {
		false
	}
	fn supports_tools(&self) -> bool {
		// Reasoner models reject tool calls; calling code routes search through
		// the V3 chat provider before invoking the reasoner with the gathered
		// context.
		false
	}
	fn is_reasoner(&self) -> bool {
		true
	}
	fn max_tokens_limit(&self) -> u32 {
		32768
	}
	fn timeout(&self) -> Duration {
		// Reasoner takes minutes for hard problems — 30s would clip mid-thought.
		Duration::from_secs(300)
	}
}
