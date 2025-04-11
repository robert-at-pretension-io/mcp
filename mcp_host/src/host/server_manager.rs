use anyhow::{anyhow, Result};
use log::{debug, error, info, warn}; // Removed unused trace, warn
use serde_json::Value;
use shared_protocol_objects::{
    Implementation, ServerCapabilities, ToolInfo, CallToolResult, ClientCapabilities
};
// Removed unused async_trait
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::process::Child as TokioChild;
use tokio::sync::Mutex;
use std::time::Duration;

use crate::host::config::Config;

// Add re-exports for dependent code
// No cfg attribute - make this available to tests
pub mod testing {
    // Test mock implementations 
    #[derive(Debug)]
    pub struct McpClient<T> {
        pub _transport: T,
    }
    
    // Simple struct for testing
    #[derive(Debug)]
    pub struct ProcessTransport;
    
    // Helper function to create instance
    pub fn create_test_transport() -> ProcessTransport {
        ProcessTransport
    }
    
    // Helper function to create a client with transport
    pub fn create_test_client() -> McpClient<ProcessTransport> {
        McpClient { _transport: create_test_transport() }
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
                    annotations: None, // Added missing field
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
                // Removed _meta, progress, total
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
            log::info!("Using enhanced list_tools method with response validation");
            // Update return type and extract tools from the result struct
            let result = self.inner.list_tools().await?;
            Ok(result.tools)
        }

        pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> anyhow::Result<shared_protocol_objects::CallToolResult> {
            // Special handling for the common error case
            if name == "tools/list" {
                log::info!("Using specialized list_tools() method directly for tools/list call");
                // Use the actual list_tools method which is more robust
                let list_tools_result = self.inner.list_tools().await?; // Get the full result

                // Create a synthetic response using the tools field from the result
                let tools_json = format!("Found {} tools: {}", list_tools_result.tools.len(), // Access .tools field
                    list_tools_result.tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", ")); // Access .tools field

                return Ok(shared_protocol_objects::CallToolResult {
                    content: vec![
                        shared_protocol_objects::ToolResponseContent {
                            type_: "text".to_string(),
                            text: tools_json,
                            annotations: None,
                        }
                    ],
                    is_error: Some(false),
                    // Removed _meta, progress, total
                });
            }
            
