use std::env;

pub struct Config {
    pub token: String,
    pub client_id: String,
    pub guild_id: String,
    pub deepseek_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub finnhub_api_key: Option<String>,
    pub database_url: String,
}

impl Config {
    pub fn load() -> Self {
        dotenvy::dotenv().ok();

        Self {
            token: get_env_or_throw("DISCORD_TOKEN"),
            client_id: get_env_or_throw("CLIENT_ID"),
            guild_id: get_env_or_throw("GUILD_ID"),
            deepseek_api_key: env::var("DEEPSEEK_API_KEY").ok().filter(|s| !s.is_empty()),
            gemini_api_key: env::var("GEMINI_API_KEY").ok().filter(|s| !s.is_empty()),
            finnhub_api_key: env::var("FINNHUB_API_KEY").ok().filter(|s| !s.is_empty()),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgresql://discord_bot:discord_bot_pass@localhost:5432/discord_bot".to_string()
            }),
        }
    }
}

fn get_env_or_throw(key: &str) -> String {
    let val = env::var(key).unwrap_or_else(|_| panic!("{key} must be set in .env"));
    if val.starts_with("your-") {
        panic!("{key} has placeholder value — set it in .env");
    }
    val
}
