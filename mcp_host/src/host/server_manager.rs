use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
use serde_json::Value;
use rmcp::model::{
    Implementation as RmcpImplementation, // Alias Implementation
    Tool as RmcpTool, // Alias Tool
    CallToolResult as RmcpCallToolResult, // Alias CallToolResult
    ServerCapabilities as RmcpServerCapabilities, // Alias ServerCapabilities
    ClientCapabilities as RmcpClientCapabilities, // Alias ClientCapabilities
    InitializeResult as RmcpInitializeResult, // Alias InitializeResult
    CallToolRequestParam as RmcpCallToolRequestParam, // Alias CallToolRequestParam
    Content as RmcpContent, // Alias Content
    RawContent as RmcpRawContent, // Alias RawContent
    RawTextContent as RmcpRawTextContent, // Alias RawTextContent
};
use rmcp::service::serve_client; // Removed unused Peer, RoleClient imports
use rmcp::transport::TokioChildProcess; // Add back TokioChildProcess import
use std::collections::HashMap;
// Use TokioCommand explicitly, remove unused StdCommand alias
use tokio::process::Command as TokioCommand;
// Removed: use std::process::Command as StdCommand;
use tokio::process::Child as TokioChild;
use std::process::Stdio;
use std::sync::Arc; // Re-add top-level Arc import
use tokio::sync::Mutex;
use std::time::Duration;
// Removed imports related to ManualTransport: ChildStdin, ChildStdout, rmcp::{TransportStream, TransportSink, TransportError}, bytes::Bytes, futures::{SinkExt, StreamExt}, tokio_util::codec


// Removed unused Config import


// --- Manual Transport Implementation Removed ---
// The ManualTransport struct and its impl blocks have been deleted.
// We will use rmcp::transport::TokioChildProcess directly.


// Add re-exports for dependent code
// No cfg attribute - make this available to tests
pub mod testing {
    // Use aliased rmcp types in testing mocks
    use crate::host::server_manager::{
        RmcpTool, RmcpCallToolResult, RmcpServerCapabilities, RmcpImplementation,
        RmcpInitializeResult, RmcpClientCapabilities, RmcpContent, RmcpRawContent,
        RmcpRawTextContent,
    };
    use rmcp::model::ProtocolVersion; // Keep ProtocolVersion direct
    use std::borrow::Cow;
    use std::sync::Arc as StdArc;

    // Test mock implementations (McpClient remains a simple struct for tests)
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

        pub async fn list_tools(&self) -> anyhow::Result<Vec<RmcpTool>> { // Use aliased type
            // Test implementation - returns rmcp::model::Tool
            // Fix field types according to rmcp::model::Tool definition
            Ok(vec![
                // Explicitly type the struct literal as RmcpTool
                RmcpTool {
                    name: Cow::Borrowed("test_tool"),
                    // Assign Some directly to the Option field
                    description: ("test tool".to_string()).into(),
                    input_schema: StdArc::new(serde_json::json!({ // input_schema needs Arc<Map<String, Value>>
                        "type": "object",
                        "properties": {
                            "param1": {"type": "string"}
                        }
                    }).as_object().unwrap().clone()), // Convert Value to Map and Arc it
                    // Removed annotations field as it doesn't exist in rmcp::model::Tool
                }
            ])
        }

        // Test implementation - returns rmcp::model::CallToolResult
        pub async fn call_tool(&self, _name: &str, _args: serde_json::Value) -> anyhow::Result<RmcpCallToolResult> { // Use aliased type
            // Use aliased rmcp types
            Ok(RmcpCallToolResult {
                content: vec![
                    RmcpContent::new( // Use aliased type
                        RmcpRawContent::Text( // Use aliased type
                            RmcpRawTextContent { // Use aliased type
                                text: "Tool executed successfully".to_string(),
                            }
                        ),
                        None // Annotations are Option<Vec<Annotation>>
                    )
                ],
                is_error: Some(false),
            })
        }

        // Add mock initialize method
        pub async fn initialize(&mut self, _capabilities: RmcpClientCapabilities) -> anyhow::Result<RmcpInitializeResult> { // Use aliased types
            Ok(RmcpInitializeResult { // Use aliased type
                protocol_version: ProtocolVersion::LATEST,
                capabilities: RmcpServerCapabilities::default(), // Use aliased type
                server_info: RmcpImplementation { name: "mock-server".into(), version: "0.0.0".into(), ..Default::default() }, // Use aliased type
                instructions: None,
            })
        }

