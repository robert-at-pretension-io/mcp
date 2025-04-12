use anyhow::{anyhow, Context, Result}; // Added Context import
use log::{debug, error, info, warn};
use serde_json::Value;
use shared_protocol_objects::{
    Implementation, ServerCapabilities, ToolInfo, CallToolResult, ClientCapabilities
};
// Removed unused async_trait
use std::collections::HashMap;
// Use TokioCommand explicitly, remove unused StdCommand alias
use tokio::process::Command as TokioCommand;
// Removed: use std::process::Command as StdCommand;
use tokio::process::Child as TokioChild;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

use crate::host::config::Config;

// Add re-exports for dependent code
// No cfg attribute - make this available to tests
pub mod testing {
    use rmcp::model::{Tool as ToolInfo, CallToolResult, ServerCapabilities, Content};
    
    // Test mock implementations 
    #[derive(Debug)]
    pub struct McpClient {
        pub _transport: ProcessTransport,
    }
    
    // Simple struct for testing
    #[derive(Debug)]
    pub struct ProcessTransport;
    
    // Helper function to create instance
    pub fn create_test_transport() -> ProcessTransport {
        ProcessTransport
    }
    
    // Helper function to create a client with transport
    pub fn create_test_client() -> McpClient {
        McpClient { _transport: create_test_transport() }
    }
    
    impl McpClient {
        pub fn new(_transport: ProcessTransport) -> Self {
            Self { _transport }
        }
    
        pub async fn list_tools(&self) -> anyhow::Result<Vec<ToolInfo>> {
            // Test implementation
            Ok(vec![
                ToolInfo {
                    name: "test_tool".to_string(),
                    description: Some("A test tool".to_string()),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "param1": {"type": "string"}
                        }
                    }),
                    annotations: None,
                }
            ])
        }
        
        pub async fn call_tool(&self, _name: &str, _args: serde_json::Value) -> anyhow::Result<CallToolResult> {
            // Test implementation
            Ok(CallToolResult {
                content: vec![
                    Content::Text {
                        text: "Tool executed successfully".to_string(),
                        annotations: None,
                    }
                ],
                is_error: None,
            })
        }
        
        pub async fn close(self) -> anyhow::Result<()> {
            Ok(())
        }
        
        pub fn capabilities(&self) -> Option<&ServerCapabilities> {
            None
        }
    }
}

// In production code, use these types from shared_protocol_objects, but wrap them
#[cfg(not(test))]
pub mod production {
    use rmcp::{
        model::{ClientJsonRpcMessage, ServerJsonRpcMessage, Tool as ToolInfo, CallToolResult, ClientCapabilities, InitializeResult},
        transport::child_process::TokioChildProcess, 
        service::RoleClient
    };
    use shared_protocol_objects;
    use std::sync::Arc;

    // Import shared protocol objects Transport for compatibility
    pub use shared_protocol_objects::rpc::Transport;
    
    // Wrapper for McpClient to provide Debug 
    pub struct McpClient {
        inner: RoleClient
    }
    
