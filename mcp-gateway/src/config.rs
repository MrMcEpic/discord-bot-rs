use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Instance {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub port: u16,
    pub instances: Vec<Instance>,
    pub auth_token: Option<String>,
}

impl GatewayConfig {
    pub fn from_env() -> Self {
        let port: u16 = std::env::var("GATEWAY_PORT")
            .unwrap_or_else(|_| "9100".to_string())
            .parse()
            .expect("GATEWAY_PORT must be a valid port number");

        let instances_str = std::env::var("INSTANCES")
            .expect("INSTANCES env var required (e.g., examplebot=http://examplebot:9090,secondbot=http://secondbot:9090)");

        let instances: Vec<Instance> = instances_str
            .split(',')
            .map(|entry| {
                let (name, url) = entry.trim().split_once('=')
                    .expect("Each instance must be name=url format");
                Instance {
                    name: name.to_string(),
                    url: url.to_string(),
                }
            })
            .collect();

        let auth_token = std::env::var("MCP_AUTH_TOKEN").ok()
            .filter(|t| !t.is_empty());

        GatewayConfig { port, instances, auth_token }
    }

    pub fn instance_map(&self) -> HashMap<String, String> {
        self.instances.iter()
            .map(|i| (i.name.clone(), i.url.clone()))
            .collect()
    }
}
