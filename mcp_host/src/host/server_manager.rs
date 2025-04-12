use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
// Removed unused import: use rmcp::RoleClient;
use serde_json::Value;
use rmcp::model::{
    Implementation as RmcpImplementation, // Alias Implementation
    Tool as RmcpTool, // Alias Tool
    CallToolResult as RmcpCallToolResult, // Alias CallToolResult
    ServerCapabilities as RmcpServerCapabilities, // Alias ServerCapabilities
    // Removed unused import: ClientCapabilities as RmcpClientCapabilities,
    // Removed unused import: InitializeResult as RmcpInitializeResult,
    CallToolRequestParam as RmcpCallToolRequestParam, // Alias CallToolRequestParam
    // Removed unused import: Content as RmcpContent,
    RawContent as RmcpRawContent, // Alias RawContent
    // Removed unused import: RawTextContent as RmcpRawTextContent,
};
use rmcp::service::{serve_client, Peer, RoleClient as RmcpRoleClient}; // Import Peer, RoleClient alias
use rmcp::transport::TokioChildProcess;
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

        // Add a method to access the inner peer
        pub fn peer(&self) -> &Peer<RoleClient> {
            &self.inner
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

    // RoleClient handles the transport interaction internally.
    // The #[async_trait] and impl block are removed.
}

// For production, use the wrapped types
#[cfg(not(test))]
pub use self::production::McpClient; // Only export McpClient for production

/// Represents a server managed by MCP host
#[derive(Debug)]
pub struct ManagedServer {
    pub name: String,
    pub process: Arc<Mutex<TokioChild>>, // Wrap process in Arc<Mutex> for killing
    pub client: Peer<RmcpRoleClient>, // Store the Peer directly
    pub capabilities: Option<RmcpServerCapabilities>, // Use aliased type
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

    /// Start a server process using detailed components.
    /// This is the core function for launching and connecting to a server.
    pub async fn start_server_with_components(
        &self,
        name: &str,
        program: &str,
        args: &[String],
        envs: &HashMap<String, String>,
    ) -> Result<()> {
        info!("Attempting to start server '{}' with program: {}, args: {:?}, envs: {:?}", name, program, args, envs.keys());

        // Check if server already exists
        {
            let servers_guard = self.servers.lock().await;
            if servers_guard.contains_key(name) {
                warn!("Server '{}' is already running.", name);
                return Ok(()); // Or return an error if preferred
            }
        } // Lock released

        // --- Spawn Process ---
        let mut tokio_command_spawn = TokioCommand::new(program);
        tokio_command_spawn.args(args)
                           .envs(envs)
                           .stdin(Stdio::piped())
                           .stdout(Stdio::piped())
                           .stderr(Stdio::piped()); // Capture stderr

        let process = match tokio_command_spawn.spawn() {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to spawn process for server '{}': {}", name, e);
                return Err(anyhow!("Failed to spawn process for server '{}': {}", name, e));
            }
        };
        let process_id = process.id();
        info!("Process spawned successfully for server '{}', PID: {:?}", name, process_id);

        // --- Create Transport and Client using rmcp ---
        // Create transport using the *original* command components
        let mut transport_cmd = TokioCommand::new(program); // Use original program path
        transport_cmd.args(args).envs(envs)
                     .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

        let transport = match TokioChildProcess::new(&mut transport_cmd) {
            Ok(t) => t,
            Ok(t) => t,
            Err(e) => {
                error!("Failed to create TokioChildProcess transport for server '{}': {}", name, e);
                // Attempt to kill the spawned process if transport creation fails
                let process_guard = Arc::new(Mutex::new(process)); // Removed mut
                if let Err(kill_err) = process_guard.lock().await.kill().await {
                     error!("Also failed to kill process for server '{}' after transport error: {}", name, kill_err);
                }
                return Err(anyhow!("Failed to create TokioChildProcess transport for server '{}': {}", name, e));
            }
        };
        info!("TokioChildProcess transport created for server '{}'.", name);

