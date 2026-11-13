//! Native Anthropic `/v1/messages` dispatcher + OpenAI↔Anthropic translation.

use super::{AiProvider, ApiResponse, ToolCall};
use crate::ai::dsml::parse_dsml;
use crate::ai::tools::tool_definitions;

/// Parse a `data:MEDIA_TYPE;base64,DATA` URL into `(media_type, data)`.
///
/// Returns `Err` on any input that isn't a base64 data URL with a media
/// type. Used by `complete_anthropic` to translate OpenAI-shape image
/// content parts (which are always base64 data URLs) into Anthropic's
/// `image.source` block shape.
pub(super) fn parse_data_url(s: &str) -> Result<(String, String), String> {
	let rest = s
		.strip_prefix("data:")
		.ok_or_else(|| format!("not a data URL: {s}"))?;
	let (media_type, data) = rest
		.split_once(";base64,")
		.ok_or_else(|| format!("data URL missing ';base64,' separator: {s}"))?;
	if media_type.is_empty() {
		return Err(format!("data URL has empty media type: {s}"));
	}
	Ok((media_type.to_string(), data.to_string()))
}

/// Translate OpenAI-shape `messages` into `(system_prompt, anthropic_messages)`.
///
/// Extracts the first `role: "system"` message's content as the system prompt
/// and omits it from the returned `messages` array. Transforms tool-result
/// messages (`role: "tool"`) into Anthropic's user-content `tool_result`
/// blocks. Translates `image_url` content parts (OpenAI shape) into
/// Anthropic's `image.source.base64` shape. User/assistant text content
/// passes through unchanged.
///
/// Returns `Err` on malformed input (e.g. non-data-URL image_url).
pub(super) fn translate_messages_to_anthropic(
	messages: &[serde_json::Value],
) -> Result<(Option<String>, Vec<serde_json::Value>), String> {
	let mut system: Option<String> = None;
	let mut out: Vec<serde_json::Value> = Vec::with_capacity(messages.len());

	for msg in messages {
		let role = msg["role"].as_str().unwrap_or("");
		match role {
			"system" => {
				if system.is_none() {
					system = msg["content"].as_str().map(|s| s.to_string());
				}
				// Subsequent system messages are dropped — Anthropic only
				// accepts a single top-level system prompt. Callers that need
				// multiple would need to concatenate themselves.
			}
			"tool" => {
				let tool_call_id = msg["tool_call_id"].as_str().unwrap_or("");
				let content = msg["content"].as_str().unwrap_or("");
				out.push(serde_json::json!({
					"role": "user",
					"content": [{
						"type": "tool_result",
						"tool_use_id": tool_call_id,
						"content": content,
					}],
				}));
			}
			"user" | "assistant" => {
				let translated = translate_message_content(msg)?;
				out.push(translated);
			}
			other => {
				return Err(format!(
					"translate_messages_to_anthropic: unexpected role '{other}'"
				));
			}
		}
	}

	Ok((system, out))
}

/// Translate a single `user`/`assistant` message. The `content` field may be
/// a string (pass-through) or an array of content parts (translate each).
pub(super) fn translate_message_content(
	msg: &serde_json::Value,
) -> Result<serde_json::Value, String> {
	let role = msg["role"].as_str().unwrap_or("");
	let content = &msg["content"];

	if content.is_string() {
		return Ok(serde_json::json!({
			"role": role,
			"content": content.clone(),
		}));
	}

	let parts = content
		.as_array()
		.ok_or_else(|| format!("message content is neither string nor array: {content:?}"))?;

	let mut translated_parts: Vec<serde_json::Value> = Vec::with_capacity(parts.len());
	for part in parts {
		let part_type = part["type"].as_str().unwrap_or("");
		match part_type {
			"text" => {
				translated_parts.push(part.clone());
			}
			"image_url" => {
				let url = part["image_url"]["url"].as_str().unwrap_or("");
				let (media_type, data) = parse_data_url(url)?;
				translated_parts.push(serde_json::json!({
					"type": "image",
					"source": {
						"type": "base64",
						"media_type": media_type,
						"data": data,
					},
				}));
			}
			other => {
				return Err(format!(
					"translate_message_content: unexpected content part type '{other}'"
				));
			}
		}
	}

	Ok(serde_json::json!({
		"role": role,
		"content": translated_parts,
	}))
}

/// Translate OpenAI-shape tool definitions to Anthropic's flatter shape.
///
/// OpenAI: `{"type": "function", "function": {"name", "description", "parameters"}}`
/// Anthropic: `{"name", "description", "input_schema"}`
///
/// Anything without a `function` sub-object is left as-is (defensive — we
/// expect the input always to be OpenAI-shape today).
pub(super) fn translate_tool_defs_to_anthropic(
	openai_defs: &[serde_json::Value],
) -> Vec<serde_json::Value> {
	openai_defs
		.iter()
		.map(|def| {
			let function = &def["function"];
			if function.is_null() {
				return def.clone();
			}
			serde_json::json!({
				"name": function["name"].clone(),
				"description": function["description"].clone(),
				"input_schema": function["parameters"].clone(),
			})
		})
		.collect()
}

