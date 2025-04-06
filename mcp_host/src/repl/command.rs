use anyhow::{anyhow, Result};
use console::style;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::io::{self, Write}; // Add io import

use crate::host::server_manager::ManagedServer;
use crate::host::MCPHost; // Import MCPHost
use crate::host::config::{ServerConfig}; // Removed AIProviderConfig
use rustyline::DefaultEditor; // Import Editor

/// Command processor for the REPL
pub struct CommandProcessor {
    host: MCPHost, // Store the host instance
    servers: Arc<Mutex<HashMap<String, ManagedServer>>>, // Keep servers for direct access if needed
    current_server: Option<String>,
    config_path: Option<PathBuf>,
    editor: DefaultEditor, // Add editor for prompting
}

impl CommandProcessor {
    // Modify constructor to take MCPHost and Editor
    pub fn new(host: MCPHost, editor: DefaultEditor) -> Self {
        Self {
            servers: Arc::clone(&host.servers), // Get servers from host
            host,
            current_server: None,
            config_path: None,
            editor, // Store the editor
        }
    }

    /// Process a command string
    pub async fn process(&mut self, command: &str) -> Result<String> {
        // Split the command into parts, respecting quotes
        let parts = match shellwords::split(command) {
            Ok(parts) => parts,
            Err(_) => return Err(anyhow!("Invalid command syntax (unmatched quotes?)"))
        };
        
        if parts.is_empty() {
            return Ok("".to_string());
        }
        
        let cmd = &parts[0];
        let args = &parts[1..];
        
        match cmd.as_str() {
            "help" => self.cmd_help(),
            "exit" | "quit" => Ok("exit".to_string()),
            "servers" => self.cmd_servers().await,
            "use" => self.cmd_use(args).await,
            "tools" => self.cmd_tools(args).await,
            "call" => self.cmd_call(args).await,
            "provider" => self.cmd_provider(args).await,
            "providers" => self.cmd_providers().await,
            "model" => self.cmd_model(args).await, // Added model command
            // chat command is handled directly in Repl::run
            "add_server" => self.cmd_add_server().await, // New command
            "edit_server" => self.cmd_edit_server(args).await, // New command
            "remove_server" => self.cmd_remove_server(args).await, // New command
            "save_config" => self.cmd_save_config().await, // New command
            "reload_config" => self.cmd_reload_config().await, // New command
            "show_config" => self.cmd_show_config(args).await, // New command
            _ => Err(anyhow!("Unknown command: '{}'. Type 'help' for available commands", cmd))
        }
    }

    /// Get available commands
    pub fn cmd_help(&self) -> Result<String> {
        Ok(
"Available commands:
  help                - Show this help
  servers             - List connected servers
  use [server]        - Set the current server (or clear if no server specified)
  tools [server]      - List tools for the current or specified server
  call <tool> [server] [json] - Call a tool with JSON arguments
  chat <server>       - Enter interactive chat mode with AI assistant and tools
  provider [name]     - Show or set the active AI provider
  providers           - List available AI providers
  model [name]        - Show or set the model for the active provider
  add_server          - Interactively add a new server configuration
  edit_server <name>  - Interactively edit an existing server configuration
  remove_server <name> - Remove a server configuration (requires save_config)
  show_config [server] - Show current configuration (all or specific server)
  save_config         - Save current server configurations to the file
  reload_config       - Reload configuration from file (discards unsaved changes)
  exit, quit          - Exit the program".to_string()
        )
    }

