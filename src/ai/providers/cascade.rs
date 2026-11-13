//! Spec dispatch, the fallback cascade, and soft-refusal detection.

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

/// Ordered fallback providers plus the policy for when to use them. Built
/// once per request from `[ai.fallback]` via `ProviderRouter::cascade_for`.
#[derive(Debug, Default, Clone)]
pub struct Cascade<'a> {
	/// Providers to try, in order, after the primary fails.
	pub members: Vec<&'a dyn AiProvider>,
	/// Also cascade on non-content errors (HTTP failure, timeout, no credits).
	pub on_error: bool,
}

impl<'a> Cascade<'a> {
	pub fn new(members: Vec<&'a dyn AiProvider>, on_error: bool) -> Self {
		Self { members, on_error }
	}

	/// Same cascade restricted to vision-capable members — used for
	/// requests that carry image content parts.
	pub fn vision_only(&self) -> Self {
		Self {
			members: self
				.members
				.iter()
				.copied()
				.filter(|p| p.supports_vision())
				.collect(),
			on_error: self.on_error,
		}
	}
}

/// Try `primary`. If it returns the `CENSORED` sentinel (or any error when
/// `cascade.on_error` is set), replay the same `messages` through each
/// cascade member in order, skipping any member that *is* the primary.
/// Returns the first success along with the name of the provider that
/// produced it.
///
/// With `on_error = false`, non-CENSORED errors from the primary
/// short-circuit (transient errors would likely hit the next provider too,
/// and the user gets a clear "try again"). Errors from cascade members are
/// always best-effort — try the next one rather than bail, since the
/// fallback path is meant to maximise the chance of *some* answer.
///
/// Returns `Err("CENSORED")` if the primary was CENSORED and no member
/// produced an answer — callers fall back to the canned snarky reply.
pub async fn complete_with_cascade(
	primary: &dyn AiProvider,
	cascade: &Cascade<'_>,
	client: &reqwest::Client,
	messages: &[Value],
	use_tools: bool,
	max_tokens: u32,
) -> Result<(ApiResponse, String), String> {
	let primary_err =
		match complete_dispatch(primary, client, messages, use_tools, max_tokens).await {
			Ok(r) => return Ok((r, primary.name().to_string())),
			Err(e) => e,
		};
	let censored = primary_err == "CENSORED";
	if !censored && !cascade.on_error {
		return Err(primary_err);
	}
	if cascade.members.iter().all(|m| m.name() == primary.name()) {
		return Err(primary_err);
	}
	tracing::info!(
		"Primary provider {} failed ({primary_err}); cascading through {} alt(s)",
		primary.name(),
		cascade.members.len()
	);

	match complete_first_success(
		cascade,
		Some(primary.name()),
		client,
		messages,
		use_tools,
		max_tokens,
	)
	.await
	{
		Some(ok) => Ok(ok),
		None if censored => Err("CENSORED".to_string()),
		None => Err(primary_err),
	}
}

/// Walk `cascade.members` in order (skipping `skip`, if given) and return the
/// first successful response with its provider name. `None` if every member
/// failed or there were none to try.
pub async fn complete_first_success(
	cascade: &Cascade<'_>,
	skip: Option<&str>,
	client: &reqwest::Client,
	messages: &[Value],
	use_tools: bool,
	max_tokens: u32,
) -> Option<(ApiResponse, String)> {
	for alt in &cascade.members {
		if Some(alt.name()) == skip {
			continue;
		}
		match complete_dispatch(*alt, client, messages, use_tools, max_tokens).await {
			Ok(r) => {
				tracing::info!("Cascade succeeded via {}", alt.name());
				return Some((r, alt.name().to_string()));
			}
			Err(e) if e == "CENSORED" => {
				tracing::info!("Cascade member {} also CENSORED; trying next", alt.name());
			}
			Err(e) => {
				tracing::warn!("Cascade member {} errored ({e}); trying next", alt.name());
			}
		}
	}
	None
}

/// System prompt for the soft-refusal judge. Kept here (not in chat.rs) so
/// it's tested alongside the parsing logic.
const REFUSAL_JUDGE_PROMPT: &str = "You audit replies from a sarcastic Discord chatbot. \
Answer ONLY 'yes' or 'no'.\n\
Answer 'yes' if the ASSISTANT REPLY refuses, dodges, or deflects the USER MESSAGE instead of \
engaging with it — including in-character or joking refusals, changing the subject, claiming \
not to know about a well-known topic, or saying it can't or won't talk about it.\n\
Answer 'no' if the reply actually engages with the request, even briefly, sarcastically, \
or critically. Also answer 'no' when the reply declines something genuinely harmful, illegal, \
or destructive (drugs, weapons, self-harm, mass-banning or other abuse of the bot's powers). \
Answer 'no' for greetings, small talk, and replies to messages that don't ask for anything.";

