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

// Removed duplicate imports and struct definition below

#[derive(Debug, Deserialize, Serialize, Clone)] // Add Clone here
// Removed leftover struct field and closing brace below

#[derive(Debug, Deserialize, Serialize, Clone)] // Add Clone here
pub struct AIProviderConfig {
    // Removed provider field, key in the map will be the provider name
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
            // provider field removed
            model: "deepseek-chat".to_string(), // Default model
        }
        // Removed extra closing brace here
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(rename = "mcpServers")]
    pub servers: HashMap<String, ServerConfig>,

    #[serde(default)]
    pub ai_providers: HashMap<String, AIProviderConfig>, // Changed from ai_provider

    #[serde(default)]
    pub default_ai_provider: Option<String>, // Added default provider setting

    #[serde(default)]
    pub timeouts: TimeoutConfig,
}

impl Config {
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        
        // Debug absolute path
        let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        log::debug!("Loading config from: {:?}", abs_path);
        log::debug!("Current directory: {:?}", std::env::current_dir().unwrap_or_default());
        
        // Ensure config directory exists
        if let Some(parent) = path.parent() {
            log::debug!("Ensuring parent directory exists: {:?}", parent);
            match fs::create_dir_all(parent).await {
                Ok(_) => log::debug!("Parent directory created or already exists"),
                Err(e) => log::error!("Failed to create parent directory: {}", e),
            }
        }
        
        // Try to read existing config or create default
        log::debug!("Attempting to read config file");
        let config_str = match fs::read_to_string(path).await {
            Ok(content) => {
                log::debug!("Config file read successfully, content length: {}", content.len());
                content
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::debug!("Config file not found, creating default");
                let default_config = Self::default();
                let default_str = serde_json::to_string_pretty(&default_config)?;
                match fs::write(path, &default_str).await {
                    Ok(_) => log::debug!("Default config written to file"),
                    Err(e) => log::error!("Failed to write default config: {}", e),
                }
                default_str
            }
            Err(e) => {
                log::error!("Failed to read config file: {}", e);
                return Err(e.into());
            }
        };
        
        // Try fallback with std::fs if tokio::fs fails
        if config_str.trim().is_empty() {
            log::debug!("Empty config string, trying with std::fs");
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    log::debug!("Config file read successfully with std::fs");
                    let config: Self = serde_json::from_str(&content)?;
                    return Ok(config);
                }
                Err(e) => {
                    log::error!("Failed to read config with std::fs: {}", e);
                }
            }
        }
        
        match serde_json::from_str::<Self>(&config_str) {
            Ok(config) => {
                log::debug!("Config parsed successfully");
                Ok(config)
            },
            Err(e) => {
                log::error!("Failed to parse config: {}", e);
                Err(e.into())
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        // Add a default provider config (e.g., deepseek) to the map
        let mut default_providers = HashMap::new();
        default_providers.insert("deepseek".to_string(), AIProviderConfig::default());

        Self {
            servers: HashMap::new(),
            ai_providers: default_providers, // Use the map with default
            default_ai_provider: None, // No default provider specified by default
            timeouts: TimeoutConfig::default(),
        }
    }
}
