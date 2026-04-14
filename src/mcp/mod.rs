pub mod tools;

use rmcp::transport::streamable_http_server::{
	session::local::LocalSessionManager, tower::StreamableHttpService, StreamableHttpServerConfig,
};
use serenity::all::*;
use std::sync::Arc;

pub async fn start(
	http: Arc<Http>,
	guild_id: GuildId,
	port: u16,
	bind_addr: String,
	auth_token: String,
	webhook_router: Option<axum::Router>,
) {
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
		body::Body,
		extract::Request,
		middleware::{self, Next},
		response::{IntoResponse, Response},
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
