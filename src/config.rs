use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    pub endpoints: Vec<EndpointConfig>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GlobalConfig {
    #[serde(default = "default_state_file")]
    pub state_file: String,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EndpointConfig {
    pub name: String,
    pub url: String,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    pub expected_status: Option<u16>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    pub webhook_url: Option<String>,
}

fn default_state_file() -> String {
    "state.json".to_string()
}

fn default_interval() -> u64 {
    30
}

fn default_timeout() -> u64 {
    10
}

pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read config file: {}", path.display()))?;
    let config: Config =
        toml::from_str(&raw).with_context(|| "Failed to parse config.toml")?;
    anyhow::ensure!(!config.endpoints.is_empty(), "No endpoints defined in config");
    Ok(config)
}