            // Normal path for other tools
            log::info!("Calling tool via enhanced call_tool method: {}", name);
            self.inner.call_tool(name, args).await
        }
        
        pub async fn close(self) -> anyhow::Result<()> {
            self.inner.close().await
        }
        
        pub fn capabilities(&self) -> Option<&shared_protocol_objects::ServerCapabilities> {
            self.inner.capabilities()
        }

        // Update return type to InitializeResult
        pub async fn initialize(&mut self, capabilities: shared_protocol_objects::ClientCapabilities) -> anyhow::Result<shared_protocol_objects::InitializeResult> {
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
        // Update function signature to accept std::process::Command
        pub async fn new(command: std::process::Command) -> anyhow::Result<Self> { 
            Ok(Self(rpc::ProcessTransport::new(command).await?))
        }

        // Helper method to create a new transport for a specific request type
        // This helps avoid the mixed response type issue by using separate transports
        // Update function signature to accept std::process::Command
        pub async fn new_for_request_type(command: std::process::Command, request_type: &str) -> anyhow::Result<Self> { 
            log::info!("Creating dedicated transport for request type: {}", request_type);
            // Create a new transport for this specific request type
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

// Add a helper method for testing
impl ManagedServer {
    #[cfg(test)]
    pub fn create_mock_client() -> McpClient<ProcessTransport> {
        #[cfg(not(test))]
        {
            panic!("This method should only be called in tests");
        }
        
        #[cfg(test)]
        {
            testing::create_test_client()
        }
    }
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
        
        // Try to use std::path to check if file exists first
        match std::path::Path::new(config_path).try_exists() {
            Ok(true) => {
                info!("Config file exists according to std::path");
                // File exists, try to read with std::fs
                let content = match std::fs::read_to_string(config_path) {
                    Ok(content) => {
                        info!("Successfully read config file with std::fs: {}", config_path);
                        info!("Content length: {}", content.len());
                        content
                    },
                    Err(e) => {
                        error!("Failed to read file with std::fs, falling back to tokio: {}", e);
                        // Fall back to Config::load
                        match Config::load(config_path).await {
                            Ok(config) => {
                                return self.configure(config).await;
                            }
                            Err(e) => {
                                error!("Failed to load config with tokio::fs: {}", e);
                                return Err(anyhow!("Failed to load config: {}", e));
                            }
                        }
                    }
                };
                
                // Parse config
                match serde_json::from_str::<Config>(&content) {
                    Ok(config) => {
                        info!("Successfully parsed config from std::fs");
                        return self.configure(config).await;
                    }
                    Err(e) => {
                        error!("Failed to parse config from std::fs: {}", e);
                        // Fall back to Config::load
                        match Config::load(config_path).await {
                            Ok(config) => {
                                return self.configure(config).await;
                            }
                            Err(e) => {
                                error!("Failed to load config with tokio::fs: {}", e);
                                return Err(anyhow!("Failed to load config: {}", e));
                            }
                        }
                    }
                }
            }
            Ok(false) => {
                error!("Config file does not exist: {}", config_path);
                // Try Config::load which will create default
                match Config::load(config_path).await {
                    Ok(config) => {
                        info!("Created default config");
                        return self.configure(config).await;
                    }
                    Err(e) => {
                        error!("Failed to create default config: {}", e);
                        return Err(anyhow!("Failed to create default config: {}", e));
                    }
                }
            }
            Err(e) => {
                error!("Failed to check if config file exists: {}", e);
                // Try Config::load as fallback
                match Config::load(config_path).await {
                    Ok(config) => {
                        info!("Successfully loaded config using fallback");
                        return self.configure(config).await;
                    }
                    Err(e) => {
                        error!("Failed to load config using fallback: {}", e);
                        return Err(anyhow!("Failed to check config file existence and load config: {}", e));
                    }
                }
            }
        }
    }
    
    /// Configure this manager with the given configuration
    /// Configure this manager with the given configuration
    /// Note: This method is now primarily used during initial load.
    /// Runtime changes should use MCPHost::apply_config.
    pub async fn configure(&self, config: Config) -> Result<()> {
        info!("Configuring ServerManager: Found {} servers in config", config.servers.len());
        for (name, server_config) in config.servers {
            info!("Preparing to start server '{}' with command: {}", name, server_config.command);

            // Create a std::process::Command first to easily set env vars and args
            let mut command = Command::new(&server_config.command);

            // Set environment variables if specified
            if !server_config.env.is_empty() {
                debug!("Setting environment variables for {}: {:?}", name, server_config.env.keys());
                command.envs(server_config.env); // Use .envs() for HashMap
            }

            // Add arguments if specified in config
            if let Some(args) = server_config.args {
                 if !args.is_empty() {
                     debug!("Adding arguments for {}: {:?}", name, args);
                     command.args(args);
                 }
            }

            // Start the server using the constructed std::process::Command
            // The start_server_with_command method handles converting it to tokio::process::Command
            match self.start_server_with_command(&name, command).await {
                Ok(_) => info!("Successfully started server '{}'", name),
                Err(e) => error!("Failed to start server '{}': {}", name, e), // Log error but continue
            }
        }

        Ok(())
    }

    /// List all available tools on the specified server
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<ToolInfo>> {
        let servers = self.servers.lock().await;
        let server = servers.get(server_name)
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;
            
        info!("Sending tool list request to server {}", server_name);
        
        // The client's list_tools method now has special handling for numeric IDs and
        // response validation to avoid the type mismatch issue
        let list_tools_result = server.client.list_tools().await?; // This is now Vec<ToolInfo>
        info!("Received tools list from server: {} tools", list_tools_result.len()); // Use .len() directly

        Ok(list_tools_result) // Return the Vec<ToolInfo> directly
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
            
        // Special case for tools/list to create a dedicated client if needed
        if tool_name == "tools/list" {
            info!("Using enhanced list_tools for tools/list call");
            let list_tools_result = server.client.list_tools().await?; // Get the full result

            // Convert to a CallToolResult format using the vector directly
            let tools_json = format!("Found {} tools: {}", list_tools_result.len(), // Use .len() directly
                list_tools_result.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", ")); // Use .iter() directly

            let call_result = shared_protocol_objects::CallToolResult {
                content: vec![
                    shared_protocol_objects::ToolResponseContent {
                        type_: "text".to_string(),
                        text: tools_json,
                        annotations: None,
                    }
                ],
                is_error: Some(false),
                // Removed _meta, progress, total
            };
            let output = format_tool_result(&call_result);
            return Ok(output);
        }
        
        // For other tools, use the standard call_tool method
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
        // ---> ADDED LOG <---
        info!("Entered start_server_with_command for server: '{}'", name);
        // ---> END ADDED LOG <---
        info!("Starting server '{}' with command: {:?}", name, command);
        debug!("Preparing tokio::process::Command for server '{}'", name);

        // Prepare the command for spawning the process
        let mut tokio_command_spawn = tokio::process::Command::new(command.get_program());
        tokio_command_spawn.args(command.get_args())
                           .stdin(Stdio::piped())
                           .stdout(Stdio::piped())
                           .stderr(Stdio::piped());

        // Copy environment variables from the std::process::Command
        for (key, val) in command.get_envs() {
            if let (Some(k), Some(v)) = (key.to_str(), val.map(|v| v.to_str()).flatten()) {
                tokio_command_spawn.env(k, v);
            }
        }
        debug!("Tokio command prepared for spawning: {:?}", tokio_command_spawn);

        // --- Spawn Process and Initialize Client BEFORE acquiring the lock ---
        #[cfg(not(test))]
        let (process, client, capabilities) = {
            // Spawn the process first
            debug!("Spawning process for server '{}'...", name);
            let process = tokio_command_spawn.spawn()
                .map_err(|e| anyhow!("Failed to spawn process for server '{}': {}", name, e))?;
            info!("Process spawned successfully for server '{}', PID: {:?}", name, process.id());

            // Create a separate command for the transport (needed by ProcessTransport::new)
            let mut transport_cmd = tokio::process::Command::new(command.get_program());
            transport_cmd.args(command.get_args())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            for (key, val) in command.get_envs() { // Copy env vars again for transport command
                if let (Some(k), Some(v)) = (key.to_str(), val.map(|v| v.to_str()).flatten()) {
                    transport_cmd.env(k, v);
                }
            }
            debug!("Creating transport for server '{}'...", name);
            let transport = ProcessTransport::new(transport_cmd).await
                 .map_err(|e| anyhow!("Failed to create transport for server '{}': {}", name, e))?;
            info!("Transport created for server '{}'.", name);

            // Create and initialize the client
            debug!("Creating MCP client for server '{}'...", name);
            let inner_client = shared_protocol_objects::rpc::McpClientBuilder::new(transport)
                .client_info(&self.client_info.name, &self.client_info.version)
                .timeout(self.request_timeout)
                .build();
            let mut client = production::McpClient::new(inner_client);
            info!("MCP client created, initializing server '{}'...", name);

            // Use correct default values for ClientCapabilities
            let client_capabilities = ClientCapabilities {
                experimental: serde_json::json!({}), // Use default empty object
                sampling: serde_json::json!({}),     // Use default empty object
                roots: Default::default(), // Use default RootsCapability
            };
            let init_timeout = Duration::from_secs(15);
            // Update to handle InitializeResult instead of just ServerCapabilities
            match tokio::time::timeout(init_timeout, client.initialize(client_capabilities)).await {
                Ok(Ok(init_result)) => { // init_result is InitializeResult
                    info!("Client '{}' initialized successfully.", name);
                    // Get capabilities from the InitializeResult
                    let capabilities = Some(init_result.capabilities); // Store capabilities from the result
                    (process, client, capabilities) // Return all three
                }
                Ok(Err(e)) => {
                    error!("Client '{}' initialization failed: {}", name, e);
                    // Attempt to kill the process since initialization failed
                    // process.start_kill().ok(); // Kill the spawned process
                    return Err(anyhow!("Client '{}' initialization failed: {}", name, e));
                }
                Err(_) => {
                    error!("Client '{}' initialization timed out after {} seconds.", name, init_timeout.as_secs());
                    // Attempt to kill the process since initialization timed out
                    // process.start_kill().ok(); // Kill the spawned process
                    return Err(anyhow!("Client '{}' initialization timed out", name));
                }
            }
        };

        #[cfg(test)]
        let (process, client, capabilities) = {
            // For tests, create a dummy client and process
            debug!("Creating mock process and client for test server '{}'", name);
            let process = tokio::process::Command::new("echo")
                .arg("test")
                .spawn()?;
            let client = McpClient { _transport: ProcessTransport };
            let capabilities = client.capabilities().cloned();
            info!("Mock process and client created for test server '{}'", name);
            (process, client, capabilities)
        };
        // --- End of process spawning and client initialization ---

        // --- Acquire Lock and Insert Server ---
        debug!("Creating ManagedServer struct for '{}'", name);
        let server = ManagedServer {
            name: name.to_string(),
            process,
            client,
            capabilities,
        };

        debug!("Acquiring servers lock to insert server '{}'...", name);
        { // Scope for the lock
            let mut servers_guard = self.servers.lock().await;
            debug!("Servers lock acquired for inserting '{}'.", name);
            servers_guard.insert(name.to_string(), server);
            debug!("Server '{}' inserted into map.", name);
        } // Lock released here
        debug!("Servers lock released after inserting '{}'.", name);

        info!("Finished start_server_with_command for '{}'", name);
        Ok(())
    }


    /// Stop a server and clean up its resources
    pub async fn stop_server(&self, name: &str) -> Result<()> {
        debug!("Attempting to stop server '{}'...", name);
        debug!("Acquiring servers lock to remove server '{}'...", name);
        let server_to_stop = { // Scope for lock
            let mut servers = self.servers.lock().await;
            debug!("Servers lock acquired for removing '{}'.", name);
            servers.remove(name) // Remove the server from the map
        }; // Lock released here
        debug!("Servers lock released after removing '{}'.", name);

        if let Some(mut server) = server_to_stop {
            info!("Found server '{}' in map, proceeding with shutdown.", name);
            // Close the client first to ensure clean shutdown
            debug!("Closing client for server '{}'...", name);
            if let Err(e) = server.client.close().await {
                error!("Error closing client for server '{}': {}", name, e);
                // Continue with process kill even if client close fails
            } else {
                debug!("Client for server '{}' closed successfully.", name);
            }

            // Then kill the process if it's still running
            debug!("Attempting to kill process for server '{}'...", name);
            if let Err(e) = server.process.start_kill() {
                error!("Error killing process for server '{}': {}", name, e);
                // Return error if killing fails? Or just log? For now, just log.
            } else {
                info!("Process for server '{}' killed successfully.", name);
            }
            Ok(()) // Return Ok even if there were errors during shutdown
        } else {
            warn!("Server '{}' not found in map, cannot stop.", name);
            Ok(()) // Not an error if the server wasn't running
        }
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
