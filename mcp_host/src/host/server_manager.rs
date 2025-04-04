use anyhow::{anyhow, Result};
use log::{debug, error, info};
use serde_json::Value;
use shared_protocol_objects::{
    ClientCapabilities, Implementation, ServerCapabilities, ToolInfo, CallToolResult
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::process::Child as TokioChild;
use tokio::sync::Mutex;
use std::time::Duration;

use crate::host::config::Config;

// Add re-exports for dependent code
#[cfg(test)]
pub mod testing {
    // Test mock implementations 
    #[derive(Debug)]
    pub struct McpClient<T> {
        pub _transport: T,
    }
    
    #[derive(Debug)]
    pub struct ProcessTransport;
    
    impl Default for ProcessTransport {
        fn default() -> Self {
            Self
        }
    }
    
    impl McpClient<ProcessTransport> {
        pub fn new(_transport: ProcessTransport) -> Self {
            Self { _transport }
        }
    }
    
    impl<T> McpClient<T> {
        pub async fn list_tools(&self) -> anyhow::Result<Vec<shared_protocol_objects::ToolInfo>> {
            // Test implementation
            Ok(vec![
                shared_protocol_objects::ToolInfo {
                    name: "test_tool".to_string(),
                    description: Some("A test tool".to_string()),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "param1": {"type": "string"}
                        }
                    }),
                }
            ])
        }
        
        pub async fn call_tool(&self, _name: &str, _args: serde_json::Value) -> anyhow::Result<shared_protocol_objects::CallToolResult> {
            // Test implementation
            Ok(shared_protocol_objects::CallToolResult {
                content: vec![
                    shared_protocol_objects::ToolResponseContent {
                        type_: "text".to_string(),
                        text: "Tool executed successfully".to_string(),
                        annotations: None,
                    }
                ],
                is_error: None,
                _meta: None,
                progress: None,
                total: None,
            })
        }
        
        pub async fn close(self) -> anyhow::Result<()> {
            Ok(())
        }
        
        pub fn capabilities(&self) -> Option<&shared_protocol_objects::ServerCapabilities> {
            None
        }
    }
}

// In production code, use these types from shared_protocol_objects, but wrap them
#[cfg(not(test))]
pub mod production {
    use shared_protocol_objects::rpc;
    
    // Wrapper for McpClient to provide Debug 
    pub struct McpClient<T: rpc::Transport> {
        inner: rpc::McpClient<T>
    }
    
    // Manual Debug implementation
    impl<T: rpc::Transport> std::fmt::Debug for McpClient<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("McpClient").finish()
        }
    }
    
    impl<T: rpc::Transport> McpClient<T> {
        pub fn new(client: rpc::McpClient<T>) -> Self {
            Self { inner: client }
        }
        
        pub async fn list_tools(&self) -> anyhow::Result<Vec<shared_protocol_objects::ToolInfo>> {
            self.inner.list_tools().await
        }
        
        pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> anyhow::Result<shared_protocol_objects::CallToolResult> {
            self.inner.call_tool(name, args).await
        }
        
        pub async fn close(self) -> anyhow::Result<()> {
            self.inner.close().await
        }
        
        pub fn capabilities(&self) -> Option<&shared_protocol_objects::ServerCapabilities> {
            self.inner.capabilities()
        }
        
        pub async fn initialize(&mut self, capabilities: shared_protocol_objects::ClientCapabilities) -> anyhow::Result<shared_protocol_objects::ServerCapabilities> {
            self.inner.initialize(capabilities).await
        }
    }
    
    // Create our own wrapper for ProcessTransport that implements Debug
    pub struct ProcessTransport(rpc::ProcessTransport);
    
    // Manual Debug implementation since the inner type doesn't impl Debug
    impl std::fmt::Debug for ProcessTransport {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ProcessTransport").finish()
        }
    }
    
    impl ProcessTransport {
        pub async fn new(command: tokio::process::Command) -> anyhow::Result<Self> {
            Ok(Self(rpc::ProcessTransport::new(command).await?))
        }
    }
    
    // Implement Transport for our ProcessTransport wrapper
    #[async_trait::async_trait]
    impl rpc::Transport for ProcessTransport {
        async fn send_request(&self, request: shared_protocol_objects::JsonRpcRequest) -> anyhow::Result<shared_protocol_objects::JsonRpcResponse> {
            self.0.send_request(request).await
        }
        
        async fn send_notification(&self, notification: shared_protocol_objects::JsonRpcNotification) -> anyhow::Result<()> {
            self.0.send_notification(notification).await
        }
        
        async fn subscribe_to_notifications(&self, handler: rpc::NotificationHandler) -> anyhow::Result<()> {
            self.0.subscribe_to_notifications(handler).await
        }
        
        async fn close(&self) -> anyhow::Result<()> {
            self.0.close().await
        }
    }
    
    // Re-export Transport trait
    pub use rpc::Transport;
}

// For testing, use the mock implementations
#[cfg(test)]
pub use self::testing::{McpClient, ProcessTransport};

// For production, use the wrapped types
#[cfg(not(test))]
pub use self::production::{McpClient, Transport, ProcessTransport};

/// Represents a server managed by MCP host
#[derive(Debug)]
pub struct ManagedServer {
    pub name: String, 
    pub process: TokioChild,
    pub client: McpClient<ProcessTransport>,
    pub capabilities: Option<ServerCapabilities>,
}

/// Manager for MCP-compatible tool servers
///
/// The ServerManager handles communication with tool servers using the shared protocol
/// library, providing a clean API for starting, stopping, and interacting with tool servers.
pub struct ServerManager {
    pub servers: Arc<Mutex<HashMap<String, ManagedServer>>>,
    pub client_info: Implementation,
    pub request_timeout: Duration,
}