        // Serve the client handler (use `()` for default behavior)
        // Provide client info and capabilities during the serve call
        let running_service = match serve_client(
            (), // Default client handler
            transport
        ).await {
           Ok(rs) => rs,
           Err(e) => {
               error!("Failed to serve client and create Peer for server '{}': {}", name, e);
                // Attempt to kill the spawned process if serve_client fails
                let process_guard = Arc::new(Mutex::new(process)); // Removed mut
                if let Err(kill_err) = process_guard.lock().await.kill().await {
                     error!("Also failed to kill process for server '{}' after serve_client error: {}", name, kill_err);
                }
                return Err(anyhow!("Failed to serve client and create Peer for server '{}': {}", name, e));
            }
        };
        info!("RunningService (including Peer) created for server '{}'.", name);

        let client = running_service.peer().clone(); // Get the Peer
        let capabilities = running_service.peer_info()
        .capabilities.clone();

        info!("Peer<RoleClient> obtained for server '{}'.", name);

        // --- Store Managed Server ---
        let managed_server = ManagedServer {
            name: name.to_string(),
            process: Arc::new(Mutex::new(process)), // Wrap process in Arc<Mutex>
            client, // Store the Peer
            capabilities: Some(capabilities),
        };

        { // Scope for servers lock
            let mut servers_guard = self.servers.lock().await;
            servers_guard.insert(name.to_string(), managed_server);
            info!("Server '{}' added to managed servers map.", name);
        } // Lock released

        Ok(())
    }

    /// Start a server process using a command string.
    /// Parses the command string and calls `start_server_with_components`.
    pub async fn start_server(&self, name: &str, command: &str, extra_args: &[String]) -> Result<()> {
        info!("Attempting to start server '{}' with command string: '{}', extra args: {:?}", name, command, extra_args);

        // Parse the command string
        let parts = match shellwords::split(command) {
            Ok(p) => p,
            Err(e) => return Err(anyhow!("Failed to parse command string '{}': {}", command, e)),
        };

        if parts.is_empty() {
            return Err(anyhow!("Command string cannot be empty for server '{}'", name));
        }

        let program = &parts[0];
        let mut args = parts[1..].to_vec(); // Arguments from the command string
        args.extend_from_slice(extra_args); // Append extra arguments

        // Use empty environment map for now. Could inherit or load from config if needed.
        let envs = HashMap::new();

        self.start_server_with_components(name, program, &args, &envs).await
    }

    /// Stop a running server process and remove it from management.
    pub async fn stop_server(&self, name: &str) -> Result<()> {
        info!("Attempting to stop server '{}'", name);
        let mut servers_guard = self.servers.lock().await;

        if let Some(server) = servers_guard.remove(name) {
            info!("Removed server '{}' from map. Attempting to kill process...", name);
            let mut process_guard = server.process.lock().await; // Lock the Mutex around the Child
            match process_guard.kill().await {
                Ok(_) => {
                    info!("Successfully killed process for server '{}'", name);
                    // Optionally wait for the process to ensure it's fully terminated
                    // let _ = process_guard.wait().await;
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to kill process for server '{}': {}", name, e);
                    // Even if killing fails, it's removed from the map.
                    // Return an error to indicate the potential zombie process.
                    Err(anyhow!("Failed to kill process for server '{}': {}", name, e))
                }
            }
        } else {
            warn!("Server '{}' not found or already stopped.", name);
            Ok(()) // Not an error if it wasn't running
        }
    }

    /// List all available tools on the specified server
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<RmcpTool>> { // Use aliased type
        let servers = self.servers.lock().await;
        let server = servers.get(server_name)
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;

        info!("Sending tool list request to server {}", server_name);

        // Call list_tools directly on the Peer stored in ManagedServer
        match server.client.list_tools(None).await { // Pass None for default params
            Ok(list_tools_result) => {
                let tools_vec = list_tools_result.tools; // Extract Vec<Tool>
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

        // Prepare parameters for the Peer's call_tool method
        let arguments_map = match args {
            Value::Object(map) => Some(map),
            Value::Null => None,
            _ => return Err(anyhow!("Tool arguments must be a JSON object or null")),
        };
        let params = RmcpCallToolRequestParam {
            name: tool_name.to_string().into(),
            arguments: arguments_map,
        };

        // Call call_tool directly on the Peer stored in ManagedServer
        let result = server.client.call_tool(params).await
            .map_err(|e| anyhow!("Failed to call tool '{}' on server '{}': {}", tool_name, server_name, e))?;

        // Format the tool response content using rmcp::model::CallToolResult
        let output = format_tool_result(&result); // Use aliased type
        Ok(output)
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

