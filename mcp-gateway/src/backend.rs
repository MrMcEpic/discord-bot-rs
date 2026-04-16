use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone)]
pub struct BackendClient {
	pub name: String,
	pub base_url: String,
	http: Client,
	session_id: Option<String>,
	/// Bearer token forwarded on outgoing requests to the backend.
	///
	/// The gateway uses a single shared-secret model: the same
	/// `MCP_AUTH_TOKEN` value that `auth_middleware` verifies on
	/// *incoming* requests is forwarded on *outgoing* requests to each
	/// backend. Backends bind `0.0.0.0:9090` on the Docker network so
	/// the gateway sidecar can reach them, which means the bot-side
	/// Tier 1.1 guard forces them to require a token; without this
	/// field the gateway would always get 401.
	pub auth_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
	jsonrpc: &'static str,
	id: Option<u64>,
	method: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	params: Option<Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JsonRpcResponse {
	#[allow(dead_code)]
	pub jsonrpc: Option<String>,
	#[allow(dead_code)]
	pub id: Option<u64>,
	pub result: Option<Value>,
	pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JsonRpcError {
	pub code: i64,
	pub message: String,
	#[allow(dead_code)]
	pub data: Option<Value>,
}

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
	REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// Try to extract a JSON-RPC response from an SSE buffer.
/// Returns Some(response) if found, None if not yet available.
fn try_parse_sse_json(buffer: &str) -> Option<JsonRpcResponse> {
	for line in buffer.lines() {
		let line = line.trim();
		if let Some(data) = line.strip_prefix("data:") {
			let data = data.trim();
			if data.starts_with('{') {
				if let Ok(parsed) = serde_json::from_str::<JsonRpcResponse>(data) {
					return Some(parsed);
				}
			}
		}
	}
	None
}

impl BackendClient {
	pub fn new(name: String, base_url: String) -> Self {
		let http = Client::builder()
			.build()
			.expect("Failed to build HTTP client");
		Self {
			name,
			base_url,
			http,
			session_id: None,
			auth_token: None,
		}
	}

	/// Attach a bearer token that will be forwarded on every outgoing
	/// request to the backend. See the field doc on `auth_token` for
	/// why the gateway forwards its own inbound token.
	pub fn with_auth_token(mut self, auth_token: Option<String>) -> Self {
		self.auth_token = auth_token;
		self
	}

	/// Initialize MCP session and keep SSE stream alive for receiving responses
	pub async fn initialize(&mut self) -> Result<(), String> {
		let init_id = next_id();
		let req = JsonRpcRequest {
			jsonrpc: "2.0",
			id: Some(init_id),
			method: "initialize".to_string(),
			params: Some(serde_json::json!({
				"protocolVersion": "2025-03-26",
				"capabilities": {},
				"clientInfo": {
					"name": "mcp-gateway",
					"version": "0.1.0"
				}
			})),
		};

		let mut init_req = self
			.http
			.post(format!("{}/mcp", self.base_url))
			.header("Content-Type", "application/json")
			.header("Accept", "application/json, text/event-stream")
			.json(&req);
		if let Some(ref token) = self.auth_token {
			init_req = init_req.header("Authorization", format!("Bearer {}", token));
		}
		let resp = init_req
			.send()
			.await
			.map_err(|e| format!("Failed to connect to {}: {}", self.name, e))?;

		let status = resp.status();
		let session_header = resp
			.headers()
			.get("mcp-session-id")
			.and_then(|v| v.to_str().ok())
			.map(String::from);

		if !status.is_success() {
			let body = resp.text().await.unwrap_or_default();
			return Err(format!("{} returned HTTP {}: {}", self.name, status, body));
		}

		if let Some(sid) = session_header {
			self.session_id = Some(sid);
		}

		let session_id = self
			.session_id
			.clone()
			.ok_or_else(|| format!("{}: no session ID received", self.name))?;

		// Read SSE stream to find the initialize response
		let mut stream = resp.bytes_stream();
		let mut buffer = String::new();

		let init_result: JsonRpcResponse = loop {
			match tokio::time::timeout(std::time::Duration::from_secs(10), stream.next()).await {
				Ok(Some(Ok(chunk))) => {
					buffer.push_str(&String::from_utf8_lossy(&chunk));
					if let Some(resp) = try_parse_sse_json(&buffer) {
						break resp;
					}
				}
				Ok(Some(Err(e))) => {
					return Err(format!("{}: stream error during init: {}", self.name, e));
				}
				Ok(None) => {
					return Err(format!("{}: stream ended during init", self.name));
				}
				Err(_) => {
					return Err(format!(
						"{}: timeout waiting for init response. Buffer: {}",
						self.name,
						&buffer[..buffer.len().min(200)]
					));
				}
			}
		};

		if let Some(err) = init_result.error {
			return Err(format!("{} init error: {}", self.name, err.message));
		}

		tracing::info!("{}: initialized, session={}", self.name, session_id);

		// Send initialized notification (fire and forget)
		let mut notify_req = self
			.http
			.post(format!("{}/mcp", self.base_url))
			.header("Content-Type", "application/json")
			.header("Accept", "application/json, text/event-stream")
			.header("Mcp-Session-Id", &session_id)
			.json(&JsonRpcRequest {
				jsonrpc: "2.0",
				id: None,
				method: "notifications/initialized".to_string(),
				params: None,
			});
		if let Some(ref token) = self.auth_token {
			notify_req = notify_req.header("Authorization", format!("Bearer {}", token));
		}
		let _ = notify_req.send().await;

		Ok(())
	}

	/// Send a JSON-RPC request. Response comes on the POST's own SSE stream.
	pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, String> {
		let id = next_id();
		let req = JsonRpcRequest {
			jsonrpc: "2.0",
			id: Some(id),
			method: method.to_string(),
			params,
		};

		let session_id = self
			.session_id
			.as_ref()
			.ok_or_else(|| format!("{}: not initialized", self.name))?;

		// Send POST request
		let mut call_req = self
			.http
			.post(format!("{}/mcp", self.base_url))
			.header("Content-Type", "application/json")
			.header("Accept", "application/json, text/event-stream")
			.header("Mcp-Session-Id", session_id)
			.json(&req);
		if let Some(ref token) = self.auth_token {
			call_req = call_req.header("Authorization", format!("Bearer {}", token));
		}
		let resp = call_req
			.send()
			.await
			.map_err(|e| format!("Request to {} failed: {}", self.name, e))?;

		let status = resp.status();
		if !status.is_success() {
			let body = resp.text().await.unwrap_or_default();
			return Err(format!("HTTP {} from {}: {}", status, self.name, body));
		}

		// Read the response from this POST's SSE stream
		let mut stream = resp.bytes_stream();
		let mut buffer = String::new();

		loop {
			match tokio::time::timeout(std::time::Duration::from_secs(15), stream.next()).await {
				Ok(Some(Ok(chunk))) => {
					buffer.push_str(&String::from_utf8_lossy(&chunk));
					if let Some(parsed) = try_parse_sse_json(&buffer) {
						if let Some(err) = parsed.error {
							return Err(format!(
								"Backend error from {}: {}",
								self.name, err.message
							));
						}
						return parsed
							.result
							.ok_or_else(|| format!("No result from {}", self.name));
					}
				}
				Ok(Some(Err(e))) => {
					return Err(format!("{}: stream error: {}", self.name, e));
				}
				Ok(None) => {
					return Err(format!(
						"{}: stream ended without response. Buffer: {}",
						self.name,
						&buffer[..buffer.len().min(200)]
					));
				}
				Err(_) => {
					return Err(format!(
						"{}: request timed out after 15s. Buffer: {}",
						self.name,
						&buffer[..buffer.len().min(200)]
					));
				}
			}
		}
	}

	pub async fn list_tools(&self) -> Result<Value, String> {
		self.call("tools/list", Some(serde_json::json!({}))).await
	}

	pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, String> {
		self.call(
			"tools/call",
			Some(serde_json::json!({
				"name": name,
				"arguments": arguments
			})),
		)
		.await
	}

	pub async fn list_guilds(&self) -> Result<Vec<String>, String> {
		let result = self.call_tool("list_guilds", serde_json::json!({})).await?;
		let mut guild_ids = Vec::new();
		if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
			for item in content {
				if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
					for line in text.lines() {
						if let Some(id_part) = line.split("ID: ").nth(1) {
							guild_ids.push(id_part.trim().to_string());
						}
					}
				}
			}
		}
		Ok(guild_ids)
	}