    // Manual Debug implementation
    impl std::fmt::Debug for McpClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("McpClient").finish()
        }
    }
    
    impl McpClient {
        pub fn new(client: RoleClient) -> Self {
            Self { inner: client }
        }
        
        pub async fn list_tools(&self) -> anyhow::Result<Vec<ToolInfo>> {
            log::info!("Using rmcp list_tools method");
            // Use rmcp's ListToolsRequest and extract tools from result
            let result = self.inner.list_tools(None).await?;
            Ok(result.tools)
        }

        pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> anyhow::Result<CallToolResult> {
            // Special handling for the tools/list case
            if name == "tools/list" {
                log::info!("Using specialized list_tools() method directly for tools/list call");
                let list_tools_result = self.inner.list_tools(None).await?;

                // Create a synthetic response using the tools field from the result
                let tools_json = format!("Found {} tools: {}", list_tools_result.tools.len(),
                    list_tools_result.tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", "));

                // Convert to shared_protocol_objects format temporarily
                let spo_result = shared_protocol_objects::CallToolResult {
                    content: vec![
                        shared_protocol_objects::ToolResponseContent {
                            type_: "text".to_string(),
                            text: tools_json,
                            annotations: None,
                        }
                    ],
                    is_error: Some(false),
                };
                
                // Create rmcp equivalent
                return Ok(CallToolResult {
                    content: vec![
                        rmcp::model::Content::Text { 
                            text: tools_json,
                            annotations: None 
                        }
                    ],
                    is_error: Some(false),
                });
            }
            
            // Normal path for other tools
            log::info!("Calling tool via rmcp call_tool method: {}", name);
            self.inner.call_tool(name, args).await
        }
        
        pub async fn close(self) -> anyhow::Result<()> {
            self.inner.close().await
        }
        
        pub fn capabilities(&self) -> Option<&rmcp::model::ServerCapabilities> {
            // Extract capabilities from the RoleClient
            self.inner.capabilities()
        }

        pub async fn initialize(&mut self, capabilities: ClientCapabilities) -> anyhow::Result<InitializeResult> {
            self.inner.initialize(capabilities, None).await
        }
    }
    
    // Use rmcp's built-in TokioChildProcess transport
    pub struct ProcessTransport(TokioChildProcess);
    
    // Manual Debug implementation
    impl std::fmt::Debug for ProcessTransport {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ProcessTransport").finish()
        }
    }
    
    impl ProcessTransport {
        pub async fn new(command: super::TokioCommand) -> anyhow::Result<Self> {
            let child_process = TokioChildProcess::new(command).await?;
            Ok(Self(child_process))
        }

        pub async fn new_for_request_type(command: super::TokioCommand, _request_type: &str) -> anyhow::Result<Self> { 
            log::info!("Creating dedicated transport");
            Self::new(command).await
        }
    }
    
    // Implement shared_protocol_objects::rpc::Transport for our ProcessTransport wrapper
    #[async_trait::async_trait]
    impl Transport for ProcessTransport {
        async fn send_request(&self, request: shared_protocol_objects::JsonRpcRequest) -> anyhow::Result<shared_protocol_objects::JsonRpcResponse> {
            // Convert from shared_protocol_objects to rmcp
            let rmcp_request = ClientJsonRpcMessage::Request(rmcp::model::JsonRpcRequest {
                jsonrpc: rmcp::model::JsonRpcVersion2_0,
                id: rmcp::model::RequestId::Number(1), // Use a default ID for now
                request: rmcp::model::Request {
                    method: request.method.clone(),
                    params: None, // This is a simplification for now
                }
            });
            
            // Send the request
            let response = self.0.send(rmcp_request).await?;
            
            // Convert back to shared_protocol_objects
            Ok(shared_protocol_objects::JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: serde_json::Value::Number(1.into()),
                result: Some(serde_json::Value::Null),
                error: None,
            })
        }
        
        async fn send_notification(&self, _notification: shared_protocol_objects::JsonRpcNotification) -> anyhow::Result<()> {
            // Simplified implementation - we're not processing notifications in this version
            Ok(())
        }
        
        async fn subscribe_to_notifications(&self, _handler: shared_protocol_objects::rpc::NotificationHandler) -> anyhow::Result<()> {
            // Simplified implementation - we're not processing notifications in this version
            Ok(())
        }
        
        async fn close(&self) -> anyhow::Result<()> {
            // Just delegate to the inner transport
            self.0.close().await
        }
    }
}

// For testing, use the mock implementations
#[cfg(test)]
pub use self::testing::{McpClient, ProcessTransport};

// For production, use the wrapped types
#[cfg(not(test))]
pub use self::production::{McpClient, ProcessTransport};

/// Represents a server managed by MCP host
#[derive(Debug)]
pub struct ManagedServer {
    pub name: String, 
    pub process: TokioChild,
    pub client: McpClient,
    pub capabilities: Option<rmcp::model::ServerCapabilities>,
}

// Add a helper method for testing
impl ManagedServer {
    #[cfg(test)]
    pub fn create_mock_client() -> McpClient {
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

            // Prepare components for start_server_with_components
            let program = server_config.command.clone();
            let args = server_config.args.clone().unwrap_or_default();
            let envs = server_config.env.clone();

            // Removed lines trying to modify non-existent 'command' variable

            // Start the server using the components
            match self.start_server_with_components(&name, &program, &args, &envs).await {
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

        // Use rmcp's list_tools
        match server.client.list_tools().await {
            Ok(tools_vec) => {
                info!("Successfully received tools list: {} tools", tools_vec.len());
                debug!("Tools list details: {:?}", tools_vec);
                Ok(tools_vec)
            },
            Err(e) => {
                error!("Error listing tools from {}: {:?}", server_name, e);
                Err(anyhow!("Failed to list tools from {}: {}", server_name, e)).context(e)
            }
        }
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
            
        // Special case for tools/list using rmcp's list_tools
        if tool_name == "tools/list" {
            info!("Using rmcp list_tools for tools/list call");
            let list_tools_result = server.client.list_tools().await?;

            // Create formatted tool result
            let tools_json = format!("Found {} tools: {}", list_tools_result.len(),
                list_tools_result.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", "));

            // Create a CallToolResult with rmcp's Content::Text
            let call_result = rmcp::model::CallToolResult {
                content: vec![
                    rmcp::model::content::Content::Text {
                        text: tools_json,
                        annotations: None,
                    }
                ],
                is_error: Some(false),
            };
            
            let output = format_tool_result(&call_result);
            return Ok(output);
        }
        
        // For other tools, use rmcp's call_tool method
        let result = server.client.call_tool(tool_name, args).await?;

        // Format the tool response content with updated format_tool_result
        let output = format_tool_result(&result);
        Ok(output)
    }
    
