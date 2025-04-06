pub mod server_manager;
pub mod config;
pub mod protocol;
pub mod error;

use std::sync::Arc;
// Removed duplicate Duration, Result, Mutex, HashMap below
use anyhow::Result;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::Duration; // Re-add Duration import

// Removed duplicate imports below
use anyhow::{anyhow}; // Keep anyhow, remove duplicate Result
use log::{debug, error, info, warn}; // Added debug, warn
use server_manager::ManagedServer;
use shared_protocol_objects::Implementation;
// Removed duplicate HashMap
// Removed unused Arc, Duration, Mutex
// Removed duplicate Arc, Duration, Mutex below

use crate::ai_client::{AIClient, AIClientFactory};
use crate::host::config::{AIProviderConfig, Config as HostConfig}; // Renamed Config to HostConfig

pub struct MCPHost {
    pub servers: Arc<Mutex<HashMap<String, ManagedServer>>>,
    pub client_info: Implementation,
    pub request_timeout: Duration,
    ai_provider_configs: HashMap<String, AIProviderConfig>, // Store all provider configs
    active_provider_name: Arc<Mutex<Option<String>>>, // Track the active provider name
    ai_client: Arc<Mutex<Option<Arc<dyn AIClient>>>>, // Active client instance, wrapped in Mutex
}

impl Clone for MCPHost {
    fn clone(&self) -> Self {
        Self {
            servers: Arc::clone(&self.servers),
            client_info: self.client_info.clone(),
            request_timeout: self.request_timeout,
            ai_provider_configs: self.ai_provider_configs.clone(), // Clone the map
            active_provider_name: Arc::clone(&self.active_provider_name), // Clone the Arc
            ai_client: Arc::clone(&self.ai_client), // Clone the Arc around the Mutex
        }
    }
}

impl MCPHost {
    /// Create a new builder
    pub fn builder() -> MCPHostBuilder {
        MCPHostBuilder::new()
    }

    /// Create a new instance with default values
    pub async fn new() -> Result<Self> {
        MCPHostBuilder::new().build().await
    }

    /// Load configuration from a file
    pub async fn load_config(&self, config_path: &str) -> Result<()> {
        self.server_manager().load_config(config_path).await
    }

    /// Configure the host with a loaded configuration
    pub async fn configure(&self, config: HostConfig) -> Result<()> {
        // Note: This only configures servers now. AI provider config is handled at build time.
        self.server_manager().configure(config).await
    }

    /// Run the REPL interface
    pub async fn run_repl(&self) -> Result<()> {
        // Pass self.clone() directly to Repl::new and remove with_host
        let mut repl = crate::repl::Repl::new(self.clone())?;
        repl.run().await
    }

    /// Get a reference to the server manager
    fn server_manager(&self) -> server_manager::ServerManager {
        server_manager::ServerManager::new(
            Arc::clone(&self.servers),
            self.client_info.clone(),
            self.request_timeout,
        )
    }

    /// List the tools available on a server
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<shared_protocol_objects::ToolInfo>> {
        self.server_manager().list_server_tools(server_name).await
    }

    /// Call a tool on a server
    pub async fn call_tool(&self, server_name: &str, tool_name: &str, args: serde_json::Value) -> Result<String> {
        self.server_manager().call_tool(server_name, tool_name, args).await
    }

    /// Start a server with the given command
    pub async fn start_server(&self, name: &str, command: &str, args: &[String]) -> Result<()> {
        self.server_manager().start_server(name, command, args).await
    }

    /// Stop a server
    pub async fn stop_server(&self, name: &str) -> Result<()> {
        self.server_manager().stop_server(name).await
    }

