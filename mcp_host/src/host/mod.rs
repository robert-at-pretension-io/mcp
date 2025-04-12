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
use log::{debug, error, info, warn};
use server_manager::ManagedServer;
use rmcp::model::Implementation as RmcpImplementation; // Alias Implementation
use rmcp::model::Tool as RmcpTool; // Alias Tool
use std::sync::Arc as StdArc; // Add alias import

use crate::ai_client::{AIClient, AIClientFactory};
use crate::host::config::{AIProviderConfig, Config as HostConfig, ProviderModelsConfig, ServerConfig}; // Added ServerConfig
use std::path::PathBuf;
pub struct MCPHost {
    pub servers: Arc<Mutex<HashMap<String, ManagedServer>>>,
    pub client_info: RmcpImplementation, // Use aliased type
    pub request_timeout: Duration,
    pub config: Arc<Mutex<HostConfig>>, // Store the whole config
    pub config_path: Arc<Mutex<Option<PathBuf>>>, // Store the config path
    // Removed ai_provider_configs
    active_provider_name: Arc<Mutex<Option<String>>>, // Track the active provider name
    ai_client: Arc<Mutex<Option<Arc<dyn AIClient>>>>, // Active client instance, wrapped in Mutex
    pub provider_models: Arc<Mutex<ProviderModelsConfig>>, // Added: Stores suggested models
    provider_models_path: Arc<Mutex<PathBuf>>, // Added: Path to provider_models.toml
}

