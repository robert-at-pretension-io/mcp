use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path; // Removed PathBuf
use tokio::fs;
use anyhow::Result;
use crate::host::anyhow;
use log::{debug, info, warn}; // Added log imports

#[derive(Debug, Deserialize, Serialize, Clone)] // Add Clone
pub struct ServerConfig {
    pub command: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub args: Option<Vec<String>>, // Add optional args field
}

// Removed duplicate imports and struct definition below

#[derive(Debug, Deserialize, Serialize, Clone)] // Add Clone here
// Removed leftover struct field and closing brace below
// Removed duplicate derive below
pub struct AIProviderConfig {
    // Removed provider field, key in the map will be the provider name
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Deserialize, Serialize,Clone)]
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
} // End of impl Default for TimeoutConfig



impl Default for AIProviderConfig {
    fn default() -> Self {
        Self {
            // provider field removed
            model: "deepseek-chat".to_string(), // Default model
        }
        // Removed extra closing brace here
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)] // Add Clone here
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
    pub async fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        log::info!("Saving configuration to: {:?}", path);
        let json_string = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| anyhow!("Failed to create config directory {:?}: {}", parent, e))?;
        }

        fs::write(path, json_string).await
            .map_err(|e| anyhow!("Failed to write config file {:?}: {}", path, e))?;
        log::info!("Configuration saved successfully.");
        Ok(())
    }
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

// --- Provider Models Configuration ---

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ProviderModelsConfig {
    // The keys of this map are lowercase provider names (e.g., "openai", "anthropic")
    #[serde(flatten)] // Use flatten because the TOML has top-level provider keys
    pub providers: HashMap<String, ProviderModelList>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderModelList {
    pub models: Vec<String>,
}

impl ProviderModelsConfig {
    /// Load the provider models configuration from a TOML file.
    /// If the file doesn't exist or fails to parse, returns default (empty).
    pub async fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        debug!("Attempting to load provider models config from: {:?}", path);
        match fs::read_to_string(path).await {
            Ok(content) => {
                match toml::from_str::<Self>(&content) {
                    Ok(config) => {
                        info!("Successfully loaded provider models config from {:?}", path);
                        config
                    },
                    Err(e) => {
                        warn!("Failed to parse provider models config file {:?}: {}. Using default.", path, e);
                        Self::default()
                    }
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("Provider models config file not found at {:?}. Using default.", path);
                Self::default()
            },
            Err(e) => {
                warn!("Failed to read provider models config file {:?}: {}. Using default.", path, e);
                Self::default()
            }
        }
    }
}
