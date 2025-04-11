use std::sync::Arc;
use std::time::Duration;

use anyhow::Result; // Removed anyhow import
use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
// Removed unused imports: AsyncBufReadExt, AsyncWriteExt, BufReader
// Restore tokio::process::Command import
use tokio::process::Command; 
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::{
    CallToolParams, CallToolResult, CancelledParams, ClientCapabilities, GetPromptParams, // Added CallToolParams, CallToolResult, GetPromptParams
    GetPromptResult, Implementation, InitializeParams, InitializeResult, JsonRpcNotification, // Added GetPromptResult
    JsonRpcRequest, LATEST_PROTOCOL_VERSION, ListPromptsResult, ListResourcesResult, // Added ListPromptsResult, ListResourcesResult
    ListRootsResult, ListToolsResult, LogMessageParams, ProgressParams, ReadResourceParams, // Added ListRootsResult, ListToolsResult
    ResourceContent, ResourceUpdateParams, SamplingParams, // Added SamplingParams
    SamplingResult, ServerCapabilities, // Added SamplingResult
    SUPPORTED_PROTOCOL_VERSIONS,
};

use super::{IdGenerator, McpError, ProcessTransport, Transport};

/// A client for interacting with MCP servers via JSON-RPC 2.0
pub struct McpClient<T: Transport> {
    transport: T,
    client_info: Implementation,
    protocol_version: String,
    server_info: Option<Implementation>,
    server_capabilities: Option<ServerCapabilities>,
    #[allow(dead_code)] // Allow unused field for now
    request_timeout: Duration,
    initialized: bool,
    id_generator: Arc<IdGenerator>,
}

impl<T: Transport> McpClient<T> {
    /// Create a new MCP client
    pub fn new(
        transport: T,
        client_info: Implementation,
        protocol_version: Option<&str>,
        timeout: Duration,
    ) -> Self {
        Self {
            transport,
            client_info,
            protocol_version: protocol_version.unwrap_or(LATEST_PROTOCOL_VERSION).to_string(),
            server_info: None,
            server_capabilities: None,
            request_timeout: timeout,
            initialized: false,
            id_generator: Arc::new(IdGenerator::new(true)), // Use UUIDs by default for most requests
        }
    }

    /// Initialize the connection with the server
    pub async fn initialize(
        &mut self,
        capabilities: ClientCapabilities,
    ) -> Result<InitializeResult> {
        if self.initialized {
            warn!("Client already initialized");
            // If already initialized, return the stored InitializeResult data
            if let (Some(caps), Some(info)) = (&self.server_capabilities, &self.server_info) {
                 return Ok(InitializeResult {
                      protocol_version: self.protocol_version.clone(), // Use the client's known version
                      capabilities: caps.clone(),
                      server_info: info.clone(),
                      instructions: None, // Instructions might not be stored, return None or fetch if needed
                 });
            } else {
                 // Should not happen if initialized is true, but handle defensively
                 return Err(McpError::Protocol("Client initialized but server info/caps missing".to_string()).into());
            }
        }
        
        info!("Initializing MCP client with protocol version {}", self.protocol_version);
        
        if !SUPPORTED_PROTOCOL_VERSIONS.contains(&self.protocol_version.as_str()) {
            warn!("Unsupported protocol version: {}", self.protocol_version);
        }
        
        let params = InitializeParams {
            protocol_version: self.protocol_version.clone(),
            client_info: self.client_info.clone(),
            capabilities,
        };
        
        info!("Making initialize call with params: {:?}", params);
            
        // Special case for initialize - we don't check the initialized flag
        debug!("Calling call_internal for initialize...");
        let response: InitializeResult = match self.call_internal("initialize", Some(params)).await {
            Ok(result) => {
                info!("Initialize call succeeded, received InitializeResult.");
                debug!("InitializeResult: {:?}", result);
                result
            },
            Err(e) => {
                error!("Initialize call failed during call_internal: {}", e);
                return Err(e);
            }
        };

        info!(
            "Initialized with server: {} v{}",
            response.server_info.name, response.server_info.version
        );
        debug!("Storing server info and capabilities.");
        self.server_info = Some(response.server_info.clone());
        self.server_capabilities = Some(response.capabilities.clone());

        // Before sending notification, make sure we've set the flag
        info!("Setting self.initialized = true");
        self.initialized = true;
        info!("Client marked as initialized");
            
        // Send initialized notification
        info!("Attempting to send notifications/initialized notification...");
        match self.notify("notifications/initialized", None::<()>).await {
            Ok(_) => info!("Successfully sent notifications/initialized notification."),
            Err(e) => {
                // Log the specific error encountered during notify
                error!("Failed to send notifications/initialized notification: {}", e);
                // Decide if this should be a fatal error. For now, log and continue.
                // return Err(anyhow!("Failed to send critical initialized notification: {}", e));
            }
        }
        info!("Client fully initialized sequence complete.");
        Ok(response) // Return the full InitializeResult
    }