impl Clone for MCPHost {
    fn clone(&self) -> Self {
        Self {
            servers: Arc::clone(&self.servers),
            client_info: self.client_info.clone(), // Use aliased type
            request_timeout: self.request_timeout,
            config: Arc::clone(&self.config), // Clone Arc for config
            config_path: Arc::clone(&self.config_path), // Clone Arc for path
            active_provider_name: Arc::clone(&self.active_provider_name),
            ai_client: Arc::clone(&self.ai_client),
            provider_models: Arc::clone(&self.provider_models), // Added clone
            provider_models_path: Arc::clone(&self.provider_models_path), // Added clone
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

    // Removed load_config method. Use reload_host_config or apply_config.

    /// Apply a new configuration, starting/stopping servers as needed.
    pub async fn apply_config(&self, new_config: HostConfig) -> Result<()> {
        // ---> ADDED LOG <---
        info!("Entered apply_config. Processing {} servers from new config.", new_config.servers.len());
        // ---> END ADDED LOG <---
        info!("Applying new configuration...");
        debug!("Acquiring servers lock to determine changes...");
        let server_manager = self.server_manager();
        let mut servers_to_start = Vec::new();
        let servers_to_stop: Vec<String>; // Keep String for consistency

        { // Scope for the servers lock
            let current_servers = self.servers.lock().await;
            debug!("Servers lock acquired.");
            let mut current_server_names = current_servers.keys().cloned().collect::<std::collections::HashSet<_>>();

            // Determine servers to start
            for (name, server_config) in &new_config.servers {
                if !current_servers.contains_key(name) {
                    info!("Server '{}' marked for start.", name);
                    // Prepare command components for starting later
                    let program = server_config.command.clone();
                    let args = server_config.args.clone().unwrap_or_default();
                    let envs = server_config.env.clone();
                    servers_to_start.push((name.clone(), program, args, envs));
                }
                // Remove from the set of current servers, leaving only those to be stopped
                current_server_names.remove(name);
            }

            // Servers remaining in current_server_names need to be stopped
            servers_to_stop = current_server_names.into_iter().collect();
            debug!("Servers lock released.");
        } // servers lock is released here

        // Stop servers that are no longer in the config
        if !servers_to_stop.is_empty() {
            info!("Stopping servers removed from config: {:?}", servers_to_stop);
            for name in servers_to_stop {
                debug!("Attempting to stop server '{}'", name);
                if let Err(e) = server_manager.stop_server(&name).await {
                    error!("Failed to stop server '{}': {}", name, e);
                } else {
                    info!("Successfully stopped server '{}'", name);
                }
            }
        } else {
            debug!("No servers need to be stopped.");
        }

        // Start new servers
        if !servers_to_start.is_empty() {
            info!("Starting new servers: {:?}", servers_to_start.iter().map(|(n, _, _, _)| n).collect::<Vec<_>>());
            for (name, program, args, envs) in servers_to_start {
                // ---> ADDED LOG <---
                info!("apply_config: Preparing to call start_server_with_command for '{}'", name);
                // ---> END ADDED LOG <---
                debug!("Attempting to start server '{}' with program: {}, args: {:?}, envs: {:?}", name, program, args, envs.keys());
                // Pass components instead of a Command object
                if let Err(e) = server_manager.start_server_with_components(&name, &program, &args, &envs).await {
                    error!("Failed to start server '{}': {}", name, e);
                    // Decide if you want to continue or return error
                } else {
                    info!("Successfully started server '{}'", name);
                }
            }
        } else {
            debug!("No new servers need to be started.");
        }
        info!("Finished processing server start/stop loop in apply_config.");

        // Update AI provider based on new config (if default changed or active one removed)
        debug!("Checking AI provider status after config change...");
        let default_provider = new_config.default_ai_provider.clone();
            let current_active = self.get_active_provider_name().await;

            let mut needs_provider_update = false;
            if let Some(active_name) = current_active {
                 if !new_config.ai_providers.contains_key(&active_name) {
                     warn!("Active provider '{}' removed from config.", active_name);
                     needs_provider_update = true;
                 }
            } else {
                 // No provider active, try setting default if specified
                 if default_provider.is_some() {
                     needs_provider_update = true;
                 }
            }

            if needs_provider_update {
                 if let Some(dp_name) = default_provider {
                     info!("Attempting to set default provider '{}' after config change.", dp_name);
                     if let Err(e) = self.set_active_provider(&dp_name).await {
                         warn!("Failed to set default provider after config change: {}", e);
                         // Clear active provider if setting default failed
                         *self.ai_client.lock().await = None;
                         *self.active_provider_name.lock().await = None;
                     }
                 } else {
                     info!("No default provider specified, clearing active provider.");
                     *self.ai_client.lock().await = None;
                     *self.active_provider_name.lock().await = None;
                 }
            }
            info!("Finished AI provider update logic in apply_config."); // <-- Add log here


            // Update the stored config
            debug!("Acquiring config lock to update stored config...");
            *self.config.lock().await = new_config;
            debug!("Config lock released.");
            info!("Configuration applied successfully.");
            info!("Exiting apply_config.");
            Ok(())
        } // End of apply_config

    // Method to save the current in-memory config
    pub async fn save_host_config(&self) -> Result<()> {
        debug!("Acquiring config and config_path locks for saving...");
        let config_to_save: HostConfig;
        let path_to_save: Option<PathBuf>;

        { // Scope for locks
            let config_guard = self.config.lock().await;
            let path_guard = self.config_path.lock().await;
            debug!("Config and config_path locks acquired.");
            config_to_save = (*config_guard).clone();
            path_to_save = (*path_guard).clone();
            debug!("Config and path cloned for saving.");
        } // Locks released here
        debug!("Config and config_path locks released.");

        if let Some(path) = path_to_save {
            debug!("Calling config.save() for path: {:?}", path);
            config_to_save.save(&path).await // Use cloned data
        } else {
            error!("No configuration file path set. Cannot save.");
            Err(anyhow!("No configuration file path set. Cannot save."))
        }
    }


    // Method to reload config from disk
    pub async fn reload_host_config(&self) -> Result<()> {
        debug!("Acquiring config_path lock for reloading...");
        let path_to_load: Option<PathBuf>;
        { // Scope for lock
            let path_guard = self.config_path.lock().await;
            debug!("Config_path lock acquired.");
            path_to_load = (*path_guard).clone();
            debug!("Config path cloned for reloading.");
        } // Lock released here
        debug!("Config_path lock released.");

        if let Some(path) = path_to_load {
            info!("Reloading configuration from {:?}", path);
            debug!("Calling HostConfig::load()...");
            let new_config = HostConfig::load(&path).await?; // Use cloned path
            debug!("Config loaded from disk, now calling apply_config...");
            self.apply_config(new_config).await?; // apply_config handles its own locks
            Ok(())
        } else {
            error!("No configuration file path set. Cannot reload.");
            Err(anyhow!("No configuration file path set. Cannot reload."))
        }
    }

    /// Reload the provider models configuration from disk.
    pub async fn reload_provider_models(&self) -> Result<()> {
        let path_to_load = { // Scope lock
            self.provider_models_path.lock().await.clone()
        };

        info!("Reloading provider models configuration from {:?}", path_to_load);
        let new_models_config = ProviderModelsConfig::load(&path_to_load).await;

        // Update the stored config
        *self.provider_models.lock().await = new_models_config;
        info!("Provider models configuration reloaded successfully.");
        Ok(())
    }

        /// Run the REPL interface
    pub async fn run_repl(&self) -> Result<()> {
        info!("Entering MCPHost::run_repl..."); // Log entry
        // Pass self.clone() directly to Repl::new and remove with_host
        info!("Attempting to create Repl instance..."); // Log before new()
        let mut repl = match crate::repl::Repl::new(self.clone()) {
            Ok(r) => {
                info!("Repl instance created successfully."); // Log after new()
                r
            },
            Err(e) => {
                error!("Failed to create Repl instance: {}", e); // Log error during new()
                return Err(e.into()); // Propagate error properly
            }
        };
        info!("Calling repl.run()..."); // Log before run()
        repl.run().await
    }

    /// Get a reference to the server manager
    fn server_manager(&self) -> server_manager::ServerManager {
        server_manager::ServerManager::new(
            StdArc::clone(&self.servers), // Use aliased Arc
            self.client_info.clone(), // Use aliased type
            self.request_timeout,
        )
    }

    /// List the tools available on a server
    // Update return type to use rmcp::model::Tool
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<RmcpTool>> { // Use aliased type
        self.server_manager().list_server_tools(server_name).await
    }

    /// Call a tool on a server
    pub async fn call_tool(&self, server_name: &str, tool_name: &str, args: serde_json::Value) -> Result<String> {
        self.server_manager().call_tool(server_name, tool_name, args).await
    }

    /// Start a server using a command string and optional extra arguments.
    /// This is a convenience wrapper. For more control (e.g., environment variables),
    /// modify the configuration and use `apply_config` or `reload_host_config`.
    pub async fn start_server(&self, name: &str, command: &str, extra_args: &[String]) -> Result<()> {
        self.server_manager().start_server(name, command, extra_args).await
    }

    /// Stop a server by name.
    pub async fn stop_server(&self, name: &str) -> Result<()> {
        self.server_manager().stop_server(name).await
    }

    /// List tools from all currently running servers, removing duplicates by name.
    // Update return type to use rmcp::model::Tool
    pub async fn list_all_tools(&self) -> Result<Vec<RmcpTool>> { // Use aliased type
        info!("Listing tools from all active servers...");
        let mut all_tools_map = HashMap::new(); // Use HashMap to deduplicate by name

        // --- Step 1: Collect Peers ---
        let peers_to_query: Vec<(String, rmcp::service::Peer<rmcp::service::RoleClient>)> = {
            let servers_guard = self.servers.lock().await;
            servers_guard.iter()
                .map(|(name, server)| (name.clone(), server.client.peer().clone())) // Clone the Peer
                .collect()
        }; // Lock released here
        debug!("Collected {} peers to query.", peers_to_query.len());

        // --- Step 2: Query Peers Concurrently (or sequentially) ---
        // Using sequential iteration for simplicity first. Can optimize with futures::join_all later if needed.
        for (server_name, peer) in peers_to_query {
            debug!("Querying tools for server '{}'...", server_name);
            // Directly call list_tools on the cloned Peer
            match peer.list_tools(None).await {
                Ok(list_tools_result) => {
                    let tools = list_tools_result.tools;
                    debug!("Found {} tools on server '{}'", tools.len(), server_name);
                    for tool in tools {
                        // Insert into HashMap, replacing duplicates (last one wins if names collide)
                        all_tools_map.insert(tool.name.clone(), tool);
                    }
                }
                Err(e) => {
                    // Log error using server_name obtained during peer collection
                    warn!("Failed to list tools for server '{}': {}. Skipping.", server_name, e);
                }
            }
        }

        // --- Step 3: Collect Unique Tools ---
        let unique_tools: Vec<_> = all_tools_map.into_values().collect();
        info!("Found {} unique tools across all servers.", unique_tools.len());
        Ok(unique_tools)
    }


    /// Find the name of the server that provides a specific tool.
    pub async fn get_server_for_tool(&self, tool_name: &str) -> Result<String> {
        debug!("Searching for server providing tool: {}", tool_name);
        let server_names = { // Scope lock
            let servers_guard = self.servers.lock().await;
            servers_guard.keys().cloned().collect::<Vec<_>>()
        }; // Lock released

        for server_name in server_names {
            // In the future, we could check cached capabilities here first.
            // For now, we call list_tools again.
            match self.list_server_tools(&server_name).await {
                Ok(tools) => {
                    if tools.iter().any(|t| t.name == tool_name) {
                        debug!("Found tool '{}' on server '{}'", tool_name, server_name);
                        return Ok(server_name);
                    }
                }
                Err(e) => {
                    warn!("Could not list tools for server '{}' while searching for tool '{}': {}", server_name, tool_name, e);
                }
            }
        }
        error!("Tool '{}' not found on any active server.", tool_name);
        Err(anyhow!("Tool '{}' not found on any active server", tool_name))
    }


    /// Enter chat mode with a specific server
    pub async fn enter_chat_mode(&self, server_name: &str) -> Result<crate::conversation_state::ConversationState> {
        info!("Entering single-server chat mode for '{}'", server_name);
        // Fetch tools from the specific server (already returns Vec<rmcp::model::Tool>)
        let tool_info_list = self.list_server_tools(server_name).await?;

        // Convert our tool list to a JSON structure - we'll use this for debugging
        let _tools_json: Vec<serde_json::Value> = tool_info_list.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description.parse::<String>().unwrap_or("".to_string()),
                "inputSchema": t.input_schema
            })
        }).collect();

        // Create the tools string first
        let tools_str = tool_info_list.iter().map(|tool| {
            format!(
                "- {}: {}\ninput schema: {}", // Use {} for schema display
                tool.name.as_ref(),
                // Revert to map().unwrap_or() on the Option<&Cow>
                tool.description.to_string(),
                serde_json::to_string_pretty(&tool.input_schema).unwrap_or_else(|_| "{}".to_string()) // Pretty print schema
            )
        }).collect::<Vec<_>>().join("\n"); // Join with newline

        log::debug!("tool_str is {:?}", &tools_str);

        // Generate simplified system prompt
        let system_prompt = format!(
            "You are a helpful assistant with access to tools. Use tools EXACTLY according to their descriptions.", // Base prompt
            // Tool instructions are now generated separately if needed
        );

        // Create the conversation state (passes Vec<rmcp::model::Tool>)
        let state = crate::conversation_state::ConversationState::new(system_prompt, tool_info_list);

        // The ConversationState::new only adds the base system prompt.
        // The tool instructions might be added later or handled by the AI client builder.
        // The generate_tool_system_prompt function is called within ConversationState::new.

        Ok(state)
    }

    /// Enter chat mode using tools from all available servers.
    pub async fn enter_multi_server_chat_mode(&self) -> Result<crate::conversation_state::ConversationState> {
        info!("Entering multi-server chat mode.");
        // Fetch tools from all servers (already returns Vec<rmcp::model::Tool>)
        let all_tools = self.list_all_tools().await?;

        // Generate system prompt using combined tool list
        let system_prompt = format!(
            "You are a helpful assistant with access to tools from multiple servers. Use tools EXACTLY according to their descriptions.", // Base prompt
            // Tool instructions are now generated separately if needed
        );

        // Create the conversation state
        let state = crate::conversation_state::ConversationState::new(system_prompt, all_tools);
        Ok(state)
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
        let config_guard = self.config.lock().await; // Lock config
        // Check configured providers first
        for (name, _config) in &config_guard.ai_providers { // Access via config
            if Self::get_api_key_for_provider(name).is_ok() {
                available.push(name.clone());
            }
        }
        drop(config_guard); // Release lock
        // Check standard environment variables for providers not explicitly configured
        for provider in ["anthropic", "openai", "deepseek", "gemini", "ollama", "xai", "phind", "groq", "openrouter"] {
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

        // Find the config for the requested provider from the host's config
        let provider_config = { // Scope for lock guard
            let config_guard = self.config.lock().await;
            config_guard.ai_providers
                .get(provider_name)
                .cloned() // Clone the Option<&AIProviderConfig> into Option<AIProviderConfig>
        }; // config lock released here

        // Get the default model using the new logic if config wasn't found
        let final_provider_config = provider_config.unwrap_or_else(|| { // Use provider_config here
            warn!("Provider '{}' not found in main config, determining default model...", provider_name);
            // Need to acquire provider_models lock here
            let default_model = { // Scope for provider_models lock
                let models_guard = futures::executor::block_on(self.provider_models.lock()); // Block briefly for sync access
                Self::get_default_model_for_provider(provider_name, &models_guard)
            };
            debug!("Using determined default model '{}' for provider '{}'", default_model, provider_name);
            AIProviderConfig { model: default_model }
        });

        // Try to create the client for this provider using the final config
        match Self::create_ai_client_internal(provider_name, &final_provider_config).await {
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

    /// Set the active AI model for the currently active provider.
    pub async fn set_active_model(&self, provider_name: &str, model_name: &str) -> Result<()> {
        info!("Attempting to set model to '{}' for provider '{}'", model_name, provider_name);

        // Ensure the provider we are setting the model for is actually the active one
        let current_active = self.get_active_provider_name().await;
        if current_active.as_deref() != Some(provider_name) {
            return Err(anyhow!(
                "Cannot set model for inactive provider '{}'. Current provider is {:?}.",
                provider_name, current_active.unwrap_or_else(|| "None".to_string())
            ));
        }

        // Create a temporary config with the new model name
        let temp_config = AIProviderConfig {
            model: model_name.to_string(),
        };

        // Try to create the client with the new model
        match Self::create_ai_client_internal(provider_name, &temp_config).await {
            Ok(Some(new_client)) => {
                // Update the active client
                *self.ai_client.lock().await = Some(Arc::from(new_client));
                info!("Successfully switched model to '{}' for provider '{}'", model_name, provider_name);
                Ok(())
            }
            Ok(None) => {
                let error_msg = format!("Could not create client for model '{}' with provider '{}'. API key might be missing or model invalid.", model_name, provider_name);
                error!("{}", error_msg);
                Err(anyhow!(error_msg))
            }
            Err(e) => {
                let error_msg = format!("Failed to set model '{}' for provider '{}': {}", model_name, provider_name, e);
                error!("{}", error_msg);
                Err(anyhow!(error_msg))
            }
        }
    }


    /// Internal helper to get the API key environment variable name for a provider.
    pub fn get_api_key_var(provider_name: &str) -> Option<&'static str> { // Make public
        match provider_name.to_lowercase().as_str() {
            "deepseek" => Some("DEEPSEEK_API_KEY"),
            "anthropic" => Some("ANTHROPIC_API_KEY"),
            "openai" => Some("OPENAI_API_KEY"),
            "gemini" | "google" => Some("GEMINI_API_KEY"), // Allow "google" as alias
            "xai" | "grok" => Some("XAI_API_KEY"), // Allow "grok" as alias
            "phind" => Some("PHIND_API_KEY"),
            "groq" => Some("GROQ_API_KEY"),
            "openrouter" => Some("OPENROUTER_API_KEY"),
            "ollama" => None, // Ollama doesn't use an API key
            _ => None,
        }
    }

    /// Internal helper to get the API key for a provider from the environment.
    pub fn get_api_key_for_provider(provider_name: &str) -> Result<String> { // Make public
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
    /// Prioritizes the first model listed in provider_models config, then falls back to hardcoded defaults.
    fn get_default_model_for_provider(
        provider_name: &str,
        provider_models_config: &ProviderModelsConfig, // Accept models config
    ) -> String {
        let provider_key = provider_name.to_lowercase();

        // Try getting the first model from the config
        if let Some(model_list) = provider_models_config.providers.get(&provider_key) {
            if let Some(first_model) = model_list.models.first() {
                if !first_model.is_empty() {
                    log::debug!("Using default model '{}' from provider_models.toml for provider '{}'", first_model, provider_name);
                    return first_model.clone();
                }
            }
        }

        // Fallback to hardcoded defaults if not found or empty in config
        log::debug!("Default model for provider '{}' not found in provider_models.toml, using hardcoded fallback.", provider_name);
        match provider_key.as_str() {
            "anthropic" => "claude-3-haiku-20240307".to_string(),
            "openai" => "gpt-4o-mini".to_string(),
            "gemini" | "google" => "gemini-1.5-flash".to_string(),
            "ollama" => "llama3".to_string(),
            "xai" | "grok" => "grok-1".to_string(),
            "phind" => "Phind-70B".to_string(),
            "groq" => "llama3-8b-8192".to_string(),
            "openrouter" => "openrouter/optimus-alpha".to_string(),
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
    config_path: Option<PathBuf>,
    provider_models_path: Option<PathBuf>, // Added path for provider models config
    // Removed ai_provider_configs and default_ai_provider
    request_timeout: Option<Duration>,
    client_info: Option<RmcpImplementation>, // Use aliased type
}

impl MCPHostBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config_path: None,
            provider_models_path: None, // Initialize new path
            request_timeout: None,
            client_info: None,
        }
    }

    /// Set the path to the configuration file
    pub fn config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Set the path to the provider models configuration file (optional)
    pub fn provider_models_path(mut self, path: PathBuf) -> Self {
        self.provider_models_path = Some(path);
        self
    }

    // Removed ai_provider_configs and default_ai_provider methods

    /// Set the request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Set the client info
    pub fn client_info(mut self, name: &str, version: &str) -> Self {
        self.client_info = Some(RmcpImplementation { // Use aliased type
            name: name.to_string().into(), // Convert to Cow<'static, str>
            version: version.to_string().into(), // Convert to Cow<'static, str>
            // Add other fields if needed, using Default::default()
            ..Default::default()
        });
        self
    }

    /// Build the MCPHost
    pub async fn build(self) -> Result<MCPHost> {
        // --- Configuration Loading ---
        let config_path = self.config_path.unwrap_or_else(|| {
            let default = dirs::config_dir()
                .map(|p| p.join("mcp/mcp_host_config.json"))
                .unwrap_or_else(|| PathBuf::from("mcp_host_config.json"));
            info!("Using default config path: {:?}", default);
            default
        });
        info!("Using main config path: {:?}", config_path); // Log the determined main config path

        // Load initial config or create default
        let initial_config = match HostConfig::load(&config_path).await {
             Ok(cfg) => {
                 info!("Loaded initial main config from {:?}", config_path);
                 cfg
             },
             Err(e) => {
                 warn!("Failed to load config from {:?}: {}. Using default.", config_path, e);
                 HostConfig::default()
             }
        };

        // Determine provider models config path
        let provider_models_path = self.provider_models_path.unwrap_or_else(|| {
            config_path // Use the main config path determined above
                .parent() // Get the directory (e.g., ~/.config/mcp)
                .map(|p| p.join("provider_models.toml")) // Append the filename
                .unwrap_or_else(|| PathBuf::from("provider_models.toml")) // Fallback
        });
        info!("Using provider models config path: {:?}", provider_models_path);

        // Load provider models config
        let provider_models_config = ProviderModelsConfig::load(&provider_models_path).await;

        // --- Client Info ---
        let client_info = self.client_info.unwrap_or_else(|| RmcpImplementation { // Use aliased type
            name: "mcp-host".to_string().into(), // Convert to Cow
            version: env!("CARGO_PKG_VERSION").to_string().into(), // Convert to Cow
            ..Default::default()
        });

        // --- Timeouts ---
        let request_timeout = self.request_timeout.unwrap_or(Duration::from_secs(120));

        // --- Initialize Core Host Structure (without servers started yet) ---
        let host_servers_map = StdArc::new(Mutex::new(HashMap::new()));
        let host = MCPHost {
            servers: StdArc::clone(&host_servers_map),
            client_info: client_info.clone(), // Clone for the host instance
            request_timeout,
            config: StdArc::new(Mutex::new(initial_config.clone())), // Store loaded config
            config_path: StdArc::new(Mutex::new(Some(config_path))),
            provider_models: StdArc::new(Mutex::new(provider_models_config.clone())), // Store loaded models
            provider_models_path: StdArc::new(Mutex::new(provider_models_path)),
            active_provider_name: StdArc::new(Mutex::new(None)), // Start with no active provider name
            ai_client: StdArc::new(Mutex::new(None)), // Start with no active client
        };

        // --- Start Initial Servers Defined in Config ---
        // --- Start Initial Servers Defined in Config ---
        // We need to call start_server_with_components directly on the host instance
        // after it's fully constructed but before returning it.
        // We'll do this after initializing the AI client.

        // --- Determine Initial AI Provider ---
        // (This logic remains largely the same, but operates on the created host instance's fields)


        // --- Determine Initial AI Provider ---
        // (This logic remains largely the same, but operates on the created host instance's fields)
        let mut initial_ai_client: Option<Arc<dyn AIClient>> = None;
        let mut active_provider_name: Option<String> = None;
        let default_provider_name = initial_config.default_ai_provider.clone();

        if let Some(ref name) = default_provider_name {
             if let Some(provider_config) = initial_config.ai_providers.get(name) {
                 match MCPHost::create_ai_client_internal(name, provider_config).await {
                     Ok(Some(client)) => {
                         info!("Using default provider from config: {}", name);
                         active_provider_name = Some(name.clone());
                         initial_ai_client = Some(Arc::from(client));
                     }
                     Ok(None) => warn!("Default provider '{}' configured but API key missing or invalid.", name),
                     Err(e) => warn!("Failed to create client for default provider '{}': {}", name, e),
                 }
             } else {
                 warn!("Default provider '{}' specified but not found in ai_providers config.", name);
             }
        }

        // If default didn't work, try preferred list
        if initial_ai_client.is_none() {
             let preferred_providers = ["anthropic", "openai", "deepseek", "gemini", "ollama", "xai", "phind", "groq", "openrouter"];
             for provider_name in preferred_providers {
                 let provider_config = initial_config.ai_providers
                     .get(provider_name); // Get reference

                 // Determine the config to use: from main config or default model from provider_models
                 let config_to_use = provider_config // Use provider_config here
                     .cloned() // Clone if found in main config
                     .unwrap_or_else(|| {
                         // If not in main config, get default model from provider_models_config
                         let default_model = MCPHost::get_default_model_for_provider(provider_name, &provider_models_config);
                         debug!("Using default model '{}' from provider_models for initial check of provider '{}'", default_model, provider_name);
                         AIProviderConfig { model: default_model }
                     });

                 // Try creating the client with the determined config
                 match MCPHost::create_ai_client_internal(provider_name, &config_to_use).await {
                     Ok(Some(client)) => {
                         info!("Using first available provider found via environment variable: {}", provider_name);
                         active_provider_name = Some(provider_name.to_string());
                         initial_ai_client = Some(Arc::from(client));
                         break;
                     }
                     Ok(None) => { /* API key not found, continue checking */ }
                     Err(e) => warn!("Error checking provider '{}': {}", provider_name, e), // Log error but continue
                 }
             }
        }

        if initial_ai_client.is_none() {
            warn!("No AI provider could be activated. Check configurations and API key environment variables.");
        }

        // --- Update Host with Initial AI Client ---
        // (The host struct was already created, now update the AI fields)
        *host.ai_client.lock().await = initial_ai_client;
        *host.ai_client.lock().await = initial_ai_client;
        *host.active_provider_name.lock().await = active_provider_name;

        // --- Start Initial Servers AFTER Host is Constructed ---
        // Now that the host object exists, we can call its methods.
        // We need to clone the initial_config again or access it via host.config
        let config_for_startup = host.config.lock().await.clone();
        info!("Starting initial servers from configuration...");
        let mut servers_started_successfully = 0;
        for (name, server_config) in &config_for_startup.servers {
            info!("Attempting initial start for server '{}'", name);
            let program = &server_config.command;
            let args = server_config.args.as_deref().unwrap_or(&[]); // Get args slice
            let envs = &server_config.env;
            // Call the method on the host instance itself
            match host.server_manager().start_server_with_components(name, program, args, envs).await {
                 Ok(_) => {
                     info!("Successfully started initial server '{}'", name);
                     servers_started_successfully += 1;
                 }
                 Err(e) => {
                     // Log error but continue trying to start other servers
                     error!("Failed to start initial server '{}': {}", name, e);
                 }
            }
        }
         info!("Finished starting initial servers. {} started successfully out of {}.",
               servers_started_successfully, config_for_startup.servers.len());


        info!("MCPHost build complete.");
        Ok(host) // Return the fully initialized host
    }
}