        pub async fn close(self) -> anyhow::Result<()> {
            Ok(())
        }

        pub fn capabilities(&self) -> Option<&RmcpServerCapabilities> { // Use aliased type
            None
        }
    }
}

// In production code, use rmcp types directly
#[cfg(not(test))]
pub mod production {
    // Import necessary rmcp types using aliases from parent scope
    use crate::host::server_manager::{
        RmcpTool, RmcpCallToolResult, RmcpCallToolRequestParam, RmcpServerCapabilities,
    };
    use rmcp::service::{Peer, RoleClient};
    use serde_json::Value;
    use anyhow::anyhow;
    // Removed unused ClientCapabilities, InitializeResult imports

    // Import shared protocol objects Transport for compatibility - KEEPING FOR NOW until fully migrated
    // pub use shared_protocol_objects::rpc::Transport; // Comment out for now

    // Wrapper for McpClient to provide Debug and hold the Peer
    pub struct McpClient {
        // Store the Peer which handles communication
        inner: Peer<RoleClient>
    }
    
    // Manual Debug implementation
    impl std::fmt::Debug for McpClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("McpClient").finish()
        }
    }

    impl McpClient {
        // Constructor now takes a Peer
        pub fn new(peer: Peer<RoleClient>) -> Self {
            Self { inner: peer }
        }

        // Delegate methods to the Peer
        pub async fn list_tools(&self) -> anyhow::Result<Vec<RmcpTool>> { // Use aliased type
            log::info!("Using rmcp Peer::list_tools method");
            self.inner.list_tools(None).await
                .map(|result| result.tools) // Extract the Vec<Tool>
                .map_err(|e| anyhow!("Failed to list tools via Peer: {}", e))
        }

        pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> anyhow::Result<RmcpCallToolResult> { // Use aliased type
            log::info!("Calling tool via rmcp Peer::call_tool method: {}", name);
            let arguments_map = match args {
                Value::Object(map) => Some(map),
                Value::Null => None,
                _ => return Err(anyhow!("Tool arguments must be a JSON object or null")),
            };

            let params = RmcpCallToolRequestParam { // Use aliased type
                name: name.to_string().into(),
                arguments: arguments_map,
            };
            self.inner.call_tool(params).await
                .map_err(|e| anyhow!("Failed to call tool via Peer: {}", e))
        }

        pub async fn close(self) -> anyhow::Result<()> {
            // Peer doesn't have a close method, shutdown happens when Peer is dropped or transport closes.
            // We might need to explicitly cancel the underlying service task if needed.
            log::warn!("McpClient::close called, but Peer manages its own lifecycle. Dropping Peer.");
            // Dropping self.inner (the Peer) should trigger shutdown logic.
            Ok(())
        }

        pub fn capabilities(&self) -> Option<&RmcpServerCapabilities> { // Use aliased type
            log::warn!("McpClient::capabilities called. Capabilities should be accessed from ManagedServer after initialization.");
            None // Or retrieve from InitializeResult if stored within McpClient after init
        }

        // Remove initialize method - initialization happens via serve_client
        // pub async fn initialize(&mut self, capabilities: ClientCapabilities) -> anyhow::Result<InitializeResult> {
        //     log::info!("Initializing connection via rmcp Peer::initialize");
        //     self.inner.initialize(capabilities, None).await
        //         .map_err(|e| anyhow!("Failed to initialize Peer: {}", e))
        // }
    }

    // ProcessTransport struct is no longer needed as we use TokioChildProcess directly
    // and pass it to serve_client.

    // Remove the manual implementation of shared_protocol_objects::rpc::Transport
    // RoleClient handles the transport interaction internally.
    // The #[async_trait] and impl block are removed.
}

// For testing, use the mock implementations
#[cfg(test)]
pub use self::testing::McpClient; // Only export McpClient for testing

// For production, use the wrapped types
#[cfg(not(test))]
pub use self::production::McpClient; // Only export McpClient for production

