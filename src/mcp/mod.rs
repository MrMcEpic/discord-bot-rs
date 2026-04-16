pub mod tools;

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
) {
	// Security gate: refuse to start when auth is disabled on a non-loopback bind.
	// Without this, a default `MCP_BIND_ADDR=0.0.0.0` plus an unset `MCP_AUTH_TOKEN`
	// would expose every Discord MCP tool (ban, delete-channel, send-message, ...)
	// to the entire network with zero credentials.
	if auth_token.is_empty() {
		if !is_loopback_addr(&bind_addr) {
			panic!(
				"Refusing to start MCP server: MCP_AUTH_TOKEN is empty but MCP_BIND_ADDR={bind_addr} is not loopback. \
				 This would expose destructive Discord tools (ban, delete-channel, send-message, ...) to the network with no auth. \
				 Either set MCP_AUTH_TOKEN to a strong secret, or set MCP_BIND_ADDR=127.0.0.1 (loopback only)."
			);
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
			if auth == format!("Bearer {}", expected) {
				next.run(req).await
			} else {
				(StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
			}
		}
	});

	let app = Router::new()
		.nest_service("/mcp", mcp_service)
		.layer(auth_middleware);

	let app = if let Some(webhook) = webhook_router {
		app.merge(webhook)
	} else {
		app
	};

	let addr = format!("{}:{}", bind_addr, port);
	let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
	tracing::info!("MCP server listening on {}", addr);

	axum::serve(listener, app).await.unwrap();
}