/// Build the two-message judge prompt for [`is_soft_refusal`]. `prompt`
/// overrides the built-in system prompt (`[ai.fallback] refusal_check_prompt`).
fn refusal_judge_messages(prompt: Option<&str>, user_text: &str, reply: &str) -> Vec<Value> {
	vec![
		serde_json::json!({ "role": "system", "content": prompt.unwrap_or(REFUSAL_JUDGE_PROMPT) }),
		serde_json::json!({
			"role": "user",
			"content": format!("USER MESSAGE:\n{user_text}\n\nASSISTANT REPLY:\n{reply}")
		}),
	]
}

/// Interpret the judge's one-word answer.
fn judge_says_yes(answer: Option<&str>) -> bool {
	answer
		.map(|a| {
			a.trim()
				.trim_start_matches(|c: char| !c.is_alphanumeric())
				.to_lowercase()
				.starts_with("yes")
		})
		.unwrap_or(false)
}

/// Ask the `fast` provider whether `reply` is a soft refusal of `user_text`.
///
/// DeepSeek no longer reliably returns an error for filtered topics; with a
/// persona prompt it answers HTTP 200 with an in-character dodge whose wording
/// varies every time ("I'm a bot with a hosting bill, not a political
/// scientist..."), so substring matching can't catch it. One yes/no call to a
/// no-thinking model can. Any judge error counts as "not a refusal" — the
/// check must never block a reply. `prompt` overrides the judge's system
/// prompt; it must still ask for a one-word yes/no answer.
pub async fn is_soft_refusal(
	judge: &dyn AiProvider,
	client: &reqwest::Client,
	prompt: Option<&str>,
	user_text: &str,
	reply: &str,
) -> bool {
	let messages = refusal_judge_messages(prompt, user_text, reply);
	match complete_dispatch(judge, client, &messages, false, 10).await {
		Ok(r) => judge_says_yes(r.content.as_deref()),
		Err(e) => {
			tracing::warn!("Refusal check via {} failed: {e}", judge.name());
			false
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn judge_answer_parsing() {
		assert!(judge_says_yes(Some("yes")));
		assert!(judge_says_yes(Some("Yes.")));
		assert!(judge_says_yes(Some("  **YES**")));
		assert!(!judge_says_yes(Some("no")));
		assert!(!judge_says_yes(Some("No, it engages.")));
		assert!(!judge_says_yes(Some("")));
		assert!(!judge_says_yes(None));
	}

	#[test]
	fn judge_messages_carry_both_sides() {
		let m = refusal_judge_messages(None, "what happened in 1989?", "I'd rather not.");
		assert_eq!(m.len(), 2);
		assert_eq!(m[0]["role"], "system");
		assert_eq!(m[0]["content"], REFUSAL_JUDGE_PROMPT);
		let custom = refusal_judge_messages(Some("custom judge"), "q", "a");
		assert_eq!(custom[0]["content"], "custom judge");
		let user = m[1]["content"].as_str().unwrap();
		assert!(user.contains("USER MESSAGE:\nwhat happened in 1989?"));
		assert!(user.contains("ASSISTANT REPLY:\nI'd rather not."));
	}

	#[test]
	fn vision_only_filters_members() {
		#[derive(Debug)]
		struct P(&'static str, bool);
		impl AiProvider for P {
			fn name(&self) -> &str {
				self.0
			}
			fn url(&self) -> &str {
				""
			}
			fn model(&self) -> &str {
				""
			}
			fn api_key(&self) -> &str {
				""
			}
			fn supports_vision(&self) -> bool {
				self.1
			}
			fn max_tokens_limit(&self) -> u32 {
				1
			}
		}
		let a = P("text_only", false);
		let b = P("seeing", true);
		let c = Cascade::new(vec![&a, &b], true);
		let v = c.vision_only();
		assert_eq!(v.members.len(), 1);
		assert_eq!(v.members[0].name(), "seeing");
		assert!(v.on_error);
		assert!(Cascade::default().members.is_empty());
	}
}
