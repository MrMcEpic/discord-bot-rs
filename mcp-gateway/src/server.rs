use crate::backend::BackendClient;
use crate::routing::{RouteError, Router};
use axum::{
	body::Body,
	extract::State,
	http::{header, HeaderMap, StatusCode},
	response::{IntoResponse, Response},
	Json,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct GatewayState {
	pub router: Router,
	pub backends: Arc<RwLock<HashMap<String, BackendClient>>>,
	pub tool_list_cache: Arc<RwLock<Option<Value>>>,
	pub auth_token: Option<String>,
}

impl GatewayState {
	pub fn new(instances: HashMap<String, String>, auth_token: Option<String>) -> Self {
		let mut backends = HashMap::new();
		for (name, url) in &instances {
			backends.insert(name.clone(), BackendClient::new(name.clone(), url.clone()));
		}

		Self {
			router: Router::new(instances),
			backends: Arc::new(RwLock::new(backends)),
			tool_list_cache: Arc::new(RwLock::new(None)),
			auth_token,
		}
	}

	pub async fn initialize_backends(&self) -> Result<(), String> {
		let mut backends = self.backends.write().await;
		for (name, client) in backends.iter_mut() {
			let mut retries = 0;
			loop {
				match client.initialize().await {
					Ok(()) => break,
					Err(e) => {
						retries += 1;
						if retries >= 6 {
							return Err(format!(
								"Failed to initialize {} after {} retries: {}",
								name, retries, e
							));
						}
						tracing::warn!("Retrying {} initialization ({}/6): {}", name, retries, e);
						tokio::time::sleep(std::time::Duration::from_secs(10)).await;
					}
				}
			}
		}
		drop(backends);
		self.refresh_guild_map().await;
		self.refresh_tool_list().await?;
		Ok(())
	}

	pub async fn refresh_guild_map(&self) {
		// First, re-initialize any backends with stale sessions
		let stale: Vec<String> = {
			let backends = self.backends.read().await;
			let mut stale = Vec::new();
			for (name, client) in backends.iter() {
				if !client.health_check().await {
					stale.push(name.clone());
				}
			}
			stale
		};
		if !stale.is_empty() {
			let mut backends = self.backends.write().await;
			for name in &stale {
				if let Some(client) = backends.get_mut(name) {
					tracing::warn!("{}: health check failed, re-initializing...", name);
					if let Err(e) = client.initialize().await {
						tracing::error!("{}: re-initialization failed: {}", name, e);
					}
				}
			}
		}

		let backends = self.backends.read().await;
		for (name, client) in backends.iter() {
			match client.list_guilds().await {
				Ok(guild_ids) => {
					tracing::info!("{} serves {} guild(s)", name, guild_ids.len());
					self.router.update_guild_map(name, guild_ids).await;
				}
				Err(e) => {
					tracing::warn!("Failed to get guilds from {}: {}", name, e);
				}
			}
		}
	}

	async fn refresh_tool_list(&self) -> Result<(), String> {
		let backends = self.backends.read().await;
		let client = backends.values().next().ok_or("No backends configured")?;

		let tools_result = client.list_tools().await?;
		let mut tools = tools_result
			.get("tools")
			.and_then(|t| t.as_array())
			.cloned()
			.unwrap_or_default();

		for tool in tools.iter_mut() {
			if let Some(schema) = tool.get_mut("inputSchema") {
				if let Some(props) = schema.get_mut("properties") {
					if let Some(props_obj) = props.as_object_mut() {
						props_obj.insert("instance".to_string(), json!({
                            "type": "string",
                            "description": "Bot instance name to route to, matching a key in the INSTANCES env var (e.g., 'bot_a', 'bot_b'). If omitted, routes by guild_id."
                        }));
					}
				}
			}
		}

		tools.push(json!({
			"name": "list_instances",
			"description": "List all registered bot instances, their guilds, and health status",
			"inputSchema": {
				"type": "object",
				"properties": {},
				"required": []
			}
		}));

		let mut cache = self.tool_list_cache.write().await;
		*cache = Some(json!({ "tools": tools }));
		Ok(())
	}
}

/// Format a JSON-RPC response as SSE for Streamable HTTP MCP protocol
fn sse_response(json_rpc: Value, session_id: &str) -> Response {
	let sse_body = format!("event: message\ndata: {}\n\n", json_rpc);
	Response::builder()
		.status(StatusCode::OK)
		.header(header::CONTENT_TYPE, "text/event-stream")
		.header("Mcp-Session-Id", session_id)
		.header(header::CACHE_CONTROL, "no-cache")
		.body(Body::from(sse_body))
		.unwrap()
}

/// Auth middleware
pub async fn auth_middleware(
	State(state): State<GatewayState>,
	headers: HeaderMap,
	request: axum::extract::Request,
	next: axum::middleware::Next,
) -> impl IntoResponse {
	use subtle::ConstantTimeEq;

	if let Some(ref expected) = state.auth_token {
		let provided = headers
			.get("authorization")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.strip_prefix("Bearer "))
			.unwrap_or("");

		let provided_bytes = provided.as_bytes();
		let expected_bytes = expected.as_bytes();
		let lengths_match = provided_bytes.len() == expected_bytes.len();
		let bytes_match: bool = lengths_match && bool::from(provided_bytes.ct_eq(expected_bytes));
		if !bytes_match {
			return StatusCode::UNAUTHORIZED.into_response();
		}
	}
	next.run(request).await.into_response()
}