impl ServerManager {
    /// Create a new ServerManager with the given parameters
    pub fn new(
        servers: Arc<Mutex<HashMap<String, ManagedServer>>>, 
        client_info: Implementation,
        request_timeout: Duration,
    ) -> Self {
        Self {
            servers,
            client_info,
            request_timeout,
        }
    }
    
    /// Load server configuration from the given config file
    pub async fn load_config(&self, config_path: &str) -> Result<()> {
        info!("Loading configuration from: {}", config_path);
        let config = Config::load(config_path).await?;
        self.configure(config).await
    }
    
    /// Configure this manager with the given configuration
    pub async fn configure(&self, config: Config) -> Result<()> {
        info!("Found {} servers in config", config.servers.len());
        for (name, server_config) in config.servers {
            // Start each configured server
            let mut command = Command::new(&server_config.command);
            
            // Set environment variables if specified
            for (key, value) in server_config.env {
                command.env(key, value);
            }
            
            self.start_server_with_command(&name, command).await?;
        }
        
        Ok(())
    }

    /// List all available tools on the specified server
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<ToolInfo>> {
        let servers = self.servers.lock().await;
        let server = servers.get(server_name)
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;
            
        info!("Sending tool list request to server {}", server_name);
        let tools = server.client.list_tools().await?;
        info!("Received tools list from server");
        
        Ok(tools)
    }

    /// Call a tool on the specified server with the given arguments
    pub async fn call_tool(&self, server_name: &str, tool_name: &str, args: Value) -> Result<String> {
        debug!("call_tool started");
        debug!("Server: {}", server_name);
        debug!("Tool: {}", tool_name);
        debug!("Arguments: {}", serde_json::to_string_pretty(&args).unwrap_or_default());
        
        let servers = self.servers.lock().await;
        let server = servers.get(server_name)
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;
            
        let result = server.client.call_tool(tool_name, args).await?;

        // Format the tool response content
        let output = format_tool_result(&result);
        Ok(output)
    }

    /// Start a server with the given name, command and arguments
    pub async fn start_server(&self, name: &str, command: &str, args: &[String]) -> Result<()> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        self.start_server_with_command(name, cmd).await
    }

    /// Start a server with the given name and command
    pub async fn start_server_with_command(&self, name: &str, command: Command) -> Result<()> {
        info!("Starting server '{}' with command: {:?}", name, command);
        
        // Prepare the command for the ProcessTransport
        let mut tokio_command = tokio::process::Command::new(command.get_program());
        tokio_command.args(command.get_args())
                     .stdin(Stdio::piped())
                     .stdout(Stdio::piped())
                     .stderr(Stdio::piped());
                     
        // For any environment variables
        for (key, val) in command.get_envs() {
            if let (Some(k), Some(v)) = (key.to_str(), val.map(|v| v.to_str()).flatten()) {
                tokio_command.env(k, v);
            }
        }

        info!("Creating MCP client with ProcessTransport");
        
        #[cfg(not(test))]
        let (process, client, capabilities) = {
            // In production, use the shared_protocol_objects library
            // Spawn the process first to ensure we have it
            let process = tokio_command.spawn()?;
            
            // Create a separate command for the transport
            let mut transport_cmd = tokio::process::Command::new(command.get_program());
            transport_cmd.args(command.get_args())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
                
            // Copy environment variables
            for (key, val) in command.get_envs() {
                if let (Some(k), Some(v)) = (key.to_str(), val.map(|v| v.to_str()).flatten()) {
                    transport_cmd.env(k, v);
                }
            }
            
            // Create the transport
            let transport = ProcessTransport::new(transport_cmd).await?;
            
            // Create a client with this transport
            let inner_client = shared_protocol_objects::rpc::McpClientBuilder::new(transport)
                .client_info(&self.client_info.name, &self.client_info.version)
                .timeout(self.request_timeout)
                .build();
                
            // Wrap it in our debug-friendly wrapper
            let mut client = production::McpClient::new(inner_client);
            
            // Initialize the client
            let capabilities = ClientCapabilities {
                experimental: None,
                sampling: None,
                roots: None,
            };
            
            client.initialize(capabilities).await?;
            
            // Get the capabilities
            let capabilities = client.capabilities().cloned();
            
            (process, client, capabilities)
        };
        
        #[cfg(test)]
        let (process, client, capabilities) = {
            // For tests, create a dummy client
            let process = tokio::process::Command::new("echo")
                .arg("test")
                .spawn()?;
                
            let client = McpClient { _transport: ProcessTransport };
            let capabilities = client.capabilities().cloned();
            
            (process, client, capabilities)
        };
        
        let server = ManagedServer {
            name: name.to_string(),
            process,
            client,
            capabilities,
        };

        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.to_string(), server);
        }

        Ok(())
    }

    /// Stop a server and clean up its resources
    pub async fn stop_server(&self, name: &str) -> Result<()> {
        let mut servers = self.servers.lock().await;
        if let Some(mut server) = servers.remove(name) {
            // Close the client first to ensure clean shutdown
            if let Err(e) = server.client.close().await {
                error!("Error closing client: {}", e);
            }
            
            // Then kill the process if it's still running
            if let Err(e) = server.process.start_kill() {
                error!("Error killing process: {}", e);
            }
        }
        Ok(())
    }
}

/// Format a tool result into a string for display
fn format_tool_result(result: &CallToolResult) -> String {
    let mut output = String::new();
    for content in &result.content {
        match content.type_.as_str() {
            "text" => {
                output.push_str(&content.text);
                output.push('\n');
            }
            _ => {
                output.push_str(&format!("Unknown content type: {}\n", content.type_));
            }
        }
    }
    output
}