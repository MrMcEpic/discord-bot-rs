use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

#[derive(Clone)]
pub struct BackendClient {
    pub name: String,
    pub base_url: String,
    http: Client,
    session_id: Option<String>,
    /// Pending response waiters: request_id -> oneshot sender
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
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
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
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

        let resp = self.http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&req)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to {}: {}", self.name, e))?;

        let status = resp.status();
        let session_header = resp.headers().get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("{} returned HTTP {}: {}", self.name, status, body));
        }

        if let Some(sid) = session_header {
            self.session_id = Some(sid);
        }

        let session_id = self.session_id.clone()
            .ok_or_else(|| format!("{}: no session ID received", self.name))?;

        // Read SSE stream: first find the initialize result, then keep reading for future responses
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut init_result: Option<JsonRpcResponse> = None;

        // Phase 1: Read until we find the initialize response
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                stream.next(),
            ).await {
                Ok(Some(Ok(chunk))) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    if let Some(resp) = try_parse_sse_json(&buffer) {
                        init_result = Some(resp);
                        buffer.clear();
                        break;
                    }
                }
                Ok(Some(Err(e))) => {
                    return Err(format!("{}: stream error during init: {}", self.name, e));
                }
                Ok(None) => {
                    return Err(format!("{}: stream ended during init", self.name));
                }
                Err(_) => {
                    return Err(format!("{}: timeout waiting for init response. Buffer: {}",
                        self.name, &buffer[..buffer.len().min(200)]));
                }
            }
        }

        let init_result = init_result.unwrap();
        if let Some(err) = init_result.error {
            return Err(format!("{} init error: {}", self.name, err.message));
        }

        tracing::info!("{}: initialized, session={}", self.name, session_id);

        // Send initialized notification (fire and forget)
        let _ = self.http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Mcp-Session-Id", &session_id)
            .json(&JsonRpcRequest {
                jsonrpc: "2.0",
                id: None,
                method: "notifications/initialized".to_string(),
                params: None,
            })
            .send()
            .await;

        // Phase 2: Keep the SSE stream alive in background, dispatching responses to pending waiters
        let pending = self.pending.clone();
        let name = self.name.clone();
        tokio::spawn(async move {
            // Process any leftover data in buffer
            Self::dispatch_responses(&buffer, &pending).await;
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        Self::dispatch_responses(&buffer, &pending).await;
                        // Clear processed events (everything before the last incomplete event)
                        if let Some(pos) = buffer.rfind("\n\n") {
                            buffer = buffer[pos + 2..].to_string();
                        }
                    }
                    Err(e) => {
                        tracing::warn!("{}: SSE stream error: {}", name, e);
                        break;
                    }
                }
            }
            tracing::warn!("{}: SSE stream ended", name);
        });

        Ok(())
    }

    /// Parse SSE buffer for JSON-RPC responses and dispatch to pending waiters
    async fn dispatch_responses(buffer: &str, pending: &Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>) {
        for line in buffer.lines() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.starts_with('{') {
                    if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(data) {
                        if let Some(id) = resp.id {
                            let mut pending = pending.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                let _ = tx.send(resp);
                            }
                        }
                    }
                }
            }
        }
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

        let session_id = self.session_id.as_ref()
            .ok_or_else(|| format!("{}: not initialized", self.name))?;

        // Send POST request
        let resp = self.http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Mcp-Session-Id", session_id)
            .json(&req)
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
            match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                stream.next(),
            ).await {
                Ok(Some(Ok(chunk))) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    if let Some(parsed) = try_parse_sse_json(&buffer) {
                        if let Some(err) = parsed.error {
                            return Err(format!("Backend error from {}: {}", self.name, err.message));
                        }
                        return parsed.result.ok_or_else(|| format!("No result from {}", self.name));
                    }
                }
                Ok(Some(Err(e))) => {
                    return Err(format!("{}: stream error: {}", self.name, e));
                }
                Ok(None) => {
                    return Err(format!("{}: stream ended without response. Buffer: {}",
                        self.name, &buffer[..buffer.len().min(200)]));
                }
                Err(_) => {
                    return Err(format!("{}: request timed out after 15s. Buffer: {}",
                        self.name, &buffer[..buffer.len().min(200)]));
                }
            }
        }
    }

    pub async fn list_tools(&self) -> Result<Value, String> {
        self.call("tools/list", Some(serde_json::json!({}))).await
    }

    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, String> {
        self.call("tools/call", Some(serde_json::json!({
            "name": name,
            "arguments": arguments
        }))).await
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
        let buffer = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-1,\"message\":\"fail\"}}\n\n";
        let result = try_parse_sse_json(buffer);
        assert!(result.is_some());
        assert_eq!(result.unwrap().error.unwrap().message, "fail");
    }
}
