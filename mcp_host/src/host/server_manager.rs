use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
use serde_json::Value;
// Replace shared_protocol_objects imports with rmcp::model
use rmcp::model::{
    Implementation, ServerCapabilities, Tool as ToolInfo, CallToolResult, ClientCapabilities, Content // Added Content
};
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
    // Use rmcp types directly in testing mocks as well for consistency
    use rmcp::model::{Tool as ToolInfo, CallToolResult, ServerCapabilities, Content, Implementation, InitializeResult, ClientCapabilities}; // Added Implementation, InitializeResult, ClientCapabilities

    // Test mock implementations
    #[derive(Debug)]
    pub struct McpClient {
        pub _transport: MockProcessTransport, // Use MockProcessTransport
    }
    
    // Simple struct for testing - represents the transport mechanism
    #[derive(Debug)]
    pub struct MockProcessTransport; // Renamed for clarity

    // Helper function to create instance
    pub fn create_test_transport() -> MockProcessTransport { // Renamed
        MockProcessTransport
    }

    // Helper function to create a client with transport
    pub fn create_test_client() -> McpClient {
        McpClient { _transport: create_test_transport() }
    }

    impl McpClient {
        // Update constructor to accept the mock transport
        pub fn new(_transport: MockProcessTransport) -> Self { // Renamed transport type
            Self { _transport }
        }

        pub async fn list_tools(&self) -> anyhow::Result<Vec<ToolInfo>> {
            // Test implementation - returns rmcp::model::Tool
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

        // Test implementation - returns rmcp::model::CallToolResult
        pub async fn call_tool(&self, _name: &str, _args: serde_json::Value) -> anyhow::Result<CallToolResult> {
            Ok(CallToolResult {
                content: vec![
                    Content::Text {
                        text: "Tool executed successfully".to_string(),
                        annotations: None,
                    }
                ],
                is_error: Some(false), // Explicitly set is_error
            })
        }

        // Add mock initialize method
        pub async fn initialize(&mut self, _capabilities: ClientCapabilities) -> anyhow::Result<InitializeResult> {
            Ok(InitializeResult {
                protocol_version: rmcp::model::ProtocolVersion::LATEST,
                capabilities: ServerCapabilities::default(),
                server_info: Implementation { name: "mock-server".into(), version: "0.0.0".into() },
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

// In production code, use rmcp types directly
#[cfg(not(test))]
pub mod production {
    // Import necessary rmcp types
    use rmcp::{
        model::{Tool as ToolInfo, CallToolResult, ClientCapabilities, InitializeResult, Implementation}, // Added Implementation
        transport::child_process::TokioChildProcess,
        service::RoleClient, // Use RoleClient directly
    };
    // Removed unused shared_protocol_objects import
    // Removed unused Arc import
    // Removed unused ClientJsonRpcMessage, ServerJsonRpcMessage

    // Import shared protocol objects Transport for compatibility - KEEPING FOR NOW until fully migrated
    pub use shared_protocol_objects::rpc::Transport;

    // Wrapper for McpClient to provide Debug and hold RoleClient
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

        // Use rmcp's call_tool directly
        pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> anyhow::Result<CallToolResult> {
            log::info!("Calling tool via rmcp call_tool method: {}", name);
            // The special handling for "tools/list" is removed as RoleClient::call_tool should handle it.
            // If issues arise, we might need to re-introduce specific handling here or fix RoleClient.
            self.inner.call_tool(name, args).await
        }

        // Delegate close to RoleClient
        pub async fn close(self) -> anyhow::Result<()> {
            self.inner.close().await
        }

        // Delegate capabilities to RoleClient
        pub fn capabilities(&self) -> Option<&rmcp::model::ServerCapabilities> {
            self.inner.capabilities()
        }

        // Delegate initialize to RoleClient
        pub async fn initialize(&mut self, capabilities: ClientCapabilities) -> anyhow::Result<InitializeResult> {
            // Pass None for protocol_version, RoleClient will handle negotiation or default
            self.inner.initialize(capabilities, None).await
        }
    }

    // Use rmcp's built-in TokioChildProcess transport directly
    pub struct ProcessTransport(TokioChildProcess);

    // Manual Debug implementation for the wrapper
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

        // Add a method to get the inner transport if needed elsewhere, though RoleClient abstracts it
        pub fn into_inner(self) -> TokioChildProcess {
             self.0
        }
    }

    // Remove the manual implementation of shared_protocol_objects::rpc::Transport
    // RoleClient handles the transport interaction internally.
    // The #[async_trait] and impl block are removed.
}

// For testing, use the mock implementations
#[cfg(test)]
// Use alias for consistency if ProcessTransport name is used elsewhere in tests
pub use self::testing::{McpClient, MockProcessTransport as ProcessTransport};

// For production, use the wrapped types
#[cfg(not(test))]
pub use self::production::{McpClient, ProcessTransport};

/// Represents a server managed by MCP host
#[derive(Debug)]
pub struct ManagedServer {
    pub name: String,
    pub process: TokioChild, // Keep the process handle for killing
    pub client: McpClient, // This now wraps RoleClient (or is the test mock)
    pub capabilities: Option<rmcp::model::ServerCapabilities>, // Type is already correct
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
            testing::create_test_client() // This now returns the testing::McpClient
        }
    }
}

// Define the concrete type for the servers map using the production McpClient
type ServerMap = HashMap<String, ManagedServer>;

/// Manager for MCP-compatible tool servers
///
/// The ServerManager handles communication with tool servers using the shared protocol
/// library, providing a clean API for starting, stopping, and interacting with tool servers.
pub struct ServerManager {
    // Use the concrete ServerMap type here
    pub servers: Arc<Mutex<ServerMap>>,
    pub client_info: Implementation, // This should be rmcp::model::Implementation
    pub request_timeout: Duration,
}

impl ServerManager {
    /// Create a new ServerManager with the given parameters
    pub fn new(
        servers: Arc<Mutex<ServerMap>>, // Use ServerMap
        client_info: Implementation, // Use rmcp::model::Implementation
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
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<ToolInfo>> { // Return type is already rmcp::model::Tool
        let servers = self.servers.lock().await;
        let server = servers.get(server_name)
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;

        info!("Sending tool list request to server {}", server_name);

        // Use the client's list_tools method (which delegates to rmcp's RoleClient)
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

        // Use the client's call_tool method (which delegates to rmcp's RoleClient)
        // The special handling for "tools/list" is removed as RoleClient::call_tool handles it.
        let result = server.client.call_tool(tool_name, args).await
            .map_err(|e| anyhow!("Failed to call tool '{}' on server '{}': {}", tool_name, server_name, e))?;

        // Format the tool response content using rmcp::model::CallToolResult
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
        let (process, mut client, capabilities) = { // Make client mutable for initialize
            // Spawn the process first
            debug!("Spawning process for server '{}'...", name);
            let process = tokio_command_spawn.spawn()
                .map_err(|e| anyhow!("Failed to spawn process for server '{}': {}", name, e))?;
            let process_id = process.id(); // Get PID for logging
            info!("Process spawned successfully for server '{}', PID: {:?}", name, process_id);

            // Create the TokioChildProcess transport directly
            let mut transport_cmd = TokioCommand::new(program);
            transport_cmd.args(args);
            transport_cmd.envs(envs);
            // TokioChildProcess needs stdin/stdout/stderr piped
            transport_cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

            debug!("Creating TokioChildProcess transport for server '{}'...", name);
            let transport = TokioChildProcess::new(transport_cmd).await
                 .map_err(|e| anyhow!("Failed to create TokioChildProcess transport for server '{}': {}", name, e))?;
            info!("TokioChildProcess transport created for server '{}'.", name);

            // Create RoleClient directly
            debug!("Creating RoleClient for server '{}'...", name);
            // Use the client_info passed to ServerManager::new
            let inner_client = rmcp::service::RoleClient::new(
                transport, // Pass the TokioChildProcess directly
                self.client_info.clone(), // Clone the Implementation struct
                rmcp::service::AtomicU32RequestIdProvider::default(), // Use default ID provider
            );

            // Wrap RoleClient in our McpClient wrapper
            let mut client = production::McpClient::new(inner_client);
            info!("RoleClient created, initializing server '{}'...", name);

            // Define client capabilities using rmcp::model::ClientCapabilities
            // Keep it simple for now, assuming default capabilities are sufficient
            let client_capabilities = rmcp::model::ClientCapabilities::default();
            debug!("Using default client capabilities for initialization.");

            let init_timeout = Duration::from_secs(15); // Keep timeout

            // Initialize the client (which now calls RoleClient::initialize)
            match tokio::time::timeout(init_timeout, client.initialize(client_capabilities)).await {
                Ok(Ok(init_result)) => {
                    info!("Server '{}' initialized successfully.", name);
                    // Store the capabilities obtained from the server
                    let capabilities = Some(init_result.capabilities);
                    // Return the original process handle, the wrapped client, and capabilities
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
        let (process, mut client, capabilities) = { // Make client mutable
            // For tests, create a dummy client and process
            debug!("Creating mock process and client for test server '{}'", name);
            let process = tokio::process::Command::new("echo") // Keep dummy process
                .arg("test")
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()) // Add pipes for consistency
                .spawn()?;
            // Use the testing McpClient and MockProcessTransport
            let mut client = testing::McpClient::new(testing::create_test_transport()); // Use testing constructor
            // Mock initialization
            let _ = client.initialize(rmcp::model::ClientCapabilities::default()).await; // Call mock initialize
            let capabilities = client.capabilities().cloned(); // Get capabilities after mock init
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

/// Format a tool result (rmcp::model::CallToolResult) into a string for display
fn format_tool_result(result: &CallToolResult) -> String { // Type is already correct
    let mut output = String::new();
    // Handle potential error state first
    if result.is_error.unwrap_or(false) {
        output.push_str("TOOL ERROR:\n");
    }

    for content in &result.content {
        match content {
            // Handle Text content
            Content::Text { text, annotations: _ } => { // Ignore annotations for now
                output.push_str(text);
            }
            // Handle Json content - pretty print it
            Content::Json { json, annotations: _ } => { // Ignore annotations
                match serde_json::to_string_pretty(json) {
                    Ok(pretty_json) => {
                        output.push_str("```json\n");
                        output.push_str(&pretty_json);
                        output.push_str("\n```"); // Add closing fence
                    }
                    Err(_) => {
                        // Fallback to debug print if pretty printing fails
                        output.push_str(&format!("{:?}", json));
                    }
                }
            }
             // Handle Image content - provide a placeholder
             Content::Image { image: _, annotations: _ } => {
                 output.push_str("[Image content - display not supported]");
             }
            // Handle other potential content types if added in the future
             _ => {
                 output.push_str("[Unsupported content type]");
             }
        }
        output.push('\n'); // Add newline between content parts
    }
    // Trim trailing newline if present
    output.trim_end().to_string()
}
