use anyhow::{anyhow, Result};
use console::style;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::host::server_manager::ManagedServer;
use crate::host::MCPHost; // Import MCPHost

/// Command processor for the REPL
pub struct CommandProcessor {
    host: MCPHost, // Store the host instance
    servers: Arc<Mutex<HashMap<String, ManagedServer>>>, // Keep servers for direct access if needed
    current_server: Option<String>,
    config_path: Option<PathBuf>,
}

impl CommandProcessor {
    // Modify constructor to take MCPHost
    pub fn new(host: MCPHost) -> Self {
        Self {
            servers: Arc::clone(&host.servers), // Get servers from host
            host,
            current_server: None,
            config_path: None,
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
            "provider" => self.cmd_provider(args).await, // Added provider command
            "providers" => self.cmd_providers().await, // Added providers command
            // chat command is handled directly in Repl::run
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
  provider [name]     - Show or set the active AI provider (e.g., openai, anthropic)
  providers           - List available AI providers (those with API keys set)
  exit, quit          - Exit the program".to_string()
        )
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