    /// Make a generic JSON-RPC call, checking initialization first
    pub async fn call<P, R>(&self, method: &str, params: Option<P>) -> Result<R>
    where
        P: Serialize + std::fmt::Debug, // Add Debug constraint for logging
        R: DeserializeOwned,
    {
        // Normal path: ensure initialized and call internal
        self.ensure_initialized(method)?;
        self.call_internal(method, params).await
    }

    /// Internal implementation of call without initialization check
    /// This is the core method for sending requests and receiving responses.
    async fn call_internal<P, R>(&self, method: &str, params: Option<P>) -> Result<R>
    where
        P: Serialize + std::fmt::Debug, // Add Debug constraint for logging
        R: DeserializeOwned,
    {
        // Always use the ID generator for requests other than initialize
        let id = if method == "initialize" {
            // Initialize should ideally have a predictable ID, e.g., 1, but let's use generator for now
            // Or handle initialize ID generation specifically if needed.
             info!("Using ID generator for initialize");
             self.id_generator.next_id()
        } else {
             info!("Using ID generator for {}", method);
             self.id_generator.next_id()
        };

        info!("Preparing call to {} with id {:?}", method, id);

        let params_value = match params {
            Some(p) => {
                // Log params only in debug builds or if trace level enabled
                trace!("Serializing params for {}: {:?}", method, p);
                match serde_json::to_value(p) {
                    Ok(value) => {
                        trace!("Params serialized successfully");
                        Some(value)
                    }
                    Err(e) => {
                        error!("Failed to serialize params for {}: {}", method, e);
                        return Err(e.into());
                    }
                }
            }
            None => None,
        };
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            method: method.to_string(),
            params: params_value,
        };
        
        info!("Sending request for method: {}", method);
        let response = match self.transport.send_request(request).await {
            Ok(resp) => {
                info!("Got response for method: {}", method);
                resp
            },
            Err(e) => {
                error!("Error sending request for method {}: {}", method, e);
                return Err(e);
            }
        };
        
        if let Some(error) = response.error {
            error!("RPC error in response: code={}, message={}", error.code, error.message);
            return Err(McpError::RpcError { 
                code: error.code, 
                message: error.message,
                data: error.data,
            }.into());
        }
        