    /// Start a server with the given name, command and arguments
    pub async fn start_server(&self, name: &str, program: &str, args: &[String]) -> Result<()> {
        // Use start_server_with_components, assuming empty envs if not provided
        let envs = HashMap::new(); 
        self.start_server_with_components(name, program, args, &envs).await
    }

    /// Start a server with the given name and command components
    pub async fn start_server_with_components(
        &self,
        name: &str,
        program: &str,
        args: &[String],
        envs: &HashMap<String, String>,
    ) -> Result<()> {
        info!("Entered start_server_with_components for server: '{}'", name);
        info!("Starting server '{}' with program: {}, args: {:?}, envs: {:?}", name, program, args, envs.keys());

        // --- Prepare Tokio Command for Spawning ---
        let mut tokio_command_spawn = TokioCommand::new(program);
        tokio_command_spawn.args(args)
                           .envs(envs) // Set envs directly
                           .stdin(Stdio::piped())
                           .stdout(Stdio::piped())
                           .stderr(Stdio::piped());
        debug!("Tokio command prepared for spawning: {:?}", tokio_command_spawn);

        // --- Spawn Process and Initialize Client BEFORE acquiring the lock ---
        #[cfg(not(test))]
        let (process, client, capabilities) = {
            // Spawn the process first
            debug!("Spawning process for server '{}'...", name);
            let process = tokio_command_spawn.spawn()
                .map_err(|e| anyhow!("Failed to spawn process for server '{}': {}", name, e))?;
            info!("Process spawned successfully for server '{}', PID: {:?}", name, process.id());

            // Create the transport using the same command configuration
            let mut transport_cmd = TokioCommand::new(program);
            transport_cmd.args(args);
            transport_cmd.envs(envs);

            debug!("Creating transport for server '{}'...", name);
            let transport = ProcessTransport::new(transport_cmd).await
                 .map_err(|e| anyhow!("Failed to create transport for server '{}': {}", name, e))?;
            info!("Transport created for server '{}'.", name);

            // Create and initialize the client using rmcp's RoleClient
            debug!("Creating MCP client for server '{}'...", name);
            
            // Create client info with builder pattern
            let client_info = rmcp::model::Implementation {
                name: self.client_info.name.clone(),
                version: self.client_info.version.clone(),
            };
            
            // Create RoleClient
            let mut inner_client = rmcp::service::RoleClient::new(
                transport,
                client_info,
                rmcp::service::AtomicU32RequestIdProvider::default(),
            );
            
            let mut client = production::McpClient::new(inner_client);
            info!("MCP client created, initializing server '{}'...", name);

            // Use rmcp's client capabilities builder
            let client_capabilities = rmcp::model::ClientCapabilitiesBuilder::new()
                .with_experimental(serde_json::json!({}))
                .with_sampling(serde_json::json!({}))
                .with_roots(rmcp::model::RootsCapabilities::default())
                .build();
                
            let init_timeout = Duration::from_secs(15);
            
            // Initialize the client with timeout
            match tokio::time::timeout(init_timeout, client.initialize(client_capabilities)).await {
                Ok(Ok(init_result)) => {
                    info!("Client '{}' initialized successfully.", name);
                    let capabilities = Some(init_result.capabilities);
                    (process, client, capabilities)
                }
                Ok(Err(e)) => {
                    error!("Client '{}' initialization failed: {}", name, e);
                    return Err(e);
                }
                Err(elapsed) => {
                    error!("Client '{}' initialization timed out after {} seconds.", name, init_timeout.as_secs());
                    return Err(anyhow!("Client '{}' initialization timed out after {}s", name, init_timeout.as_secs()).context(elapsed));
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
fn format_tool_result(result: &rmcp::model::CallToolResult) -> String {
    let mut output = String::new();
    for content in &result.content {
        match content {
            rmcp::model::Content::Text { text, .. } => {
                output.push_str(text);
                output.push('\n');
            }
            _ => {
                output.push_str("Content type not supported for display\n");
            }
        }
    }
    output
}