/// Represents a server managed by MCP host
#[derive(Debug)]
pub struct ManagedServer {
    pub name: String,
    pub process: TokioChild, // Keep the process handle for killing
    pub client: McpClient, // This now wraps Peer<RoleClient> (or is the test mock)
    pub capabilities: Option<RmcpServerCapabilities>, // Use aliased type
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
    pub client_info: RmcpImplementation, // Use aliased type
    pub request_timeout: Duration,
}

impl ServerManager {
    /// Create a new ServerManager with the given parameters
    pub fn new(
        servers: Arc<Mutex<ServerMap>>, // Use ServerMap
        client_info: RmcpImplementation, // Use aliased type
        request_timeout: Duration,
    ) -> Self {
        Self {
            servers,
            client_info,
            request_timeout,
        }
    }
    
    // Removed load_config and configure methods.
    // Configuration loading and server startup orchestration are handled by MCPHost.

    /// List all available tools on the specified server
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<RmcpTool>> { // Use aliased type
        let servers = self.servers.lock().await;
        let server = servers.get(server_name)
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;

        info!("Sending tool list request to server {}", server_name);

        // Use the client's list_tools method (which delegates to rmcp's Peer)
        match server.client.list_tools().await {
            Ok(tools_vec) => {
                info!("Successfully received tools list: {} tools", tools_vec.len());
                debug!("Tools list details: {:?}", tools_vec);
                Ok(tools_vec)
            },
            Err(e) => {
                error!("Error listing tools from {}: {:?}", server_name, e);
                // Use context method from anyhow::Context trait
                Err(anyhow!("Failed to list tools from {}: {}", server_name, e)).context(format!("Listing tools failed for {}", server_name))
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
        let output = format_tool_result(&result); // Use aliased type
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

        // --- Prepare Tokio Command with Shell Wrapper ---
        let mut command = {
            #[cfg(windows)]
            {
                let mut cmd = TokioCommand::new("cmd");
                cmd.arg("/C");
                cmd.arg(program); // Add program first
                cmd.args(args);   // Then add arguments
                cmd
            }
            #[cfg(not(windows))]
            {
                let mut cmd = TokioCommand::new("sh");
                let mut command_string = format!("{} {}", program, args.join(" ")); // Simple join, may need quoting improvements later
                // Escape the command string for the shell if necessary, though simple cases might work.
                // For robustness, consider libraries like `shell-escape` or `shlex`.
                // command_string = shell_escape::escape(command_string.into()).into_owned();
                cmd.arg("-c");
                cmd.arg(command_string);
                cmd
            }
        };

        command.envs(envs) // Set environment variables
               .stdin(Stdio::piped())
               .stdout(Stdio::piped())
               .stderr(Stdio::piped()); // Capture stderr for potential debugging
        debug!("Prepared Tokio command with shell wrapper: {:?}", command);


        // --- Spawn the SINGLE Process ---
        #[cfg(not(test))]
        let (process, client, capabilities) = {
            debug!("Spawning the single process for server '{}'...", name);
            let mut child = command.spawn()
                .map_err(|e| anyhow!("Failed to spawn process for server '{}': {}", name, e))?;
            let process_id = child.id();
            info!("Single process spawned successfully for server '{}', PID: {:?}", name, process_id);

            // --- Re-approach: Spawn, get handle, THEN create transport ---
            // Spawn the process first to keep the handle
            debug!("Spawning command to get handle for server '{}'...", name);
             let mut child_for_handle = command.spawn() // Spawn once just for the handle
                 .map_err(|e| anyhow!("Failed to spawn process for handle for server '{}': {}", name, e))?;
             let process_id_handle = child_for_handle.id();
             info!("Process for handle spawned successfully for server '{}', PID: {:?}", name, process_id_handle);

            // Now create the transport using a *new* command instance (this is the one serve_client will manage)
            let mut transport_cmd = { // Create a new command instance identical to the first one
                #[cfg(windows)]
                {
                    let mut cmd = TokioCommand::new("cmd");
                    cmd.arg("/C");
                    cmd.arg(program);
                    cmd.args(args);
                    cmd
                }
                #[cfg(not(windows))]
                {
                    let mut cmd = TokioCommand::new("sh");
                    let command_string = format!("{} {}", program, args.join(" "));
                    cmd.arg("-c");
                    cmd.arg(command_string);
                    cmd
                }
            };
            transport_cmd.envs(envs)
                         .stdin(Stdio::piped())
                         .stdout(Stdio::piped())
                         .stderr(Stdio::piped()); // Ensure stderr is piped for the transport's child

            // --- Create TokioChildProcess Transport using the command ---
            // TokioChildProcess::new takes &mut Command and spawns internally
            debug!("Creating TokioChildProcess transport with new command instance for server '{}'...", name);
            let transport = TokioChildProcess::new(&mut transport_cmd) // Use ::new(&mut command)
                .map_err(|e| anyhow!("Failed to create TokioChildProcess transport from command for server '{}': {}", name, e))?;
            info!("TokioChildProcess transport created from command for server '{}'.", name);

            // --- Serve Client using TokioChildProcess Transport ---
            // serve_client takes anything implementing IntoTransport, which TokioChildProcess does.
            debug!("Serving client handler '()' with TokioChildProcess for server '{}'...", name);
            let running_service = serve_client((), transport).await // Pass the transport directly
                .map_err(|e| anyhow!("Failed to serve client using TokioChildProcess for server '{}': {}", name, e))?;
            info!("RunningService (including Peer) created using TokioChildProcess for server '{}'.", name);

            // Extract Peer and Capabilities from the running service
            let peer = running_service.peer().clone();
            // Use if let for clarity and potentially avoid compiler issue
            let capabilities = Some(running_service.peer_info().capabilities.clone());


            // --- Wrap Peer in McpClient ---
            let client = production::McpClient::new(peer);
            info!("McpClient created for server '{}'.", name);

            // Return the handle from the *first* spawn, the client, and capabilities
            (child_for_handle, client, capabilities) // Return the handle we kept
        };

        #[cfg(test)] // Keep test logic separate and simpler
        let (process,client, capabilities) = { // Removed mut from client
            // For tests, create a dummy client and process
            debug!("Creating mock process and client for test server '{}'", name);
            let process = tokio::process::Command::new("echo") // Keep dummy process
                .arg("test")
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()) // Add pipes for consistency
                .spawn()?;
            // Use the testing McpClient and MockProcessTransport
            let mut client = testing::McpClient::new(testing::create_test_transport()); // Removed mut
            // Mock initialization returns InitializeResult which contains capabilities
            let init_result = client.initialize(RmcpClientCapabilities::default()).await?; // Call mock initialize, use aliased type
            let capabilities = Some(init_result.capabilities);
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
pub fn format_tool_result(result: &RmcpCallToolResult) -> String { // Make public, use aliased type
    let mut output = String::new();
    // Handle potential error state first
    if result.is_error.unwrap_or(false) {
        output.push_str("TOOL ERROR:\n");
    }

    for content in &result.content { // content is RmcpContent
        // Match on the inner RawContent via content.raw
        match &content.raw { // Access the inner RawContent enum via .raw
            // Handle Text content - check if it's JSON
            RmcpRawContent::Text(text_content) => { // Use aliased type
                let text = &text_content.text;
                // Try to parse as JSON for pretty printing
                if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(text) {
                     match serde_json::to_string_pretty(&json_value) {
                        Ok(pretty_json) => {
                            output.push_str("```json\n");
                            output.push_str(&pretty_json);
                            output.push_str("\n```");
                        }
                        Err(_) => {
                            // Fallback to raw text if pretty printing fails
                            output.push_str(text);
                        }
                    }
                } else {
                    // Not JSON, just append the text
                    output.push_str(text);
                }
            }
            // Handle Image content - provide a placeholder
            RmcpRawContent::Image { .. } => { // Use aliased type
                output.push_str("[Image content - display not supported]");
            }
            // Handle Resource content
            RmcpRawContent::Resource { .. } => { // Use aliased type
                output.push_str("[Resource content - display not supported]");
            }
            // Removed Audio variant match as it's not in rmcp 0.1.5 RawContent
            // RmcpRawContent::Audio { .. } => { // Use aliased type
            //     output.push_str("[Audio content - display not supported]");
            // }
            // Handle other potential content types if added in the future
            // _ => { // This becomes unreachable if all variants are handled
            //     output.push_str("[Unsupported content type]");
            // }
        }
        output.push('\n');
    }
    // Trim trailing newline if present
    output.trim_end().to_string()
}