    // --- Interactive Add Server ---
    async fn cmd_add_server(&mut self) -> Result<String> {
        println!("--- Add New Server Configuration ---");

        let name = self.prompt_for_input("Enter unique server name:")?;
        if name.is_empty() { return Ok("Cancelled.".to_string()); }
        // Check uniqueness
        {
            let config_guard = self.host.config.lock().await;
            if config_guard.servers.contains_key(&name) {
                return Err(anyhow!("Server name '{}' already exists.", name));
            }
        }


        let command = self.prompt_for_input("Enter command to run server:")?;
        if command.is_empty() { return Ok("Cancelled.".to_string()); }

        let mut args = Vec::new();
        println!("Enter command arguments (one per line, press Enter on empty line to finish):");
        loop {
            let arg = self.prompt_for_input(&format!("Argument {}:", args.len() + 1))?;
            if arg.is_empty() { break; }
            args.push(arg);
        }

        let mut env = HashMap::new();
        println!("Enter environment variables (KEY=VALUE format, press Enter on empty line to finish):");
        loop {
            let env_line = self.prompt_for_input("Env Var (e.g., KEY=value):")?;
            if env_line.is_empty() { break; }
            if let Some((key, value)) = env_line.split_once('=') {
                env.insert(key.trim().to_string(), value.trim().to_string());
            } else {
                println!("{}", style("Invalid format. Use KEY=VALUE.").yellow());
            }
        }

        let server_config = ServerConfig {
            command,
            env,
            args: if args.is_empty() { None } else { Some(args) }, // Store args
        };

        // Add to in-memory config
        {
            let mut config_guard = self.host.config.lock().await;
            config_guard.servers.insert(name.clone(), server_config);
        }

        // Automatically save the configuration
        match self.host.save_host_config().await {
            Ok(_) => Ok(format!("Server '{}' added and configuration saved successfully.", name)),
            Err(e) => {
                // Log the error, but still report success for adding to memory
                log::error!("Failed to automatically save config after adding server '{}': {}", name, e);
                Ok(format!("Server '{}' added to configuration, but failed to save automatically: {}. Run 'save_config' manually.", name, e))
            }
        }
    }

