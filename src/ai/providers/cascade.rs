//! Spec dispatch and CENSORED fallback cascade.

use serde_json::Value;

use super::{complete, complete_anthropic, AiProvider, ApiResponse, ProviderSpec};

/// Dispatch to the right spec-specific completion function based on
/// `provider.spec()`. This is the function every call site in `chat.rs`
/// should use (directly or via `complete_with_cascade`), so that swapping
/// an OpenAI-spec provider for an Anthropic-spec one at routing time is a
/// single-line config change with no chat.rs code change required.
pub async fn complete_dispatch(
	provider: &dyn AiProvider,
	client: &reqwest::Client,
	messages: &[serde_json::Value],
	use_tools: bool,
	max_tokens: u32,
) -> Result<ApiResponse, String> {
	match provider.spec() {
		ProviderSpec::OpenAi => complete(provider, client, messages, use_tools, max_tokens).await,
		ProviderSpec::Anthropic => {
			complete_anthropic(provider, client, messages, use_tools, max_tokens).await
		}
	}
}

/// Try `primary.complete(...)`. If it returns the `CENSORED` sentinel, replay
/// the same `messages` through each provider in `cascade` in order. Returns
/// the first non-CENSORED success along with the provider name that produced
/// it (useful for log lines and optional debug footers).
///
/// Non-CENSORED errors from the primary short-circuit (per the issue brief:
/// "Don't cascade on non-content errors — they're transient and a different
/// provider may have the same problem"). Errors from cascade members are
/// treated as best-effort — we try the next one rather than bail, since the
/// fallback path is meant to maximise the chance of *some* answer.
///
/// Returns `Err("CENSORED")` if every provider in `[primary, ...cascade]` was
/// CENSORED — callers fall back to the existing snarky-message behaviour in
/// that case.
pub async fn complete_with_cascade(
	primary: &dyn AiProvider,
	cascade: &[&dyn AiProvider],
	client: &reqwest::Client,
	messages: &[Value],
	use_tools: bool,
	max_tokens: u32,
) -> Result<(ApiResponse, String), String> {
	match complete_dispatch(primary, client, messages, use_tools, max_tokens).await {
		Ok(r) => return Ok((r, primary.name().to_string())),
		Err(e) if e != "CENSORED" => return Err(e),
		Err(_) => {
			if cascade.is_empty() {
				return Err("CENSORED".to_string());
			}
			tracing::info!(
				"Primary provider {} returned CENSORED; cascading through {} alt(s)",
				primary.name(),
				cascade.len()
			);
		}
	}

	for alt in cascade {
		match complete_dispatch(*alt, client, messages, use_tools, max_tokens).await {
			Ok(r) => {
				tracing::info!("Cascade succeeded via {}", alt.name());
				return Ok((r, alt.name().to_string()));
			}
			Err(e) if e == "CENSORED" => {
				tracing::info!("Cascade member {} also CENSORED; trying next", alt.name());
				continue;
			}
			Err(e) => {
				tracing::warn!("Cascade member {} errored ({e}); trying next", alt.name());
				continue;
			}
		}
	}

	Err("CENSORED".to_string())
}
