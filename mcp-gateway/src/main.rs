mod backend;
mod config;
mod routing;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = config::GatewayConfig::from_env();
    tracing::info!("Gateway configured with {} instances", config.instances.len());
    for inst in &config.instances {
        tracing::info!("  {} -> {}", inst.name, inst.url);
    }
}