    // --- Edit Server ---
    async fn cmd_edit_server(&mut self, args: &[String]) -> Result<String> {
        const DELETE_KEYWORD: &str = "DELETE";
        if args.is_empty() {
            return Err(anyhow!("Usage: edit_server <server_name>"));
        }
        let name = &args[0];
        println!("--- Edit Server Configuration for '{}' ---", name);
        println!("(Press Enter to keep current value, type '{}' to delete)", DELETE_KEYWORD);

        let mut config_guard = self.host.config.lock().await;

        // Get mutable access to the server config
        let server_config = match config_guard.servers.get_mut(name) {
            Some(cfg) => cfg,
            None => return Err(anyhow!("Server '{}' not found in configuration.", name)),
        };

        // --- Edit Command ---
        let command_prompt = format!("Command: ");
        let new_command = self.prompt_with_initial(&command_prompt, &server_config.command)?;
        if !new_command.is_empty() { // Only update if user provided non-empty input
            server_config.command = new_command;
        } else {
            println!("Keeping current command: {}", server_config.command); // Explicitly state keeping
        }


        // --- Edit Arguments ---
        println!("\n--- Editing Arguments ---");
        let mut current_args = server_config.args.clone().unwrap_or_default();
        let mut final_args = Vec::new();
        for (i, arg) in current_args.iter().enumerate() {
            let prompt = format!("Arg {}: ", i);
            let new_arg = self.prompt_with_initial(&prompt, arg)?;
            if new_arg.eq_ignore_ascii_case(DELETE_KEYWORD) {
                println!("Deleting argument: {}", arg);
            } else if new_arg.is_empty() {
                 println!("Keeping argument: {}", arg);
                 final_args.push(arg.clone()); // Keep original if input is empty
            } else {
                final_args.push(new_arg); // Use the edited value
            }
        }
        // Add new arguments
        println!("--- Add New Arguments (Press Enter on empty line to finish) ---");
        loop {
            let prompt = format!("New Arg {}: ", final_args.len());
            let new_arg = self.prompt_for_input(&prompt)?;
            if new_arg.is_empty() {
                break;
            }
            final_args.push(new_arg);
        }
        server_config.args = if final_args.is_empty() { None } else { Some(final_args) };


        // --- Edit Environment Variables ---
        println!("\n--- Editing Environment Variables ---");
        let mut current_env = server_config.env.clone();
        let mut final_env = HashMap::new();
        // Sort keys for consistent editing order
        let mut sorted_keys: Vec<String> = current_env.keys().cloned().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            let value = current_env.get(&key).unwrap(); // Should always exist
            let prompt = format!("Env '{}': ", key);
            let new_value = self.prompt_with_initial(&prompt, value)?;
            if new_value.eq_ignore_ascii_case(DELETE_KEYWORD) {
                println!("Deleting env var: {}", key);
            } else if new_value.is_empty() {
                 println!("Keeping env var: {}={}", key, value);
                 final_env.insert(key.clone(), value.clone()); // Keep original if input is empty
            } else {
                final_env.insert(key.clone(), new_value); // Use the edited value
            }
        }
        // Add new environment variables
        println!("--- Add New Environment Variables (KEY=VALUE format, press Enter to finish) ---");
        loop {
            let env_line = self.prompt_for_input("New Env Var (e.g., KEY=value):")?;
            if env_line.is_empty() {
                break;
            }
            if let Some((key, value)) = env_line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if !key.is_empty() {
                    final_env.insert(key.to_string(), value.to_string());
                } else {
                     println!("{}", style("Invalid format: Key cannot be empty.").yellow());
                }
            } else {
                println!("{}", style("Invalid format. Use KEY=VALUE.").yellow());
            }
        }
        server_config.env = final_env;

        // Config is updated in place because server_config is a mutable reference
        // Drop the lock explicitly before saving
        drop(config_guard);

        // Automatically save the configuration
        match self.host.save_host_config().await {
            Ok(_) => Ok(format!("Server '{}' configuration updated and saved successfully.", name)),
            Err(e) => {
                log::error!("Failed to automatically save config after editing server '{}': {}", name, e);
                Ok(format!("Server '{}' configuration updated in memory, but failed to save automatically: {}. Run 'save_config' manually.", name, e))
            }
        }
    }


    // --- Remove Server ---
    async fn cmd_remove_server(&mut self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            return Err(anyhow!("Usage: remove_server <server_name>"));
        }
        let name = &args[0];

        let removed = {
            let mut config_guard = self.host.config.lock().await;
            config_guard.servers.remove(name).is_some()
        };

        if removed {
            Ok(format!("Server '{}' removed from configuration. Run 'save_config' to make it persistent.", name))
        } else {
            Err(anyhow!("Server '{}' not found in configuration.", name))
        }
    }

    // --- Save Config ---
    async fn cmd_save_config(&self) -> Result<String> {
        match self.host.save_host_config().await {
            Ok(_) => Ok("Configuration saved successfully.".to_string()),
            Err(e) => Err(anyhow!("Failed to save configuration: {}", e)),
        }
        // Potentially trigger reconfiguration here if needed immediately after save
        // self.host.reload_host_config().await?; // Or a more direct reconfigure
    }

    // --- Reload Config ---
    async fn cmd_reload_config(&self) -> Result<String> {
         println!("{}", style("Warning: This will discard any unsaved configuration changes.").yellow());
         let confirm = self.prompt_for_input("Proceed? (yes/no):")?;
         if confirm.trim().to_lowercase() != "yes" {
             return Ok("Reload cancelled.".to_string());
         }

        match self.host.reload_host_config().await {
            Ok(_) => Ok("Configuration reloaded successfully.".to_string()),
            Err(e) => Err(anyhow!("Failed to reload configuration: {}", e)),
        }
    }

     // --- Show Config ---
     async fn cmd_show_config(&self, args: &[String]) -> Result<String> {
        let config_guard = self.host.config.lock().await;

        if args.is_empty() {
            // Show all config
            let config_str = serde_json::to_string_pretty(&*config_guard)
                .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;
            Ok(format!("Current Configuration:\n{}", config_str))
        } else {
            // Show specific server config
            let server_name = &args[0];
            if let Some(server_config) = config_guard.servers.get(server_name) {
                let server_config_str = serde_json::to_string_pretty(server_config)
                    .map_err(|e| anyhow!("Failed to serialize server config: {}", e))?;
                Ok(format!("Configuration for server '{}':\n{}", server_name, server_config_str))
            } else {
                Err(anyhow!("Server '{}' not found in configuration.", server_name))
            }
        }
    }


    // Helper for interactive input
    fn prompt_for_input(&self, prompt: &str) -> Result<String> {
        print!("{} ", style(prompt).green());
        io::stdout().flush()?; // Ensure prompt is displayed before reading
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(input.trim().to_string())
    }

    /// List available servers
    pub async fn cmd_servers(&self) -> Result<String> {
        let servers_map = self.servers.lock().await;
        let servers: Vec<String> = servers_map.keys().cloned().collect();
        if servers.is_empty() {
            return Ok("No servers available".to_string());
        }
        
        let current = self.current_server.as_deref();
        let server_list = servers.iter()
            .map(|name| {
                if Some(name.as_str()) == current {
                    format!("{} (current)", name)
                } else {
                    name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
            
        Ok(format!("Available servers:\n{}", server_list))
    }
    
    /// Set the current server
    pub async fn cmd_use(&mut self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            self.current_server = None;
            return Ok("Cleared current server selection".to_string());
        }
        
        let server_name = &args[0];
        // Check if server exists
        let servers_map = self.servers.lock().await;
        if servers_map.contains_key(server_name) {
            self.current_server = Some(server_name.clone());
            Ok(format!("Now using server '{}'", server_name))
        } else {
            Err(anyhow!("Server '{}' not found", server_name))
        }
    }
    
    /// List tools for a server
    pub async fn cmd_tools(&self, args: &[String]) -> Result<String> {
        let server_name = self.get_target_server_name(args)?;

        let servers_map = self.servers.lock().await;
        let server = servers_map.get(&server_name)
            .ok_or_else(|| anyhow!("Internal error: Server '{}' vanished", server_name))?;

        // Call list_tools directly on the ManagedServer's client
        let tools = server.client.list_tools().await?;

        if tools.is_empty() {
            return Ok(format!("No tools available on {}", server_name));
        }

        let tool_list = tools.iter()
            .map(|tool| {
                let desc = tool.description.as_deref().unwrap_or("No description");
                format!("{} - {}", tool.name, desc)
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!("Tools on {}:\n{}", server_name, tool_list))
    }

    /// Call a tool
    pub async fn cmd_call(&self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            return Err(anyhow!("Usage: call <tool> [server] [json]"));
        }
        
        let tool_name = &args[0];
        
        // Determine server name and JSON args
        let (server_name, json_arg_opt) = self.parse_call_args(args)?;
        let args_value: Value = match json_arg_opt {
            Some(json_str) => serde_json::from_str(&json_str)
                           .map_err(|e| anyhow!("Invalid JSON: {}", e))?,
            None => serde_json::json!({}), // Default to empty object
        };

        // Lock map, get server, call tool
        // Lock map, get server
        let servers_map = self.servers.lock().await;
        let server = servers_map.get(&server_name)
            .ok_or_else(|| anyhow!("Internal error: Server '{}' vanished", server_name))?;
            
        // Call tool with progress indicator
        let progress_msg = format!("Calling tool '{}' on server '{}'...", tool_name, server_name);
        let result = crate::repl::with_progress(
            progress_msg,
            server.client.call_tool(tool_name, args_value)
        ).await?;
        
        // Format result
        let mut raw_output = if result.is_error.unwrap_or(false) {
            format!("Tool '{}' on server '{}' returned an error:\n", tool_name, server_name)
        } else {
            format!("Tool '{}' result from server '{}':\n", tool_name, server_name)
        };
        
        for content in result.content {
            raw_output.push_str(&content.text);
            raw_output.push('\n');
        }
        
        // Truncate the output before returning
        Ok(crate::repl::truncate_lines(&raw_output, 150))
    }

    /// Show or set the active AI provider
    async fn cmd_provider(&self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            // Show current provider
            match self.host.get_active_provider_name().await {
                Some(name) => Ok(format!("Current AI provider: {}", style(name).cyan())),
                None => Ok("No AI provider is currently active.".to_string()),
            }
        } else {
            // Set provider
            let provider_name = &args[0];
            match self.host.set_active_provider(provider_name).await {
                Ok(_) => Ok(format!("AI provider set to: {}", style(provider_name).cyan())),
                Err(e) => Err(anyhow!("Failed to set provider: {}", e)),
            }
        }
    }

    /// List available AI providers
    async fn cmd_providers(&self) -> Result<String> {
        let providers = self.host.list_available_providers().await;
        if providers.is_empty() {
            Ok("No AI providers available (check API key environment variables)".to_string())
        } else {
            let current_provider = self.host.get_active_provider_name().await;
            let provider_list = providers.iter()
                .map(|name| {
                    if Some(name) == current_provider.as_ref() {
                        format!("{} (current)", style(name).cyan())
                    } else {
                        name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(format!("Available AI providers:\n{}", provider_list))
        }
    }

    /// Show or set the active AI model for the current provider
    async fn cmd_model(&self, args: &[String]) -> Result<String> {
        let active_provider = match self.host.get_active_provider_name().await {
            Some(name) => name,
            None => return Err(anyhow!("No active AI provider. Use 'provider <name>' first.")),
        };

        if args.is_empty() {
            // Show current model
            match self.host.ai_client().await {
                Some(client) => Ok(format!(
                    "Current model for provider '{}': {}",
                    style(&active_provider).cyan(),
                    style(client.model_name()).green()
                )),
                None => Ok(format!(
                    "No active model found for provider '{}'.",
                    style(&active_provider).cyan()
                )),
            }
        } else {
            // Set model
            let model_name = &args[0];
            match self.host.set_active_model(&active_provider, model_name).await {
                Ok(_) => Ok(format!(
                    "Model for provider '{}' set to: {}",
                    style(&active_provider).cyan(),
                    style(model_name).green()
                )),
                Err(e) => Err(anyhow!("Failed to set model: {}", e)),
            }
        }
    }

    // Helper methods

    /// Helper to get the server name to target
    fn get_target_server_name(&self, args: &[String]) -> Result<String> {
        if !args.is_empty() {
            // Explicit server name provided
            Ok(args[0].clone())
        } else {
            // Use current server if set
            self.current_server.clone()
                .ok_or_else(|| anyhow!("No server specified and no current server selected. Use 'use <server>'."))
        }
    }

    /// Helper to parse arguments for the 'call' command
    fn parse_call_args(&self, args: &[String]) -> Result<(String, Option<String>)> {
        // args[0] is the tool name

        // Determine server name (arg[1] unless it's JSON) or use current
        let (server_name, json_arg_index) = if args.len() > 1 && !args[1].starts_with('{') {
            (args[1].clone(), 2) // Server specified in arg[1], JSON might be in arg[2]
        } else {
            // Server not specified or arg[1] is JSON, use current
            let current = self.current_server.clone().ok_or_else(|| {
                anyhow!("No server specified and no current server selected for tool call.")
            })?;
            (current, 1) // JSON might be in arg[1]
        };

        // Extract JSON argument if present
        let json_arg = if args.len() > json_arg_index {
            Some(args[json_arg_index].clone())
        } else {
            None
        };

        Ok((server_name, json_arg))
    }
    
    // Public methods for server management
    
    pub fn current_server_name(&self) -> Option<&str> {
        self.current_server.as_deref()
    }
    
    pub fn set_config_path(&mut self, path: PathBuf) {
        self.config_path = Some(path);
    }
    
    pub async fn close(&self) -> Result<()> {
        // Nothing to close now, since we don't own the servers
        Ok(())
    }
}
