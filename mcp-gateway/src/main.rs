mod backend;
mod config;
mod routing;
mod server;

use axum::{middleware, Router};
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt::init();
	let config = config::GatewayConfig::from_env();

	// Security gate: the gateway always binds 0.0.0.0:GATEWAY_PORT so sibling
	// containers can reach it, so there's no loopback escape hatch like the
	// bot-side MCP server has. Refuse to start without a token -- otherwise
	// every backend's destructive Discord tools would be reachable across the
	// Docker network with zero credentials.
	if config.auth_token.is_none() {
		panic!(
			"Refusing to start gateway: MCP_AUTH_TOKEN is empty/unset. The gateway \
			 is reachable across the Docker network and will expose every backend's \
			 22 destructive Discord tools (ban, delete-channel, send-message, ...) \
			 to anyone with network reach. Set MCP_AUTH_TOKEN to a strong secret."
		);
	}

	tracing::info!(
		"MCP Gateway starting with {} instances",
		config.instances.len()
	);
	for inst in &config.instances {
		tracing::info!("  {} -> {}", inst.name, inst.url);
	}

	let state = server::GatewayState::new(config.instance_map(), config.auth_token.clone());

	if let Err(e) = state.initialize_backends().await {
		tracing::error!("Failed to initialize backends: {}. Starting anyway.", e);
	}

	let refresh_state = state.clone();
	tokio::spawn(async move {
		loop {
			tokio::time::sleep(Duration::from_secs(300)).await;
			refresh_state.refresh_guild_map().await;
			// Also re-fetch the tool list — without this, any new tool
			// added to a backend bot stays invisible to clients until
			// the gateway itself is restarted, even though the bot
			// already serves it correctly.
			if let Err(e) = refresh_state.refresh_tool_list().await {
				tracing::warn!("Periodic refresh_tool_list failed: {}", e);
			}
		}
	});

	let app = Router::new()
		.route("/mcp", axum::routing::post(server::mcp_handler))
		.route("/mcp", axum::routing::get(|| async { "MCP Gateway OK" }))
		// 64 KiB cap on request bodies. JSON-RPC envelopes are tiny; this prevents
		// authenticated callers from saturating the gateway with multi-MiB bodies
		// in tight loops (backend state lives behind tokio Mutex/RwLock).
		.layer(RequestBodyLimitLayer::new(64 * 1024))
		.layer(middleware::from_fn_with_state(
			state.clone(),
			server::auth_middleware,
		))
		.with_state(state);

	let addr = format!("0.0.0.0:{}", config.port);
	tracing::info!("MCP Gateway listening on {}", addr);
	let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
	axum::serve(listener, app).await.unwrap();
}
