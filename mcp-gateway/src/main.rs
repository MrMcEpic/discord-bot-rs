mod backend;
mod config;
mod routing;
mod server;

use axum::{middleware, Router};
use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = config::GatewayConfig::from_env();

    tracing::info!("MCP Gateway starting with {} instances", config.instances.len());
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
        }
    });

    let app = Router::new()
        .route("/mcp", axum::routing::post(server::mcp_handler))
        .route("/mcp", axum::routing::get(|| async { "MCP Gateway OK" }))
        .layer(middleware::from_fn_with_state(state.clone(), server::auth_middleware))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("MCP Gateway listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
