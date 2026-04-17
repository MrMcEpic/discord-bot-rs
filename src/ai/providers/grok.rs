//! Grok provider (xAI). OpenAI-compatible.
//!
//! Used primarily as a less-restrictive fallback when DeepSeek hits its
//! content-moderation block (`Err("CENSORED")`). Configurable per-instance
//! via the `[ai.fallback] on_censored = ["grok"]` toml field. Also valid as
//! a primary provider if a future deployment chooses.

use super::AiProvider;

const GROK_URL: &str = "https://api.x.ai/v1/chat/completions";
const GROK_MODEL: &str = "grok-3";

#[derive(Debug, Clone)]
pub struct Grok {
	api_key: String,
}

impl Grok {
	pub fn new(api_key: String) -> Self {
		Self { api_key }
	}
}

impl AiProvider for Grok {
	fn name(&self) -> &'static str {
		"grok"
	}
	fn url(&self) -> &'static str {
		GROK_URL
	}
	fn model(&self) -> &'static str {
		GROK_MODEL
	}
	fn api_key(&self) -> &str {
		&self.api_key
	}
	fn supports_vision(&self) -> bool {
		// Grok 3 does support vision, but the routing in chat.rs only sends
		// images to the configured vision provider (Gemini today). Mark false
		// here so an accidental router change doesn't end up sending images
		// to Grok without explicit intent.
		false
	}
	fn max_tokens_limit(&self) -> u32 {
		// Conservative cap — xAI's published cap is higher but we don't need
		// the headroom and it keeps cascade replays inside DeepSeek's window.
		16384
	}
}
