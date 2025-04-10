use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
// Removed unused imports: AsyncBufReadExt, AsyncWriteExt, BufReader
use tokio::process::Command;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    CallToolParams, CallToolResult, ClientCapabilities, Implementation, InitializeParams,
    InitializeResult, JsonRpcNotification, JsonRpcRequest, LATEST_PROTOCOL_VERSION, // Removed unused JsonRpcResponse
    ProgressNotification, ReadResourceParams, ReadResourceResult, ResourceContent,
    ResourceInfo, ServerCapabilities, ToolInfo, ToolResponseContent, SUPPORTED_PROTOCOL_VERSIONS,
};

use super::{IdGenerator, McpError, Transport, ProcessTransport};

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
            id_generator: Arc::new(IdGenerator::new(true)), // Use UUIDs by default
        }
    }
    
    /// Initialize the connection with the server
    pub async fn initialize(&mut self, capabilities: ClientCapabilities) -> Result<ServerCapabilities> {
        if self.initialized {
            warn!("Client already initialized");
            if let Some(caps) = &self.server_capabilities {
                return Ok(caps.clone());
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
        let response: InitializeResult = match self.call_internal("initialize", Some(params)).await {
            Ok(result) => {
                info!("Initialize call succeeded");
                result
            },
            Err(e) => {
                error!("Initialize call failed: {}", e);
                return Err(e);
            }
        };
        
        info!("Initialized with server: {} v{}", 
             response.server_info.name,
             response.server_info.version);
        
        self.server_info = Some(response.server_info.clone());
        self.server_capabilities = Some(response.capabilities.clone());
        
        // Before sending notification, make sure we've set the flag
        self.initialized = true;
        info!("Client marked as initialized");
        
        // Send initialized notification
        info!("Sending initialized notification");
        match self.notify("notifications/initialized", None::<()>).await {
            Ok(_) => info!("Initialized notification sent successfully"),
            Err(e) => {
                error!("Failed to send initialized notification: {}", e);
                // Continue anyway, some implementations might not need this notification
            }
        }
        
        info!("Client fully initialized");
        Ok(response.capabilities)
    }
    
    /// Make a generic JSON-RPC call, checking initialization first
    pub async fn call<P, R>(&self, method: &str, params: Option<P>) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        // Special handling for key methods
        if method == "tools/list" {
            info!("Special handling for tools/list in call method");
            
            // Create a direct request with specific format
            let request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: Value::Number(1.into()), // Always use explicit ID 1 for tools/list
                method: "tools/list".to_string(),
                params: None, // Always null params
            };
            
            // Send the request - get exclusive access to transport
            info!("Sending tools/list with specific format");
            let response = self.transport.send_request(request).await?;
            
            if let Some(error) = response.error {
                error!("Error in tools/list response: {}", error.message);
                return Err(anyhow!("Error in tools/list response: {}", error.message));
            }
            
            // Try to convert the response to the expected type
            if let Some(result) = response.result {
                info!("Got tools/list result, deserializing to requested type");
                match serde_json::from_value::<R>(result) {
                    Ok(value) => {
                        info!("Successfully deserialized tools/list result");
                        return Ok(value);
                    },
                    Err(e) => {
                        error!("Failed to deserialize tools/list result: {}", e);
                        return Err(e.into());
                    }
                }
            } else {
                error!("No result in tools/list response");
                return Err(anyhow!("No result in tools/list response"));
            }
        }
        else if method == "tools/call" {
            info!("Using internal method directly for tools/call to avoid ID conflicts");
            // For tools/call, use the internal method directly to avoid ID conflicts
            return self.call_internal(method, params).await;
        }
        
        // Normal path for other methods
        self.ensure_initialized(method)?;
        self.call_internal(method, params).await
    }
    
    /// Internal implementation of call without initialization check
    async fn call_internal<P, R>(&self, method: &str, params: Option<P>) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        // Generate the appropriate ID based on the method
        let id = match method {
            "tools/list" => {
                // For tools/list, always use ID 1
                info!("Using fixed ID 1 for tools/list");
                Value::Number(1.into())
            },
            "tools/call" => {
                // For tools/call, use a predictable numeric ID
                // Get the name of the tool being called
                let tool_name = match &params {
                    Some(p) => {
                        // Try to extract tool name from params
                        match serde_json::to_value(p) {
                            Ok(val) => {
                                if let Some(obj) = val.as_object() {
                                    if let Some(name) = obj.get("name") {
                                        if let Some(name_str) = name.as_str() {
                                            Some(name_str.to_owned())
                                        } else { None }
                                    } else { None }
                                } else { None }
                            },
                            Err(_) => None
                        }
                    },
                    None => None
                };
                
                // Use ID 2 for tools/call to avoid conflicting with tools/list
                info!("Using fixed ID 2 for tools/call to {}", tool_name.unwrap_or_else(|| "unknown".to_string()));
                Value::Number(2.into())
            },
            _ => {
                // For all other methods, use the ID generator
                info!("Using ID generator for {}", method);
                self.id_generator.next_id()
            }
        };
        
        info!("Preparing call to {} with id {:?}", method, id);
        
        let params_value = match params {
            Some(p) => {
                info!("Serializing params for {}", method);
                match serde_json::to_value(p) {
                    Ok(value) => {
                        info!("Params serialized successfully");
                        Some(value)
                    },
                    Err(e) => {
                        error!("Failed to serialize params: {}", e);
                        return Err(e.into());
                    }
                }
            },
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
        P: Serialize,
    {
        if method != "notifications/initialized" {
            self.ensure_initialized(method)?;
        }
        
        let params_value = match params {
            Some(p) => serde_json::to_value(p)?,
            None => Value::Null,
        };
        
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: params_value,
        };
        
        self.transport.send_notification(notification).await
    }
    
    /// Check if client is initialized
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
    
    /// List all available tools on the server
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        // Use a specially crafted request just like our direct test showed works
        info!("Creating direct tools/list request with numeric ID 1");
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::Number(1.into()), // Always use ID 1 for tools/list
            method: "tools/list".to_string(),
            params: None, // Use null params, not empty object
        };
        
        info!("Sending tools/list request to transport: {:?}", request);
        // Send the request directly to the transport
        let response = self.transport.send_request(request).await?;
        info!("Got tools/list response: {:?}", response);
        
        // Check for errors
        if let Some(error) = response.error {
            error!("RPC error in response: code={}, message={}", error.code, error.message);
            return Err(anyhow!("RPC error {}: {}", error.code, error.message));
        }
        
        // Parse the result field
        if let Some(result) = response.result {
            // Extract the tools array which should be in the form {"tools": [...]}
            if let Some(tools_obj) = result.as_object() {
                if let Some(tools_array) = tools_obj.get("tools") {
                    info!("Found tools array in response");
                    
                    // Try to parse the tools array
                    match serde_json::from_value::<Vec<ToolInfo>>(tools_array.clone()) {
                        Ok(tools) => {
                            info!("Successfully parsed {} tools", tools.len());
                            return Ok(tools);
                        }
                        Err(e) => {
                            error!("Failed to parse tools array: {}", e);
                            return Err(e.into());
                        }
                    }
                } else {
                    error!("Response contains result object but no 'tools' field: {:?}", tools_obj);
                    return Err(anyhow!("Unexpected response format: missing tools array"));
                }
            } else {
                error!("Result is not an object: {:?}", result);
                return Err(anyhow!("Unexpected response format: result is not an object"));
            }
        } else {
            error!("No result in response");
            return Err(anyhow!("No result in response"));
        }
    }
    
    
    /// Call a tool with the given name and arguments
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult> {
        // For tools/list, use the specialized method directly
        if name == "tools/list" {
            info!("Using specialized list_tools method for tools/list");
            let tools = self.list_tools().await?;
            
            // Create a synthetic CallToolResult with the tool list
            let tools_json = format!("Found {} tools: {}", tools.len(), 
                tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", "));
            
            return Ok(CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".to_string(),
                    text: tools_json,
                    annotations: None,
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None,
            });
        }
        
        // Normal path for other tools
        info!("Calling tool {} via tools/call", name);
        let params = CallToolParams {
            name: name.to_string(),
            arguments,
        };
        
        // Call through internal method to avoid cross-interaction with other requests
        self.call_internal("tools/call", Some(params)).await
    }
    
    /// Call a tool with progress reporting
    pub async fn call_tool_with_progress<F>(
        &self, 
        name: &str, 
        arguments: Value,
        progress_handler: F
    ) -> Result<CallToolResult>
    where
        F: Fn(ProgressNotification) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        let progress_token = Uuid::new_v4().to_string();
        
        // Create a clone of arguments or empty object
        let mut args_obj = match arguments {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        
        // Add _meta field with progress token
        let meta = json!({
            "progressToken": progress_token.clone(),
        });
        
        args_obj.insert("_meta".to_string(), meta);
        
        // Set up notification handler for progress updates
        // Use Arc to allow cloning the handler
        let progress_handler = Arc::new(progress_handler);
        let token_clone = progress_token.clone();
        
        self.subscribe_to_notifications(move |notification| {
            let handler = Arc::clone(&progress_handler);
            let token = token_clone.clone();
            
            Box::pin(async move {
                if notification.method == "notifications/progress" {
                    if let Ok(progress) = serde_json::from_value::<ProgressNotification>(notification.params.clone()) {
                        // The protocol doesn't actually have a progress_token field in ProgressNotification,
                        // but assuming we could access the token via params
                        if let Some(token_value) = notification.params.get("progressToken") {
                            if let Some(notif_token) = token_value.as_str() {
                                if notif_token == token {
                                    handler(progress).await;
                                }
                            }
                        }
                    }
                }
            })
        }).await?;
        
        // Call the tool with progress token
        let params = CallToolParams {
            name: name.to_string(),
            arguments: Value::Object(args_obj),
        };
        
        self.call("tools/call", Some(params)).await
    }
    
    /// Cancel an in-flight tool call
    pub async fn cancel_tool_call(&self, request_id: &str, reason: Option<&str>) -> Result<()> {
        let params = json!({
            "requestId": request_id,
            "reason": reason.unwrap_or("User cancelled operation"),
        });
        
        self.notify("notifications/cancelled", Some(params)).await
    }
    
    /// List all available resources
    pub async fn list_resources(&self) -> Result<Vec<ResourceInfo>> {
        let result: Result<Value> = self.call("resources/list", None::<()>).await;
        
        match result {
            Ok(value) => {
                if let Some(resources) = value.get("resources") {
                    if let Ok(res_list) = serde_json::from_value::<Vec<ResourceInfo>>(resources.clone()) {
                        return Ok(res_list);
                    }
                }
                Err(anyhow!("Invalid resources response format"))
            }
            Err(e) => Err(e),
        }
    }
    
    /// Read a resource by URI
    pub async fn read_resource(&self, uri: &str) -> Result<Vec<ResourceContent>> {
        let params = ReadResourceParams {
            uri: uri.to_string(),
        };
        
        let result: ReadResourceResult = self.call("resources/read", Some(params)).await?;
        Ok(result.contents)
    }
    
    /// Get the server capabilities
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
        
        // Set up minimal client capabilities
        let capabilities = ClientCapabilities {
            experimental: None,
            sampling: None,
            roots: None,
        };
        
        // Initialize the connection
        client.initialize(capabilities).await?;
        
        Ok(client)
    }
}

/// Convenience function to create a client with process transport
#[allow(dead_code)] // Allow unused function for now
pub async fn connect_to_process(command: Command) -> Result<McpClient<ProcessTransport>> {
    let transport = ProcessTransport::new(command).await?;

    McpClientBuilder::new(transport)
        .connect()
        .await
}
