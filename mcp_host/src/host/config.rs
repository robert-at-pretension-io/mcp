use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use anyhow::Result;

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    pub command: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AIProviderConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TimeoutConfig {
    #[serde(default = "default_request_timeout")]
    pub request: u64,
    #[serde(default = "default_tool_timeout")]
    pub tool: u64,
}

fn default_request_timeout() -> u64 {
    120
}

fn default_tool_timeout() -> u64 {
    300
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request: default_request_timeout(),
            tool: default_tool_timeout(),
        }
    }
}

impl Default for AIProviderConfig {
    fn default() -> Self {
        Self {
            provider: "deepseek".to_string(),
            model: "deepseek-chat".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(rename = "mcpServers")]
    pub servers: HashMap<String, ServerConfig>,
    
    #[serde(default)]
    pub ai_provider: AIProviderConfig,
    
    #[serde(default)]
    pub timeouts: TimeoutConfig,
}

impl Config {
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        
        // Ensure config directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        // Try to read existing config or create default
        let config_str = match fs::read_to_string(path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let default_config = Self::default();
                let default_str = serde_json::to_string_pretty(&default_config)?;
                fs::write(path, &default_str).await?;
                default_str
            }
            Err(e) => return Err(e.into()),
        };
        
        let config: Self = serde_json::from_str(&config_str)?;
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            servers: HashMap::new(),
            ai_provider: AIProviderConfig::default(),
            timeouts: TimeoutConfig::default(),
        }
    }
}