/// Parse an Anthropic `/v1/messages` response JSON body into the uniform
/// `ApiResponse` shape used by `chat.rs`.
///
/// Concatenates all `content[i].text` blocks into the returned `content`
/// string. Flattens `tool_use` content blocks into `ToolCall { id, name,
/// arguments }` where `arguments` is the stringified JSON of the structured
/// `input` object (matching the shape `chat.rs` expects from the OpenAI
/// path). DSML-embedded tool calls in the text content are also extracted
/// via `parse_dsml`, and the DSML text is stripped from the returned
/// content.
pub(super) fn parse_anthropic_response(body: &serde_json::Value) -> Result<ApiResponse, String> {
	let content_blocks = body["content"]
		.as_array()
		.ok_or_else(|| format!("Anthropic response missing 'content' array: {body}"))?;

	let mut text = String::new();
	let mut tool_calls: Vec<ToolCall> = Vec::new();

	for block in content_blocks {
		let block_type = block["type"].as_str().unwrap_or("");
		match block_type {
			"text" => {
				if let Some(t) = block["text"].as_str() {
					text.push_str(t);
				}
			}
			"tool_use" => {
				let id = block["id"].as_str().unwrap_or("").to_string();
				let name = block["name"].as_str().unwrap_or("").to_string();
				let arguments = serde_json::to_string(&block["input"]).unwrap_or_default();
				tool_calls.push(ToolCall {
					id,
					name,
					arguments,
				});
			}
			_ => {
				// Unknown block types (future Anthropic additions) are ignored.
			}
		}
	}

	// Pull DSML-embedded tool calls out of the text content, same as the
	// OpenAI path in `complete`.
	let (dsml_calls, cleaned) = parse_dsml(&text);
	for dsml in dsml_calls {
		let args_json = serde_json::to_string(&dsml.arguments).unwrap_or_default();
		tool_calls.push(ToolCall {
			id: format!(
				"dsml_{}_{}",
				chrono::Utc::now().timestamp_millis(),
				rand::random::<u32>()
			),
			name: dsml.name,
			arguments: args_json,
		});
	}

	let final_content = if cleaned.trim().is_empty() {
		None
	} else {
		Some(cleaned)
	};

	Ok(ApiResponse {
		content: final_content,
		tool_calls,
	})
}

/// Send a chat-completions-equivalent request to an Anthropic-spec provider
/// (`POST /v1/messages`) and return the uniform `ApiResponse` shape.
///
/// Translates OpenAI-shape `messages` + tool definitions to Anthropic's
/// wire shape on input; translates `content` blocks + `tool_use` blocks
/// back into the flat `ApiResponse { content, tool_calls }` shape on
/// output. See `translate_messages_to_anthropic`,
/// `translate_tool_defs_to_anthropic`, and `parse_anthropic_response` for
/// per-dimension translation details.
///
/// Uses `provider.auth_header()` / `provider.auth_scheme()` / `provider.extra_headers()`
/// to configure auth + extra headers — Anthropic requires
/// `x-api-key: <key>` with no scheme + `anthropic-version: 2023-06-01`.
pub async fn complete_anthropic(
	provider: &dyn AiProvider,
	client: &reqwest::Client,
	messages: &[serde_json::Value],
	use_tools: bool,
	max_tokens: u32,
) -> Result<ApiResponse, String> {
	let clamped_tokens = max_tokens.min(provider.max_tokens_limit());

	let (system, translated_msgs) = translate_messages_to_anthropic(messages)?;

	let mut body = serde_json::json!({
		"model": provider.model(),
		"messages": translated_msgs,
		"max_tokens": clamped_tokens,
	});

	if let Some(sys) = system {
		body["system"] = serde_json::Value::String(sys);
	}

	if use_tools && provider.supports_tools() {
		let openai_tool_defs = tool_definitions();
		body["tools"] =
			serde_json::Value::Array(translate_tool_defs_to_anthropic(&openai_tool_defs));
	}
	super::openai::merge_extra_body(&mut body, provider.extra_body());

	// Build the request with provider-configured auth + headers.
	let auth_value = format!("{}{}", provider.auth_scheme(), provider.api_key());
	let mut req = client
		.post(provider.url())
		.header("Content-Type", "application/json")
		.header(provider.auth_header(), auth_value)
		.timeout(provider.timeout())
		.json(&body);

	for (k, v) in provider.extra_headers() {
		req = req.header(k.as_str(), v.as_str());
	}

	let response = req
		.send()
		.await
		.map_err(|e| format!("API request failed: {e}"))?;

	if !response.status().is_success() {
		let status = response.status();
		let err_body = response.text().await.unwrap_or_default();
		tracing::error!("{} API {status}: {err_body}", provider.model());
		if super::openai::is_censored_error(provider, &err_body) {
			return Err("CENSORED".to_string());
		}
		return Err(format!("API returned {status}"));
	}

	let data: serde_json::Value = response
		.json()
		.await
		.map_err(|e| format!("Failed to parse API response: {e}"))?;

	// Claude's safety stop: no usable answer, cascade like any other filter.
	if data["stop_reason"].as_str() == Some("refusal") {
		tracing::info!("{} stopped with stop_reason=refusal", provider.name());
		return Err("CENSORED".to_string());
	}

	parse_anthropic_response(&data)
}
