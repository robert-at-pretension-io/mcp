use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// Removed old import: use shared_protocol_objects::client::ReplClient;
use crate::host::server_manager::ManagedServer;

/// Command processor for the REPL
pub struct CommandProcessor {
    servers: Arc<Mutex<HashMap<String, ManagedServer>>>,
    current_server: Option<String>,
    config_path: Option<PathBuf>,
}

impl CommandProcessor {
    pub fn new(servers: Arc<Mutex<HashMap<String, ManagedServer>>>) -> Self {
        Self {
            servers,
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
  tools [server]      - List tools for a server
  call <tool> [server] [json] - Call a tool with JSON arguments
  chat <server>       - Enter interactive chat mode with AI assistant and tools
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
        let servers_map = self.servers.lock().await;
        let server = servers_map.get(&server_name)
            .ok_or_else(|| anyhow!("Internal error: Server '{}' vanished", server_name))?;
        let result = server.client.call_tool(tool_name, args_value).await?;
        
        // Format result
        let mut output = if result.is_error.unwrap_or(false) {
            format!("Tool '{}' on server '{}' returned an error:\n", tool_name, server_name)
        } else {
            format!("Tool '{}' result from server '{}':\n", tool_name, server_name)
        };
        
        for content in result.content {
            output.push_str(&content.text);
            output.push('\n');
        }
        
        Ok(output)
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

// Remove Default implementation since we now require servers parameter
// impl Default for CommandProcessor {
//     fn default() -> Self {
//         Self::new()
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repl::test_utils::MockTransport;
    
    // Removed old ReplClient implementation that is no longer needed
    
    async fn setup_test_processor() -> CommandProcessor {
        // Import what we need for tests
        use std::process::Stdio;
        use tokio::process::Child;
        // Use test utils for MockTransport
        use crate::host::server_manager::testing::{McpClient, ProcessTransport};
        
        // Create a HashMap of servers
        let servers_map = Arc::new(Mutex::new(HashMap::new()));
        
        // Create the processor with the map
        let processor = CommandProcessor::new(Arc::clone(&servers_map));
        
        // Add mock servers to the map
        {
            let mut servers = servers_map.lock().await;
            
            // Create server1
            let cmd1 = tokio::process::Command::new("echo")
                .arg("test")
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
                
            // Create transport for server1
            let transport1 = MockTransport::new();
            let client1 = McpClient { _transport: transport1 };
            
            // Create server2 with special tool
            let cmd2 = tokio::process::Command::new("echo")
                .arg("test")
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
                
            // Create transport for server2 with tool
            let mut transport2 = MockTransport::new();
            transport2.add_tool("special_tool", "A special tool", serde_json::json!({}));
            transport2.add_call_result("special_tool", "Special tool result");
            let client2 = McpClient { _transport: transport2 };
            
            // Insert the servers
            servers.insert("server1".to_string(), ManagedServer {
                name: "server1".to_string(),
                process: cmd1,
                client: client1,
                capabilities: None,
            });
            
            servers.insert("server2".to_string(), ManagedServer {
                name: "server2".to_string(),
                process: cmd2,
                client: client2,
                capabilities: None,
            });
        }
        
        processor
    }
    
    #[tokio::test]
    async fn test_help_command() {
        let servers_map = Arc::new(Mutex::new(HashMap::new()));
        let mut processor = CommandProcessor::new(servers_map);
        let result = processor.process("help").await.unwrap();
        assert!(result.contains("Available commands"));
        assert!(result.contains("servers"));
        assert!(result.contains("tools"));
    }
    
    #[tokio::test]
    async fn test_servers_command() {
        let mut processor = setup_test_processor().await;
        let result = processor.process("servers").await.unwrap();
        assert!(result.contains("server1"));
        assert!(result.contains("server2"));
        assert!(result.contains("current")); // First server should be current
    }
    
    #[tokio::test]
    async fn test_use_command() {
        let mut processor = setup_test_processor().await;
        
        // Change current server
        let result = processor.process("use server2").await.unwrap();
        assert!(result.contains("server2"));
        assert_eq!(processor.current_server_name(), Some("server2"));
        
        // Clear current server
        let result = processor.process("use").await.unwrap();
        assert!(result.contains("Cleared"));
        assert_eq!(processor.current_server_name(), None);
        
        // Try invalid server
        let result = processor.process("use invalid_server").await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_tools_command() {
        let mut processor = setup_test_processor().await;
        
        // List tools on specific server
        let result = processor.process("tools server2").await.unwrap();
        assert!(result.contains("special_tool"));
        
        // List tools on current server
        let result = processor.process("tools").await.unwrap();
        assert!(result.contains("test_tool"));
        
        // Try invalid server
        let result = processor.process("tools invalid_server").await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_call_command() {
        let mut processor = setup_test_processor().await;
        
        // Call tool on specific server
        let result = processor.process("call special_tool server2").await.unwrap();
        assert!(result.contains("Special tool result"));
        
        // Call tool on current server
        let result = processor.process("call test_tool").await.unwrap();
        assert!(result.contains("Test tool output"));
        
        // Call with JSON args
        let result = processor.process("call test_tool '{\"param1\": \"value1\"}'").await.unwrap();
        assert!(result.contains("Test tool output"));
        
        // Error cases
        let result = processor.process("call").await;
        assert!(result.is_err());
        
        // In our mocked transport, even invalid tools return a result
        let result = processor.process("call invalid_tool").await.unwrap();
        assert!(result.contains("No result defined for tool: invalid_tool"));
        
        let result = processor.process("call test_tool invalid_server").await;
        assert!(result.is_err());
        
        let result = processor.process("call test_tool '{invalid json}'").await;
        assert!(result.is_err());
    }
}