    /// Enter chat mode with a server
    pub async fn enter_chat_mode(&self, server_name: &str) -> Result<crate::conversation_state::ConversationState> {
        // This implementation remains largely the same as in host.rs
        // Fetch tools from the server
        let tool_info_list = self.list_server_tools(server_name).await?;

        // Convert our tool list to a JSON structure - we'll use this for debugging
        let _tools_json: Vec<serde_json::Value> = tool_info_list.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description.as_ref().unwrap_or(&"".to_string()),
                "inputSchema": t.input_schema
            })
        }).collect();

        // Create the tools string first
        let tools_str = tool_info_list.iter().map(|tool| {
            format!(
                "- {}: {}\ninput schema: {:?}",
                tool.name,
                tool.description.as_ref().unwrap_or(&"".to_string()),
                tool.input_schema
            )
        }).collect::<Vec<_>>().join("");

        log::debug!("tool_str is {:?}", &tools_str);

        // Generate simplified system prompt
        let system_prompt = format!(
            "You are a helpful assistant with access to tools. Use tools EXACTLY according to their descriptions.\n\
            TOOLS:\n{}",
            tools_str
        );

        // Create the conversation state
        let mut state = crate::conversation_state::ConversationState::new(system_prompt, tool_info_list.clone());
        
        // Use the new smiley-delimited JSON format for tool calling
        let smiley_instruction = crate::conversation_service::generate_smiley_tool_system_prompt(&tool_info_list);

        log::debug!("smiley_instruction is {:?}", &smiley_instruction);

        // Add the smiley instruction as a system message
        state.add_system_message(&smiley_instruction);

        Ok(state)
    }

    /// Generate a system prompt with tool information
    pub fn generate_system_prompt(&self, tools: &[serde_json::Value]) -> String {
        let tools_section = serde_json::to_string_pretty(&serde_json::json!({ "tools": tools })).unwrap_or_else(|_| "".to_string());

        format!(
            "You are a helpful assistant with access to tools. Use tools only when necessary.\n\n\
            CORE RESPONSIBILITIES:\n\
            1. Create knowledge graph nodes when important new information is shared\n\
            2. Use tools to gather additional context when needed\n\
            3. Maintain natural conversation flow\n\n\
            TOOL USAGE GUIDELINES:\n\
            - Use tools only when they would provide valuable information\n\
            - Create nodes for significant new information\n\
            - Connect information when it helps the conversation\n\
            - Suggest tool usage only when it would be genuinely helpful\n\n\
            CONVERSATION STYLE:\n\
            - Focus on natural conversation\n\
            - Use tools subtly when needed\n\
            - Avoid excessive tool usage\n\
            - Only reference tool outputs when relevant\n\n\
            {}\n\n\
            TOOL CALLING FORMAT:\n\
            When calling a tool, your ENTIRE response must be a JSON object with this format:\n\
            {{\n\
                \"tool\": \"tool_name\",\n\
                \"arguments\": {{\n\
                    ... tool parameters ...\n\
                }}\n\
            }}\n\n\
            IMPORTANT: When calling a tool, your response must be ONLY valid JSON and nothing else.",
            tools_section
        )
    }

    /// Get the currently active AI client
    pub async fn ai_client(&self) -> Option<Arc<dyn AIClient>> {
        self.ai_client.lock().await.clone()
    }

    /// Get the name of the currently active AI provider
    pub async fn get_active_provider_name(&self) -> Option<String> {
        self.active_provider_name.lock().await.clone()
    }

    /// List providers that have configuration and a corresponding API key set in the environment.
    pub async fn list_available_providers(&self) -> Vec<String> {
        let mut available = Vec::new();
        // Check configured providers first
        for (name, _config) in &self.ai_provider_configs {
            if Self::get_api_key_for_provider(name).is_ok() {
                available.push(name.clone());
            }
        }
        // Check standard environment variables for providers not explicitly configured
        for provider in ["anthropic", "openai", "deepseek", "gemini", "ollama", "xai", "phind", "groq"] {
            if !available.contains(&provider.to_string()) && Self::get_api_key_for_provider(provider).is_ok() {
                 available.push(provider.to_string());
            }
        }
        available.sort();
        available.dedup(); // Remove duplicates if any
        available
    }

    /// Set the active AI provider by name.
    pub async fn set_active_provider(&self, provider_name: &str) -> Result<()> {
        info!("Attempting to set active AI provider to: {}", provider_name);

        // Find the config for the requested provider
        let config = self.ai_provider_configs
            .get(provider_name)
            .cloned() // Clone the config if found
            .unwrap_or_else(|| {
                // If not in config, create a provider-specific default config
                warn!("Provider '{}' not found in config, using provider default model.", provider_name);
                let default_model = Self::get_default_model_for_provider(provider_name);
                AIProviderConfig { model: default_model }
            });

        // Try to create the client for this provider
        match Self::create_ai_client_internal(provider_name, &config).await {
            Ok(Some(new_client)) => {
                let model_name = new_client.model_name(); // Get model name before moving
                // Update the active client and name
                *self.ai_client.lock().await = Some(Arc::from(new_client)); // Use Arc::from
                *self.active_provider_name.lock().await = Some(provider_name.to_string());
                info!("Successfully switched active AI provider to '{}' (model: {})", provider_name, model_name);
                Ok(())
            }
            Ok(None) => {
                // This means create_ai_client_internal determined no client could be created (e.g., missing API key)
                let error_msg = format!("Could not activate provider '{}'. API key might be missing or invalid.", provider_name);
                error!("{}", error_msg);
                Err(anyhow!(error_msg))
            }
            Err(e) => {
                // This means client creation failed for other reasons
                let error_msg = format!("Failed to create AI client for provider '{}': {}", provider_name, e);
                error!("{}", error_msg);
                Err(anyhow!(error_msg))
            }
        }
    }

    /// Internal helper to get the API key environment variable name for a provider.
    fn get_api_key_var(provider_name: &str) -> Option<&'static str> {
        match provider_name.to_lowercase().as_str() {
            "deepseek" => Some("DEEPSEEK_API_KEY"),
            "anthropic" => Some("ANTHROPIC_API_KEY"),
            "openai" => Some("OPENAI_API_KEY"),
            "gemini" | "google" => Some("GEMINI_API_KEY"), // Allow "google" as alias
            "xai" | "grok" => Some("XAI_API_KEY"), // Allow "grok" as alias
            "phind" => Some("PHIND_API_KEY"),
            "groq" => Some("GROQ_API_KEY"),
            "ollama" => None, // Ollama doesn't use an API key
            _ => None,
        }
    }

    /// Internal helper to get the API key for a provider from the environment.
    fn get_api_key_for_provider(provider_name: &str) -> Result<String> {
        if let Some(var_name) = Self::get_api_key_var(provider_name) {
            std::env::var(var_name)
                .map_err(|e| anyhow!("API key environment variable '{}' not found: {}", var_name, e))
        } else if provider_name.to_lowercase() == "ollama" {
            Ok("".to_string()) // Ollama doesn't need a key
        } else {
            Err(anyhow!("Unsupported provider or provider requires no API key: {}", provider_name))
        }
    }

    /// Helper to get a default model name for a given provider.
    fn get_default_model_for_provider(provider_name: &str) -> String {
        match provider_name.to_lowercase().as_str() {
            "anthropic" => "claude-3-haiku-20240307".to_string(),
            "openai" => "gpt-4o-mini".to_string(),
            "gemini" | "google" => "gemini-1.5-flash".to_string(), // Use flash as default
            "ollama" => "llama3".to_string(),
            "xai" | "grok" => "grok-1".to_string(), // Assuming a default, adjust if needed
            "phind" => "Phind-70B".to_string(), // Assuming a default
            "groq" => "llama3-8b-8192".to_string(),
            "deepseek" | _ => "deepseek-chat".to_string(), // Default fallback
        }
    }

     /// Internal helper to create an AI client instance.
     /// Refactored from the original builder logic.
     async fn create_ai_client_internal(provider_name: &str, config: &AIProviderConfig) -> Result<Option<Box<dyn AIClient>>> {
        let provider_lower = provider_name.to_lowercase();
        let model = &config.model; // Use model from config

        info!("Attempting to create AI client for provider: '{}', model: '{}'", provider_lower, model);

        match Self::get_api_key_for_provider(&provider_lower) {
            Ok(api_key) => {
                if provider_lower != "ollama" {
                    info!("Found API key for provider '{}'.", provider_lower);
                } else {
                    info!("Using Ollama provider (no API key needed).");
                }

                // Use AIClientFactory to create the client
                let factory_config = serde_json::json!({
                    "api_key": api_key,
                    "model": model // Pass the model name from config
                });

                match AIClientFactory::create(&provider_lower, factory_config) {
                    Ok(client) => {
                        info!("Successfully created AI client for provider '{}' with model '{}'", provider_lower, client.model_name());
                        Ok(Some(client))
                    },
                    Err(e) => {
                        error!("Failed to create AI client using factory for provider '{}': {}", provider_lower, e);
                        Err(e) // Propagate the factory error
                    }
                }
            },
            Err(e) => {
                // Only warn if it's not Ollama (which doesn't need a key)
                if provider_lower != "ollama" {
                    warn!("Could not get API key for provider '{}': {}. No AI client created.", provider_lower, e);
                } else {
                     // This case shouldn't happen for Ollama based on get_api_key_for_provider logic
                     error!("Unexpected error getting API key for Ollama: {}", e);
                }
                Ok(None) // It's not an error if the key isn't set, just means no client
            }
        }
    }
}