	pub async fn health_check(&self) -> bool {
		self.list_tools().await.is_ok()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn try_parse_sse_finds_json() {
		let buffer = "data: \nid: 0\nretry: 3000\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
		let result = try_parse_sse_json(buffer);
		assert!(result.is_some());
		assert!(result.unwrap().result.is_some());
	}

	#[test]
	fn try_parse_sse_skips_empty_data() {
		let buffer = "data: \nid: 0\nretry: 3000\n\n";
		let result = try_parse_sse_json(buffer);
		assert!(result.is_none());
	}

	#[test]
	fn try_parse_sse_error() {
		let buffer =
			"data: {\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-1,\"message\":\"fail\"}}\n\n";
		let result = try_parse_sse_json(buffer);
		assert!(result.is_some());
		assert_eq!(result.unwrap().error.unwrap().message, "fail");
	}

	#[test]
	fn new_client_has_no_auth_token() {
		let client = BackendClient::new("bot1".to_string(), "http://bot1:9090".to_string());
		assert_eq!(client.name, "bot1");
		assert_eq!(client.base_url, "http://bot1:9090");
		assert!(client.auth_token.is_none());
	}

	#[test]
	fn with_auth_token_sets_field() {
		let client = BackendClient::new("bot1".to_string(), "http://bot1:9090".to_string())
			.with_auth_token(Some("s3cret".to_string()));
		assert_eq!(client.auth_token.as_deref(), Some("s3cret"));
	}

	#[test]
	fn with_auth_token_accepts_none() {
		let client = BackendClient::new("bot1".to_string(), "http://bot1:9090".to_string())
			.with_auth_token(Some("s3cret".to_string()))
			.with_auth_token(None);
		assert!(client.auth_token.is_none());
	}
}
