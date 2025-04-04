use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::PathBuf;

use super::connections::ServerConnections;
use shared_protocol_objects::client::ReplClient;

/// Command processor for the REPL
pub struct CommandProcessor {
    connections: ServerConnections,
}

impl CommandProcessor {
    pub fn new() -> Self {
        Self {
            connections: ServerConnections::new(),
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
            "servers" => self.cmd_servers(),
            "use" => self.cmd_use(args),
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
    
    /// List connected servers
    pub fn cmd_servers(&self) -> Result<String> {
        let servers = self.connections.server_names();
        if servers.is_empty() {
            return Ok("No servers connected".to_string());
        }
        
        let current = self.connections.current_server_name();
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
            
        Ok(format!("Connected servers:\n{}", server_list))
    }
    
    /// Set the current server
    pub fn cmd_use(&mut self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            self.connections.set_current_server(None)?;
            return Ok("Cleared current server selection".to_string());
        }
        
        let server_name = &args[0];
        self.connections.set_current_server(Some(server_name.clone()))?;
        Ok(format!("Now using server '{}'", server_name))
    }
    
    /// List tools for a server
    pub async fn cmd_tools(&self, args: &[String]) -> Result<String> {
        let server = self.get_server(args)?;
        let tools = server.list_tools().await?;
        
        if tools.is_empty() {
            return Ok(format!("No tools available on {}", server.name()));
        }
        
        let tool_list = tools.iter()
            .map(|tool| {
                let desc = tool.description.as_deref().unwrap_or("No description");
                format!("{} - {}", tool.name, desc)
            })
            .collect::<Vec<_>>()
            .join("\n");
            
        Ok(format!("Tools on {}:\n{}", server.name(), tool_list))
    }
    
    /// Call a tool
    pub async fn cmd_call(&self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            return Err(anyhow!("Usage: call <tool> [server] [json]"));
        }
        
        let tool_name = &args[0];
        
        // Determine server (from args or current)
        let server = if args.len() > 1 && !args[1].starts_with('{') {
            self.connections.get_server(&args[1])
                .ok_or_else(|| anyhow!("Server '{}' not found", args[1]))?
        } else {
            self.get_server(&[])?
        };
        
        // Parse JSON arguments (from args or as empty object)
        let json_arg = if args.len() > 1 && args[1].starts_with('{') {
            args[1].clone()
        } else if args.len() > 2 {
            args[2].clone()
        } else {
            "{}".to_string()
        };
        
        let args_value: Value = serde_json::from_str(&json_arg)
            .map_err(|e| anyhow!("Invalid JSON: {}", e))?;
            
        // Call the tool
        let result = server.call_tool(tool_name, args_value).await?;
        
        // Format result
        let mut output = if result.is_error.unwrap_or(false) {
            format!("Tool '{}' returned an error:\n", tool_name)
        } else {
            format!("Tool '{}' result:\n", tool_name)
        };
        
        for content in result.content {
            output.push_str(&content.text);
            output.push('\n');
        }
        
        Ok(output)
    }
    
    /// Get server from args or current
    fn get_server<'a>(&'a self, args: &[String]) -> Result<&'a dyn ReplClient> {
        if !args.is_empty() {
            self.connections.get_server(&args[0])
                .ok_or_else(|| anyhow!("Server '{}' not found", args[0]))
        } else {
            self.connections.get_current_server()
                .ok_or_else(|| anyhow!("No server specified and no current server selected"))
        }
    }
    
    // Public getters/setters for the connections
    
    pub fn add_server(&mut self, client: Box<dyn ReplClient>) -> Result<()> {
        self.connections.add_server(client)
    }
    
    pub fn remove_server(&mut self, name: &str) -> Result<Box<dyn ReplClient>> {
        self.connections.remove_server(name)
    }
    
    pub fn server_names(&self) -> Vec<String> {
        self.connections.server_names()
    }
    
    pub fn current_server_name(&self) -> Option<&str> {
        self.connections.current_server_name()
    }
    
    pub fn set_config_path(&mut self, path: PathBuf) {
        self.connections.set_config_path(path);
    }
    
    pub async fn close(self) -> Result<()> {
        self.connections.close_all().await
    }
}

impl Default for CommandProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repl::test_utils::MockTransport;
    use shared_protocol_objects::rpc::{McpClient, McpClientBuilder};
    use async_trait::async_trait;
    
    // Create a wrapper that implements ReplClient for McpClient<MockTransport>
    struct MockReplClient {
        client: McpClient<MockTransport>,
        name: String,
    }
    
    #[async_trait]
    impl shared_protocol_objects::client::ReplClient for MockReplClient {
        fn name(&self) -> &str {
            &self.name
        }
        
        async fn list_tools(&self) -> Result<Vec<shared_protocol_objects::ToolInfo>> {
            self.client.list_tools().await
        }
        
        async fn call_tool(&self, tool_name: &str, args: Value) -> Result<shared_protocol_objects::CallToolResult> {
            self.client.call_tool(tool_name, args).await
        }
        
        async fn close(self: Box<Self>) -> Result<()> {
            self.client.close().await
        }
    }
    
    async fn setup_test_processor() -> CommandProcessor {
        let mut processor = CommandProcessor::new();
        
        // Create mock client 1
        let transport1 = MockTransport::new();
        let mut client1 = McpClientBuilder::new(transport1)
            .client_info("test1", "1.0.0")
            .build();
        
        // Initialize the client first
        let caps = shared_protocol_objects::ClientCapabilities {
            experimental: None,
            sampling: None,
            roots: None,
        };
        client1.initialize(caps).await.unwrap();
            
        let repl_client1 = Box::new(MockReplClient {
            client: client1,
            name: "server1".to_string(),
        });
        
        // Create mock client 2
        let mut transport2 = MockTransport::new();
        transport2.add_tool("special_tool", "A special tool", serde_json::json!({}));
        transport2.add_call_result("special_tool", "Special tool result");
        
        let mut client2 = McpClientBuilder::new(transport2)
            .client_info("test2", "1.0.0")
            .build();
            
        // Initialize the client first
        let caps = shared_protocol_objects::ClientCapabilities {
            experimental: None,
            sampling: None,
            roots: None,
        };
        client2.initialize(caps).await.unwrap();
            
        let repl_client2 = Box::new(MockReplClient {
            client: client2,
            name: "server2".to_string(),
        });
        
        // Add the clients
        processor.add_server(repl_client1).unwrap();
        processor.add_server(repl_client2).unwrap();
        
        processor
    }
    
    #[tokio::test]
    async fn test_help_command() {
        let mut processor = CommandProcessor::new();
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