/// Builder for MCPHost configuration
pub struct MCPHostBuilder {
    // ai_client field removed, will be determined dynamically
    ai_provider_configs: Option<HashMap<String, AIProviderConfig>>, // Store map from config
    default_ai_provider: Option<String>, // Store default provider name from config
    request_timeout: Option<Duration>,
    client_info: Option<Implementation>,
}

impl MCPHostBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            ai_provider_configs: None,
            default_ai_provider: None,
            request_timeout: None,
            client_info: None,
        }
    }

    /// Set the AI provider configurations from loaded config
    pub fn ai_provider_configs(mut self, configs: HashMap<String, AIProviderConfig>) -> Self {
        self.ai_provider_configs = Some(configs);
        self
    }

    /// Set the default AI provider name from loaded config
    pub fn default_ai_provider(mut self, provider_name: Option<String>) -> Self {
        self.default_ai_provider = provider_name;
        self
    }

    /// Set the request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Set the client info
    pub fn client_info(mut self, name: &str, version: &str) -> Self {
        self.client_info = Some(Implementation {
            name: name.to_string(),
            version: version.to_string(),
        });
        self
    }

    /// Build the MCPHost
    pub async fn build(self) -> Result<MCPHost> {
        let ai_provider_configs = self.ai_provider_configs.unwrap_or_else(|| {
            warn!("No AI provider configurations provided, using default.");
            let mut defaults = HashMap::new();
            defaults.insert("deepseek".to_string(), AIProviderConfig::default());
            defaults
        });

        let mut initial_ai_client: Option<Arc<dyn AIClient>> = None;
        let mut active_provider_name: Option<String> = None;

        // Determine initial active provider
        // 1. Check explicit default from config
        if let Some(default_name) = self.default_ai_provider {
            if let Some(config) = ai_provider_configs.get(&default_name) {
                 match MCPHost::create_ai_client_internal(&default_name, config).await {
                    Ok(Some(client)) => {
                        info!("Using default provider from config: {}", default_name);
                        active_provider_name = Some(default_name.clone());
                        initial_ai_client = Some(Arc::from(client)); // Use Arc::from
                    }
                    Ok(None) => warn!("Default provider '{}' configured but API key missing or invalid.", default_name),
                    Err(e) => warn!("Failed to create client for default provider '{}': {}", default_name, e),
                 }
            } else {
                 warn!("Default provider '{}' specified in config but no configuration found for it.", default_name);
            }
        }

        // 2. If no client yet, check environment variables in preferred order
        if initial_ai_client.is_none() {
            let preferred_providers = ["anthropic", "openai", "deepseek", "gemini", "ollama", "xai", "phind", "groq"];
            for provider_name in preferred_providers {
                // Get config or use default if not explicitly configured
                let config = ai_provider_configs
                    .get(provider_name)
                    .cloned()
                    .unwrap_or_else(|| {
                        debug!("Using default config for provider check: {}", provider_name);
                        AIProviderConfig::default()
                    });

                match MCPHost::create_ai_client_internal(provider_name, &config).await {
                    Ok(Some(client)) => {
                        info!("Using first available provider found via environment variable: {}", provider_name);
                        active_provider_name = Some(provider_name.to_string());
                        initial_ai_client = Some(Arc::from(client)); // Use Arc::from
                        break; // Found one, stop checking
                    }
                    Ok(None) => { /* API key not found, continue checking */ }
                    Err(e) => warn!("Error checking provider '{}': {}", provider_name, e), // Log error but continue
                }
            }
        }

        if initial_ai_client.is_none() {
            warn!("No AI provider could be activated. Check configurations and API key environment variables.");
        }

        let request_timeout = self.request_timeout.unwrap_or(Duration::from_secs(120));

        let client_info = self.client_info.unwrap_or_else(|| Implementation {
            name: "mcp-host".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        });

        Ok(MCPHost {
            servers: Arc::new(Mutex::new(HashMap::new())),
            client_info,
            request_timeout,
            ai_provider_configs,
            active_provider_name: Arc::new(Mutex::new(active_provider_name)),
            ai_client: Arc::new(Mutex::new(initial_ai_client)),
        })
    }
}
