//! Gemini provider: vision-capable, OpenAI-compatible.
//!
//! Google exposes Gemini through an OpenAI-shape endpoint, so the default
//! [`super::openai_compat_complete`] body is sufficient — no override needed.

use super::AiProvider;

const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";
const GEMINI_MODEL: &str = "gemini-3-flash-preview";

#[derive(Debug, Clone)]
pub struct Gemini {
	api_key: String,
}

impl Gemini {
	pub fn new(api_key: String) -> Self {
		Self { api_key }
	}
}

impl AiProvider for Gemini {
	fn name(&self) -> &'static str {
		"gemini"
	}
	fn url(&self) -> &'static str {
		GEMINI_URL
	}
	fn model(&self) -> &'static str {
		GEMINI_MODEL
	}
	fn api_key(&self) -> &str {
		&self.api_key
	}
	fn supports_vision(&self) -> bool {
		true
	}
	fn max_tokens_limit(&self) -> u32 {
		16384
	}
}
