pub mod tools;

use crate::error::BotError;
use rmcp::transport::streamable_http_server::{
	session::local::LocalSessionManager, tower::StreamableHttpService, StreamableHttpServerConfig,
};
use serenity::all::*;
use std::sync::Arc;

fn is_loopback_addr(addr: &str) -> bool {
	matches!(addr, "127.0.0.1" | "::1" | "localhost")
}

pub async fn start(
	http: Arc<Http>,
	guild_id: GuildId,
	port: u16,
	bind_addr: String,
	auth_token: String,
	webhook_router: Option<axum::Router>,
) -> Result<(), BotError> {
	// Security gate: refuse to start when auth is disabled on a non-loopback bind.
	// Without this, a default `MCP_BIND_ADDR=0.0.0.0` plus an unset `MCP_AUTH_TOKEN`
	// would expose every Discord MCP tool (ban, delete-channel, send-message, ...)
	// to the entire network with zero credentials.
	if auth_token.is_empty() {
		if !is_loopback_addr(&bind_addr) {
			return Err(BotError::Other(format!(
				"Refusing to start MCP server: MCP_AUTH_TOKEN is empty but MCP_BIND_ADDR={bind_addr} is not loopback. \
				 This would expose destructive Discord tools (ban, delete-channel, send-message, ...) to the network with no auth. \
				 Either set MCP_AUTH_TOKEN to a strong secret, or set MCP_BIND_ADDR=127.0.0.1 (loopback only)."
			)));
		}
		tracing::warn!(
			"MCP server starting without authentication (loopback-only bind {}). \
			 Set MCP_AUTH_TOKEN to require Bearer auth.",
			bind_addr
		);
	}

	let session_manager = Arc::new(LocalSessionManager::default());
	let config = StreamableHttpServerConfig::default();

	let http_clone = http.clone();
	let mcp_service = StreamableHttpService::new(
		move || Ok(tools::DiscordTools::new(http_clone.clone(), guild_id)),
		session_manager,
		config,
	);

	// Wrap with auth check using axum
	use axum::{
		extract::Request,
		middleware::{self, Next},
		response::IntoResponse,
		Router,
	};
	use http::StatusCode;
	use subtle::ConstantTimeEq;
	use tower_http::limit::RequestBodyLimitLayer;

	let token = auth_token.clone();
	let auth_middleware = middleware::from_fn(move |req: Request, next: Next| {
		let expected = token.clone();
		async move {
			if expected.is_empty() {
				return next.run(req).await;
			}
			let auth = req
				.headers()
				.get("authorization")
				.and_then(|v| v.to_str().ok())
				.unwrap_or("");
			let expected_header = format!("Bearer {}", expected);
			let provided_bytes = auth.as_bytes();
			let expected_bytes = expected_header.as_bytes();
			let lengths_match = provided_bytes.len() == expected_bytes.len();
			let bytes_match: bool =
				lengths_match && bool::from(provided_bytes.ct_eq(expected_bytes));
			if bytes_match {
				next.run(req).await
			} else {
				(StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
			}
		}
	});

	// 64 KiB cap on request bodies. JSON-RPC envelopes are tiny; this is generous
	// while preventing authenticated callers from DoS'ing the shared backend
	// state (tokio Mutex/RwLock<HashMap>) with multi-MiB bodies in tight loops.
	let app = Router::new()
		.nest_service("/mcp", mcp_service)
		.layer(RequestBodyLimitLayer::new(64 * 1024))
		.layer(auth_middleware);

	let app = if let Some(webhook) = webhook_router {
		app.merge(webhook)
	} else {
		app
	};

	let addr = format!("{}:{}", bind_addr, port);
	let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
		BotError::Other(format!(
			"MCP server failed to bind {addr}: {e}. Is the port already in use?"
		))
	})?;
	tracing::info!("MCP server listening on {}", addr);

	axum::serve(listener, app)
		.await
		.map_err(|e| BotError::Other(format!("MCP server crashed: {e}")))?;

	Ok(())
}
