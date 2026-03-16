use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub struct BackendClient {
    pub name: String,
    pub base_url: String,
    http: Client,
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: Option<u64>,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
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

/// Parse a JSON-RPC response from either plain JSON or SSE event stream.
fn parse_response(content_type: &str, body: &str) -> Result<JsonRpcResponse, String> {
    if content_type.contains("text/event-stream") {
        for line in body.lines().rev() {
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.starts_with('{') {
                    return serde_json::from_str(data)
                        .map_err(|e| format!("Failed to parse SSE JSON: {}", e));
                }
            }
        }
        Err("No JSON-RPC response found in SSE stream".to_string())
    } else {
        serde_json::from_str(body)
            .map_err(|e| format!("Failed to parse JSON response: {}", e))
    }
}

impl BackendClient {
    pub fn new(name: String, base_url: String) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");
        Self { name, base_url, http, session_id: None }
    }

    pub async fn initialize(&mut self) -> Result<(), String> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: Some(next_id()),
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

        if let Some(sid) = resp.headers().get("mcp-session-id") {
            self.session_id = Some(sid.to_str().unwrap_or("").to_string());
        }

        let content_type = resp.headers().get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body_text = resp.text().await
            .map_err(|e| format!("Failed to read response from {}: {}", self.name, e))?;

        let parsed = parse_response(&content_type, &body_text)?;
        if let Some(err) = parsed.error {
            return Err(format!("Backend {} returned error: {}", self.name, err.message));
        }

        // Send initialized notification
        let notif = JsonRpcRequest {
            jsonrpc: "2.0",
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };

        let mut req_builder = self.http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .json(&notif);

        if let Some(ref sid) = self.session_id {
            req_builder = req_builder.header("Mcp-Session-Id", sid);
        }

        let _ = req_builder.send().await;

        tracing::info!("Initialized MCP session with {} (session: {:?})", self.name, self.session_id);
        Ok(())
    }

    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: Some(next_id()),
            method: method.to_string(),
            params,
        };

        let mut req_builder = self.http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&req);

        if let Some(ref sid) = self.session_id {
            req_builder = req_builder.header("Mcp-Session-Id", sid);
        }

        let resp = req_builder.send().await
            .map_err(|e| format!("Request to {} failed: {}", self.name, e))?;

        let content_type = resp.headers().get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body_text = resp.text().await
            .map_err(|e| format!("Failed to read response from {}: {}", self.name, e))?;

        let parsed = parse_response(&content_type, &body_text)?;
        if let Some(err) = parsed.error {
            return Err(format!("Backend error: {}", err.message));
        }

        parsed.result.ok_or_else(|| format!("No result from {}", self.name))
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
    fn parse_sse_response() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
        let parsed = parse_response("text/event-stream", body).unwrap();
        assert!(parsed.result.is_some());
        assert!(parsed.error.is_none());
    }

    #[test]
    fn parse_json_response() {
        let body = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}";
        let parsed = parse_response("application/json", body).unwrap();
        assert!(parsed.result.is_some());
    }

    #[test]
    fn parse_sse_error_response() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-1,\"message\":\"fail\"}}\n\n";
        let parsed = parse_response("text/event-stream", body).unwrap();
        assert!(parsed.error.is_some());
        assert_eq!(parsed.error.unwrap().message, "fail");
    }
}