/// MCP HTTP handler -- handles POST /mcp
pub async fn mcp_handler(
	State(state): State<GatewayState>,
	Json(body): Json<Value>,
) -> impl IntoResponse {
	let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
	let id = body.get("id").cloned();
	let params = body.get("params").cloned();

	let session_id = "gateway-session";

	let result = match method {
		"initialize" => {
			json!({
				"protocolVersion": "2025-03-26",
				"capabilities": { "tools": {} },
				"serverInfo": {
					"name": "mcp-gateway",
					"version": "0.1.0"
				}
			})
		}

		"notifications/initialized" => {
			return StatusCode::ACCEPTED.into_response();
		}

		"tools/list" => {
			let cache = state.tool_list_cache.read().await;
			cache.clone().unwrap_or(json!({ "tools": [] }))
		}

		"tools/call" => {
			let tool_name = params
				.as_ref()
				.and_then(|p| p.get("name"))
				.and_then(|n| n.as_str())
				.unwrap_or("");

			let arguments = params
				.as_ref()
				.and_then(|p| p.get("arguments"))
				.cloned()
				.unwrap_or(json!({}));

			match handle_tool_call(&state, tool_name, arguments).await {
				Ok(result) => result,
				Err(e) => json!({
					"content": [{ "type": "text", "text": format!("Gateway error: {}", e) }],
					"isError": true
				}),
			}
		}

		_ => {
			return sse_response(
				json!({
					"jsonrpc": "2.0",
					"id": id,
					"error": { "code": -32601, "message": format!("Unknown method: {}", method) }
				}),
				session_id,
			)
			.into_response();
		}
	};

	sse_response(
		json!({
			"jsonrpc": "2.0",
			"id": id,
			"result": result
		}),
		session_id,
	)
	.into_response()
}

async fn handle_tool_call(
	state: &GatewayState,
	tool_name: &str,
	mut arguments: Value,
) -> Result<Value, String> {
	if tool_name == "list_instances" {
		return handle_list_instances(state).await;
	}

	let instance = arguments
		.as_object_mut()
		.and_then(|obj| obj.remove("instance"))
		.and_then(|v| v.as_str().map(String::from));

	let guild_id = arguments
		.get("guild_id")
		.and_then(|v| v.as_str())
		.map(String::from);

	let target = state
		.router
		.resolve(instance.as_deref(), guild_id.as_deref())
		.await
		.map_err(|e| match e {
			RouteError::NoTarget => {
				let names: Vec<String> = state.router.instances.keys().cloned().collect();
				format!(
					"No instance or guild_id specified. Available instances: {}",
					names.join(", ")
				)
			}
			other => other.to_string(),
		})?;

	let crate::routing::RouteTarget::Instance(name, _) = target;

	// Try the call, and if session is stale, re-initialize and retry once
	let args_for_retry = arguments.clone();

	let result = {
		let backends = state.backends.read().await;
		let client = backends
			.get(&name)
			.ok_or_else(|| format!("Backend '{}' not found", name))?;
		client.call_tool(tool_name, arguments).await
	};

	match result {
		Ok(val) => Ok(val),
		Err(e) if e.contains("Session not found") || e.contains("404 Not Found") => {
			tracing::warn!("{}: session expired, re-initializing...", name);
			{
				let mut backends = state.backends.write().await;
				if let Some(client) = backends.get_mut(&name) {
					if let Err(reinit_err) = client.initialize().await {
						return Err(format!("Re-init of {} failed: {}", name, reinit_err));
					}
				}
			}
			let backends = state.backends.read().await;
			let client = backends
				.get(&name)
				.ok_or_else(|| format!("Backend '{}' not found after re-init", name))?;
			client.call_tool(tool_name, args_for_retry).await
		}
		Err(e) => Err(e),
	}
}

async fn handle_list_instances(state: &GatewayState) -> Result<Value, String> {
	let backends = state.backends.read().await;
	let guild_map = state.router.guild_map.read().await;

	let mut lines = Vec::new();
	for (name, client) in backends.iter() {
		let healthy = client.health_check().await;
		let status = if healthy { "online" } else { "offline" };
		let guilds: Vec<&str> = guild_map
			.iter()
			.filter(|(_, v)| v.as_str() == name)
			.map(|(k, _)| k.as_str())
			.collect();
		lines.push(format!(
			"{} | {} | guilds: {}",
			name,
			status,
			if guilds.is_empty() {
				"none".to_string()
			} else {
				guilds.join(", ")
			}
		));
	}

	Ok(json!({
		"content": [{ "type": "text", "text": lines.join("\n") }]
	}))
}
