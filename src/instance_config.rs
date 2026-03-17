use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct InstanceConfig {
    pub bot_name: String,
    pub command_prefix: String,
    #[serde(default = "default_personality_file")]
    pub personality_file: String,
    #[serde(default)]
    pub features: Features,
    pub auto_role: Option<AutoRoleConfig>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Features {
    #[serde(default)]
    pub minecraft: bool,
    #[serde(default)]
    pub auto_role: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AutoRoleConfig {
    pub from_role: String,
    pub to_role: String,
    #[serde(default = "default_min_age")]
    pub min_age: String,
    #[serde(default = "default_min_messages")]
    pub min_messages: i64,
    #[serde(default)]
    pub require_all: bool,
}

fn default_min_age() -> String {
    "3d".to_string()
}

fn default_min_messages() -> i64 {
    20
}

fn default_personality_file() -> String {
    "personality.txt".to_string()
}

impl InstanceConfig {
    pub fn load(config_dir: &Path) -> Self {
        let config_path = config_dir.join("config.toml");
        let content = std::fs::read_to_string(&config_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", config_path.display()));
        toml::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", config_path.display()))
    }

    pub fn load_personality(&self, config_dir: &Path) -> String {
        let path = config_dir.join(&self.personality_file);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read personality file {}: {e}", path.display()));
        if content.trim().is_empty() {
            panic!("Personality file {} is empty", path.display());
        }
        content
    }

    pub fn config_dir() -> PathBuf {
        let dir = std::env::var("CONFIG_DIR").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(dir)
    }
}