        match response.result {
            Some(result) => {
                info!("Got result for method {}, deserializing", method);
                match serde_json::from_value(result.clone()) {
                    Ok(value) => {
                        info!("Result deserialized successfully");
                        Ok(value)
                    },
                    Err(e) => {
                        error!("Failed to deserialize result: {}, raw: {:?}", e, result);
                        Err(e.into())
                    }
                }
            },
            None => {
                error!("No result in response for method: {}", method);
                Err(McpError::NoResult.into())
            },
        }
    }
    
    /// Send a notification (no response expected)
    pub async fn notify<P>(&self, method: &str, params: Option<P>) -> Result<()>
    where
        P: Serialize + std::fmt::Debug, // Add Debug constraint for logging
    {
        // Allow 'initialized' and 'cancelled' notifications before full initialization if needed
        if method != "notifications/initialized" && method != "notifications/cancelled" {
            self.ensure_initialized(method)?;
        }

        let params_value = match params {
            Some(p) => {
                trace!("Serializing notification params for {}: {:?}", method, p);
                match serde_json::to_value(p) {
                    Ok(value) => value,
                    Err(e) => {
                         error!("Failed to serialize notification params for {}: {}", method, e);
                         return Err(e.into());
                    }
                }
            }
            None => Value::Null,
        };
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: params_value.clone(), // Clone params_value here
        };

        info!("Sending notification: {}", method);
        trace!("Notification params: {:?}", params_value); // Log params value
        debug!("Calling transport.send_notification for method: {}", method);
        let result = self.transport.send_notification(notification).await;
        match &result {
            Ok(_) => debug!("Transport successfully sent notification: {}", method),
            Err(e) => error!("Transport failed to send notification {}: {}", method, e),
        }
        result // Return the result
    }

    /// Check if client is initialized before allowing most operations
    fn ensure_initialized(&self, method: &str) -> Result<()> {
        if !self.initialized && method != "initialize" {
            Err(McpError::NotInitialized.into())
        } else {
            Ok(())
        }
    }
    
    /// Subscribe to server notifications
    pub async fn subscribe_to_notifications<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(JsonRpcNotification) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        self.transport.subscribe_to_notifications(Box::new(handler)).await
    }
    
    /// Close the connection
    pub async fn close(self) -> Result<()> {
        debug!("Closing MCP client");
        self.transport.close().await
    }

    // --- Tool Methods ---

    /// List all available tools on the server.
    pub async fn list_tools(&self) -> Result<ListToolsResult> {
        info!("Requesting tools list");
        self.call("tools/list", None::<()>).await
    }

    /// Call a tool with the given name and arguments.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult> {
        info!("Calling tool {} via tools/call", name);
        // Do not handle tools/list specially here. If the user wants to list tools,
        // they should call list_tools(). Calling tools/list via call_tool is incorrect usage.
        if name == "tools/list" {
             warn!("Attempted to call 'tools/list' using call_tool method. Use list_tools() instead.");
             return Err(McpError::Protocol("Use list_tools() method to list tools, not call_tool()".to_string()).into());
        }
        let params = CallToolParams {
            name: name.to_string(),
            arguments: arguments.clone(), // Clone arguments for the params struct
        };

        trace!("Calling tool {} with args: {:?}", name, arguments);
        self.call("tools/call", Some(params)).await
    }

    /// Call a tool with progress reporting.
    pub async fn call_tool_with_progress<F>(
        &self, 
        name: &str, 
        arguments: Value,
        progress_handler: F,
    ) -> Result<CallToolResult>
    where
        F: Fn(ProgressParams) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        let progress_token = Uuid::new_v4().to_string();

        // Add progress token to _meta field within arguments
        let mut args_with_meta = arguments.clone();
        if let Value::Object(map) = &mut args_with_meta {
            let meta = json!({ "progressToken": progress_token });
            map.insert("_meta".to_string(), meta);
        } else if args_with_meta.is_null() {
             // If args were null, create an object just for meta
             args_with_meta = json!({ "_meta": { "progressToken": progress_token } });
        } else {
             warn!("Cannot add progressToken to non-object arguments for tool {}", name);
             // Proceed without token, progress might not work
        }


        // Set up notification handler for progress updates
        let progress_handler = Arc::new(progress_handler);
        let token_clone = progress_token.clone();

        // Subscribe to notifications *before* making the call
        self.subscribe_to_notifications(move |notification| {
            let handler = Arc::clone(&progress_handler);
            let expected_token = token_clone.clone();

            Box::pin(async move {
                if notification.method == "notifications/progress" {
                    match serde_json::from_value::<ProgressParams>(notification.params) {
                        Ok(progress_params) => {
                            // Check if the token matches
                            if progress_params.progress_token == expected_token {
                                trace!("Received progress for token {}: {:?}", expected_token, progress_params);
                                handler(progress_params).await;
                            } else {
                                trace!("Ignoring progress notification for different token: {}", progress_params.progress_token);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse progress notification params: {}", e);
                        }
                    }
                }
                // Handle other notifications if necessary
            })
        })
        .await?; // Ensure subscription is set up before proceeding

        // Call the tool with the modified arguments containing the progress token
        let params = CallToolParams {
            name: name.to_string(),
            arguments: args_with_meta,
        };

        info!("Calling tool {} with progress token {}", name, progress_token);
        self.call("tools/call", Some(params)).await
        // Note: Consider unregistering the handler after the call completes or errors.
    }

    /// Send a cancellation notification for a request.
    pub async fn cancel_request(&self, request_id: Value, reason: Option<&str>) -> Result<()> {
        let params = CancelledParams {
             request_id,
             reason: reason.map(|s| s.to_string()),
        };
        info!("Sending cancellation for request ID: {:?}", params.request_id);
        self.notify("notifications/cancelled", Some(params)).await
    }

    // --- Resource Methods ---

    /// List all available resources.
    pub async fn list_resources(&self) -> Result<ListResourcesResult> {
        info!("Requesting resource list");
        self.call("resources/list", None::<()>).await
    }

    /// Read a resource by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<Vec<ResourceContent>> {
        info!("Reading resource: {}", uri);
        let params = ReadResourceParams {
            uri: uri.to_string(),
        };
        self.call("resources/read", Some(params)).await
    }

    /// Subscribe to resource updates for a given URI pattern.
    pub async fn subscribe_resources(&self, uri_pattern: &str) -> Result<()> {
         info!("Subscribing to resource updates for pattern: {}", uri_pattern);
         let params = json!({ "uri": uri_pattern });
         // Assuming subscribe returns an empty object on success
         let _result: Value = self.call("resources/subscribe", Some(params)).await?;
         Ok(())
    }

    // --- Prompt Methods ---

    /// List available prompt templates.
    pub async fn list_prompts(&self) -> Result<ListPromptsResult> {
        info!("Requesting prompt list");
        self.call("prompts/list", None::<()>).await
    }

    /// Get a specific prompt template filled with arguments.
    pub async fn get_prompt(&self, name: &str, arguments: Value) -> Result<GetPromptResult> {
        info!("Getting prompt template: {}", name);
        let params = GetPromptParams {
            name: name.to_string(),
            arguments,
        };
        self.call("prompts/get", Some(params)).await
    }

    // --- Server -> Client Methods (Handling Requests/Notifications) ---

    // These methods would typically be called by the notification handler

    /// Handle an incoming `sampling/createMessage` request from the server.
    /// This requires the client to have LLM capabilities.
    pub async fn handle_create_message_request(
         &self,
         _request_id: Value, // ID to use in the response
         _params: SamplingParams
    ) -> Result<SamplingResult> {
         error!("handle_create_message_request not implemented");
         Err(McpError::CapabilityNotSupported("sampling/createMessage".to_string()).into())
         // Implementation would involve:
         // 1. Getting user consent/confirmation.
         // 2. Calling the local LLM with params.messages and sampling settings.
         // 3. Formatting the LLM output into SamplingResult.
         // 4. Sending the SamplingResult back using sendResponse (needs transport access or callback).
    }

    /// Handle an incoming `roots/list` request from the server.
    /// This requires the client to expose local directories.
    pub async fn handle_list_roots_request(
         &self,
         _request_id: Value, // ID to use in the response
    ) -> Result<ListRootsResult> {
         error!("handle_list_roots_request not implemented");
         Err(McpError::CapabilityNotSupported("roots/list".to_string()).into())
         // Implementation would involve:
         // 1. Determining accessible root directories based on configuration/permissions.
         // 2. Formatting them into ListRootsResult.
         // 3. Sending the ListRootsResult back using sendResponse.
    }

    /// Handle an incoming `logging/setLevel` request from the server.
    pub async fn handle_set_log_level_request(
         &self,
         _request_id: Value, // ID to use in the response
         _params: Value, // Should deserialize into a SetLogLevelParams struct if defined
    ) -> Result<()> {
         warn!("handle_set_log_level_request not implemented");
         // Implementation would involve adjusting the client's internal logging level.
         // Send back an empty success response.
         Ok(())
    }

    /// Handle an incoming `notifications/message` (log message) from the server.
    pub async fn handle_log_message_notification(&self, params: LogMessageParams) {
         // Log the message received from the server using the client's logger
         match params.level.to_lowercase().as_str() {
              "error" => error!("[Server Log] {}", params.data),
              "warning" | "warn" => warn!("[Server Log] {}", params.data),
              "info" => info!("[Server Log] {}", params.data),
              "debug" | "trace" => debug!("[Server Log] {}", params.data), // Map trace to debug for simplicity
              _ => info!("[Server Log] ({}) {}", params.level, params.data),
         }
    }

    /// Handle an incoming `notifications/resources/updated` notification.
    pub async fn handle_resource_update_notification(&self, params: ResourceUpdateParams) {
         info!("Resource updated notification received for: {}", params.uri);
         // Client application logic to potentially re-read the resource or update UI would go here.
    }


    // --- Getters ---

    /// Get the server capabilities negotiated during initialization.
    pub fn capabilities(&self) -> Option<&ServerCapabilities> {
        self.server_capabilities.as_ref()
    }
    
    /// Get the server info
    pub fn server_info(&self) -> Option<&Implementation> {
        self.server_info.as_ref()
    }
    
    /// Check if the client is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    
    /// Get the protocol version being used
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }
    
    /// Get a reference to the transport
    pub fn get_transport(&self) -> &T {
        &self.transport
    }
}

/// Builder for McpClient configuration
pub struct McpClientBuilder<T: Transport> {
    transport: T,
    client_info: Implementation,
    protocol_version: String,
    timeout: Duration,
    use_uuid: bool,
}

impl<T: Transport> McpClientBuilder<T> {
    /// Create a new builder with the given transport
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            client_info: Implementation {
                name: "mcp-rust-client".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
            timeout: Duration::from_secs(300),
            use_uuid: true,
        }
    }
    
    /// Set the client info
    pub fn client_info(mut self, name: &str, version: &str) -> Self {
        self.client_info = Implementation {
            name: name.to_string(),
            version: version.to_string(),
        };
        self
    }
    
    /// Set the protocol version
    pub fn protocol_version(mut self, version: &str) -> Self {
        self.protocol_version = version.to_string();
        self
    }
    
    /// Set the request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    /// Use numeric IDs instead of UUIDs
    pub fn numeric_ids(mut self) -> Self {
        self.use_uuid = false;
        self
    }
    
    /// Build the client without initializing
    pub fn build(self) -> McpClient<T> {
        let id_generator = IdGenerator::new(self.use_uuid);
        
        McpClient {
            transport: self.transport,
            client_info: self.client_info,
            protocol_version: self.protocol_version,
            server_info: None,
            server_capabilities: None,
            request_timeout: self.timeout,
            initialized: false,
            id_generator: Arc::new(id_generator),
        }
    }
    
    /// Build and initialize the client
    pub async fn connect(self) -> Result<McpClient<T>> {
        let mut client = self.build();
        
        // Set up minimal client capabilities using defaults
        let capabilities = ClientCapabilities {
            experimental: json!({}), // Use default empty object
            sampling: json!({}),     // Use default empty object
            // Use ClientCapabilities::default() if you derive Default for it
            roots: Default::default(),
        };

        // Initialize the connection
        client.initialize(capabilities).await?;

        Ok(client)
    }
}

/// Convenience function to create a client with process transport
#[allow(dead_code)] // Allow unused function for now
// Change back to accept tokio::process::Command
pub async fn connect_to_process(command: Command) -> Result<McpClient<ProcessTransport>> { 
    let transport = ProcessTransport::new(command).await?;

    McpClientBuilder::new(transport)
        .connect()
        .await
}
