# Implementing SSE as an MCP Transport

This guide provides a detailed implementation approach for Server-Sent Events (SSE) as a transport layer for the Model Context Protocol (MCP). It covers both client and server implementations with sufficient detail for a software developer to implement the code.

## 1. Overview of SSE in MCP

The SSE transport for MCP uses:
- HTTP POST requests for client-to-server communication
- Server-Sent Events for server-to-client streaming
- JSON-RPC 2.0 as the message format

### Transport Architecture

```
┌─────────────┐                  ┌─────────────┐
│             │  HTTP POST (req) │             │
│ MCP Client  │ ─────────────────▶ MCP Server  │
│             │                  │             │
│             │  SSE Stream      │             │
│             │ ◀─────────────────             │
└─────────────┘                  └─────────────┘
```

## 2. Client-Side SSE Implementation

### 2.1 SSEClientTransport Class

```rust
use anyhow::{anyhow, Result};
use reqwest::{Client as HttpClient, RequestBuilder, StatusCode};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use futures::stream::StreamExt;
use tokio::sync::mpsc::{self, Sender, Receiver};
use async_trait::async_trait;
use reqwest_eventsource::{Event, EventSource}; // You'd need to implement or find this
use crate::transport::Transport; // Your transport trait

pub struct SSEClientTransport {
    url: String,
    http_client: HttpClient,
    headers: Arc<Mutex<std::collections::HashMap<String, String>>>,
    event_stream: Arc<Mutex<Option<EventSource>>>,
    message_sender: Arc<Mutex<Option<Sender<Value>>>>,
    onmessage: Arc<Mutex<Option<Box<dyn Fn(Value) -> () + Send + Sync>>>>,
    onerror: Arc<Mutex<Option<Box<dyn Fn(anyhow::Error) -> () + Send + Sync>>>>,
    onclose: Arc<Mutex<Option<Box<dyn Fn() -> () + Send + Sync>>>>,
}

impl SSEClientTransport {
    pub fn new(url: String) -> Self {
        Self {
            url,
            http_client: HttpClient::new(),
            headers: Arc::new(Mutex::new(std::collections::HashMap::new())),
            event_stream: Arc::new(Mutex::new(None)),
            message_sender: Arc::new(Mutex::new(None)),
            onmessage: Arc::new(Mutex::new(None)),
            onerror: Arc::new(Mutex::new(None)),
            onclose: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_headers(&mut self, headers: std::collections::HashMap<String, String>) {
        let mut h = self.headers.lock().unwrap();
        *h = headers;
    }

    pub fn add_header(&mut self, key: &str, value: &str) {
        let mut h = self.headers.lock().unwrap();
        h.insert(key.to_string(), value.to_string());
    }

    async fn create_event_source(&self) -> Result<EventSource> {
        // Create an event source connection to the SSE endpoint
        let mut builder = EventSource::new(&self.url)?;
        
        // Add headers
        let headers = self.headers.lock().unwrap();
        for (k, v) in headers.iter() {
            builder = builder.header(k, v)?;
        }
        
        Ok(builder.build())
    }

    async fn send_post_request(&self, body: Value) -> Result<Value> {
        // Create an HTTP POST request
        let mut request_builder = self.http_client.post(&self.url);
        
        // Add headers
        let headers = self.headers.lock().unwrap();
        for (k, v) in headers.iter() {
            request_builder = request_builder.header(k, v);
        }
        
        // Send the request with the JSON body
        let response = request_builder
            .json(&body)
            .send()
            .await?;
        
        if response.status() != StatusCode::OK {
            return Err(anyhow!("Server responded with status: {}", response.status()));
        }
        
        // Parse the response body
        let response_body = response.json::<Value>().await?;
        Ok(response_body)
    }
}

#[async_trait]
impl Transport for SSEClientTransport {
    async fn start(&mut self) -> Result<()> {
        // Create a channel for passing received messages
        let (tx, mut rx) = mpsc::channel(100);
        *self.message_sender.lock().unwrap() = Some(tx);
        
        // Create the event source
        let event_source = self.create_event_source().await?;
        *self.event_stream.lock().unwrap() = Some(event_source.clone());
        
        // Clone references for the event handler closure
        let onmessage = self.onmessage.clone();
        let onerror = self.onerror.clone();
        
        // Spawn a task to handle incoming events
        tokio::spawn(async move {
            while let Some(event) = event_source.next().await {
                match event {
                    Ok(Event::Message(msg)) => {
                        // Parse the message as JSON
                        match serde_json::from_str::<Value>(&msg.data) {
                            Ok(json_msg) => {
                                // Send the message through the channel
                                let _ = tx.send(json_msg.clone()).await;
                                
                                // Call the onmessage handler if set
                                if let Some(handler) = onmessage.lock().unwrap().as_ref() {
                                    handler(json_msg);
                                }
                            }
                            Err(e) => {
                                if let Some(handler) = onerror.lock().unwrap().as_ref() {
                                    handler(anyhow!("Failed to parse message as JSON: {}", e));
                                }
                            }
                        }
                    }
                    Ok(Event::Open) => {
                        // Connection established
                    }
                    Err(e) => {
                        if let Some(handler) = onerror.lock().unwrap().as_ref() {
                            handler(anyhow!("EventSource error: {}", e));
                        }
                    }
                }
            }
            
            // Connection closed
            if let Some(handler) = onerror.lock().unwrap().as_ref() {
                handler(anyhow!("EventSource connection closed"));
            }
        });
        
        Ok(())
    }

    async fn send(&mut self, message: Value) -> Result<Value> {
        self.send_post_request(message).await
    }

    async fn close(&mut self) -> Result<()> {
        // Close the event stream if it exists
        if let Some(event_source) = self.event_stream.lock().unwrap().take() {
            event_source.close();
        }
        
        // Call the onclose handler if set
        if let Some(handler) = self.onclose.lock().unwrap().as_ref() {
            handler();
        }
        
        Ok(())
    }

    fn set_onmessage(&mut self, handler: Box<dyn Fn(Value) -> () + Send + Sync>) {
        *self.onmessage.lock().unwrap() = Some(handler);
    }

    fn set_onerror(&mut self, handler: Box<dyn Fn(anyhow::Error) -> () + Send + Sync>) {
        *self.onerror.lock().unwrap() = Some(handler);
    }

    fn set_onclose(&mut self, handler: Box<dyn Fn() -> () + Send + Sync>) {
        *self.onclose.lock().unwrap() = Some(handler);
    }
}
```

### 2.2 SSE Client Usage Example

```rust
async fn example_sse_client_usage() -> Result<()> {
    // Create an SSE transport
    let mut transport = SSEClientTransport::new("https://api.example.com/mcp/sse".to_string());
    
    // Add authentication headers
    transport.add_header("Authorization", "Bearer your_api_token");
    
    // Set message handler
    transport.set_onmessage(Box::new(|message| {
        println!("Received message: {}", message);
    }));
    
    // Set error handler
    transport.set_onerror(Box::new(|error| {
        eprintln!("Error: {}", error);
    }));
    
    // Start the transport
    transport.start().await?;
    
    // Create and send an initialize request
    let initialize_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "mcp-host",
                "version": "0.1.0"
            },
            "capabilities": {
                "tools": {},
                "resources": {},
                "prompts": {}
            }
        }
    });
    
    let response = transport.send(initialize_request).await?;
    println!("Initialize response: {}", response);
    
    // After initialization, send an initialized notification
    let initialized_notification = json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });
    
    let _ = transport.send(initialized_notification).await?;
    
    // Use the transport for other requests...
    
    // Close the transport when done
    transport.close().await?;
    
    Ok(())
}
```

## 3. Server-Side SSE Implementation

### 3.1 SSEServerTransport Class

```rust
use anyhow::{anyhow, Result};
use axum::{
    extract::{State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{Sse, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::trace::TraceLayer;

struct ServerState {
    message_tx: broadcast::Sender<String>,
    message_handler: Arc<Mutex<Option<Box<dyn Fn(Value) -> Result<Value> + Send + Sync>>>>,
}

pub struct SSEServerTransport {
    port: u16,
    message_tx: broadcast::Sender<String>,
    message_handler: Arc<Mutex<Option<Box<dyn Fn(Value) -> Result<Value> + Send + Sync>>>>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl SSEServerTransport {
    pub fn new(port: u16) -> Self {
        let (message_tx, _) = broadcast::channel(100);
        
        Self {
            port,
            message_tx,
            message_handler: Arc::new(Mutex::new(None)),
            server_handle: None,
        }
    }
    
    pub async fn start(&mut self) -> Result<()> {
        let state = Arc::new(ServerState {
            message_tx: self.message_tx.clone(),
            message_handler: self.message_handler.clone(),
        });
        
        // Create the router
        let app = Router::new()
            .route("/sse", get(handle_sse_request))
            .route("/", post(handle_post_request))
            .layer(TraceLayer::new_for_http())
            .with_state(state);
        
        // Start the server
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], self.port));
        println!("Starting SSE server on {}", addr);
        
        let server = axum::Server::bind(&addr)
            .serve(app.into_make_service());
        
        // Spawn the server in a separate task
        let handle = tokio::spawn(async move {
            if let Err(e) = server.await {
                eprintln!("Server error: {}", e);
            }
        });
        
        self.server_handle = Some(handle);
        Ok(())
    }
    
    pub fn send_message(&self, message: Value) -> Result<()> {
        let message_str = serde_json::to_string(&message)?;
        self.message_tx.send(message_str)?;
        Ok(())
    }
    
    pub fn set_message_handler(&mut self, handler: Box<dyn Fn(Value) -> Result<Value> + Send + Sync>) {
        *self.message_handler.lock().unwrap() = Some(handler);
    }
    
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
        Ok(())
    }
}

async fn handle_sse_request(
    State(state): State<Arc<ServerState>>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    // Subscribe to the broadcast channel
    let rx = state.message_tx.subscribe();
    let stream = BroadcastStream::new(rx).map(|msg| {
        let data = msg.unwrap_or_default();
        Ok(axum::response::sse::Event::default().data(data))
    });
    
    Sse::new(stream)
}

async fn handle_post_request(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // Process the incoming message
    if let Some(handler) = state.message_handler.lock().unwrap().as_ref() {
        match handler(body) {
            Ok(response) => {
                (StatusCode::OK, Json(response))
            }
            Err(e) => {
                let error_response = json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32603,
                        "message": format!("Internal error: {}", e)
                    },
                    "id": null
                });
                (StatusCode::INTERNAL_SERVER_ERROR, Json(error_response))
            }
        }
    } else {
        let error_response = json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32603,
                "message": "No message handler configured"
            },
            "id": null
        });
        (StatusCode::INTERNAL_SERVER_ERROR, Json(error_response))
    }
}
```

### 3.2 SSE Server Usage Example

```rust
async fn example_sse_server_usage() -> Result<()> {
    // Create and start an SSE server
    let mut server = SSEServerTransport::new(3000);
    
    // Set a message handler
    server.set_message_handler(Box::new(|message| {
        println!("Received message: {}", message);
        
        // Parse as JSON-RPC
        if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
            match method {
                "initialize" => {
                    // Handle initialize request
                    let id = message.get("id").unwrap();
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "serverInfo": {
                                "name": "example-server",
                                "version": "1.0.0"
                            },
                            "capabilities": {
                                "tools": {
                                    "tools": [
                                        {
                                            "name": "echo",
                                            "description": "Echo back the input",
                                            "inputSchema": {
                                                "type": "object",
                                                "properties": {
                                                    "message": {"type": "string"}
                                                },
                                                "required": ["message"]
                                            }
                                        }
                                    ]
                                },
                                "resources": {},
                                "prompts": {}
                            }
                        }
                    });
                    Ok(response)
                },
                "initialized" => {
                    // Handle initialized notification (no response needed)
                    Ok(json!({}))
                },
                "tools/call" => {
                    // Handle tool call
                    let id = message.get("id").unwrap();
                    let params = message.get("params").unwrap();
                    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                    let args = params.get("arguments").unwrap_or(&json!({}));
                    
                    if tool_name == "echo" {
                        let message = args.get("message").and_then(|m| m.as_str()).unwrap_or("No message provided");
                        let response = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [
                                    {
                                        "type": "text",
                                        "text": message
                                    }
                                ]
                            }
                        });
                        Ok(response)
                    } else {
                        let error_response = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": format!("Tool not found: {}", tool_name)
                            }
                        });
                        Ok(error_response)
                    }
                },
                _ => {
                    // Unknown method
                    let id = message.get("id").unwrap_or(&json!(null));
                    let error_response = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("Method not found: {}", method)
                        }
                    });
                    Ok(error_response)
                }
            }
        } else {
            // Not a valid JSON-RPC message
            Err(anyhow!("Invalid JSON-RPC message"))
        }
    }));
    
    // Start the server
    server.start().await?;
    
    // Server is now running, wait for termination signal
    println!("Server running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    
    // Stop the server
    server.stop().await?;
    
    Ok(())
}
```

## 4. Integration with MCP Client Framework

### 4.1 Creating an SSETransport Factory

```rust
pub fn create_sse_client(url: &str, api_key: Option<&str>) -> Result<Client> {
    // Create the SSE transport
    let mut transport = SSEClientTransport::new(url.to_string());
    
    // Add headers
    let mut headers = std::collections::HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    
    if let Some(key) = api_key {
        headers.insert("Authorization".to_string(), format!("Bearer {}", key));
    }
    
    transport.set_headers(headers);
    
    // Create the MCP client with this transport
    let client = Client::new(transport)?;
    
    Ok(client)
}
```

### 4.2 Using the SSE Transport in the MCP Host

```rust
impl MCPHost {
    // ... existing code ...
    
    /// Start a server using SSE transport
    pub async fn connect_to_sse_server(&self, name: &str, url: &str, api_key: Option<&str>) -> Result<()> {
        info!("Connecting to SSE server '{}' at {}", name, url);
        
        // Create the SSE transport
        let mut transport = SSEClientTransport::new(url.to_string());
        
        // Add headers
        let mut headers = std::collections::HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        
        if let Some(key) = api_key {
            headers.insert("Authorization".to_string(), format!("Bearer {}", key));
        }
        
        transport.set_headers(headers);
        
        // Start the transport
        transport.start().await?;
        
        // Create the MCP client
        #[cfg(not(test))]
        let inner_client = shared_protocol_objects::rpc::McpClientBuilder::new(transport)
            .client_info(&self.client_info.name, &self.client_info.version)
            .timeout(self.request_timeout)
            .build();
        
        #[cfg(not(test))]
        let mut client = crate::host::server_manager::production::McpClient::new(inner_client);
        
        #[cfg(test)]
        let mut client = crate::host::server_manager::testing::create_test_client();
        
        // Initialize the client
        let client_capabilities = shared_protocol_objects::ClientCapabilities {
            experimental: serde_json::json!({}),
            sampling: serde_json::json!({}),
            roots: Default::default(),
        };
        
        let init_timeout = Duration::from_secs(15);
        let init_result = tokio::time::timeout(init_timeout, client.initialize(client_capabilities)).await??;
        
        // Create a "virtual" managed server without a process
        let server = crate::host::server_manager::ManagedServer {
            name: name.to_string(),
            process: tokio::process::Command::new("echo").spawn()?, // Dummy process
            client,
            capabilities: Some(init_result.capabilities),
        };
        
        // Add the server to our list
        let mut servers = self.servers.lock().await;
        servers.insert(name.to_string(), server);
        
        info!("Successfully connected to SSE server '{}'", name);
        Ok(())
    }
}
```

## 5. SSE Transport-Specific Considerations

### 5.1 Handling Connection Interruptions

```rust
impl SSEClientTransport {
    // ... existing code ...
    
    async fn reconnect(&mut self) -> Result<()> {
        info!("Attempting to reconnect to SSE server...");
        
        // Close existing connection if any
        if let Some(event_source) = self.event_stream.lock().unwrap().take() {
            event_source.close();
        }
        
        // Create a new event source
        let event_source = self.create_event_source().await?;
        *self.event_stream.lock().unwrap() = Some(event_source.clone());
        
        // Clone references for the event handler closure
        let onmessage = self.onmessage.clone();
        let onerror = self.onerror.clone();
        let transport_clone = self.clone();
        
        // Spawn a task to handle incoming events with reconnection logic
        tokio::spawn(async move {
            while let Some(event) = event_source.next().await {
                match event {
                    // ... handle events as before ...
                }
            }
            
            // Connection closed, attempt to reconnect after a delay
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Err(e) = transport_clone.reconnect().await {
                if let Some(handler) = onerror.lock().unwrap().as_ref() {
                    handler(anyhow!("Failed to reconnect: {}", e));
                }
            }
        });
        
        Ok(())
    }
}
```

### 5.2 Implementing HTTP Keep-Alive

```rust
impl SSEClientTransport {
    // ... existing code ...
    
    fn new(url: String) -> Self {
        // Create an HTTP client with keep-alive settings
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| HttpClient::new());
            
        Self {
            url,
            http_client,
            // ... other fields
        }
    }
}
```

## 6. Testing the SSE Transport

### 6.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;
    use mock_server::MockServer; // Hypothetical mock server implementation
    
    #[test]
    fn test_sse_client_connect() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Start a mock server
            let mock_server = MockServer::start(3001).await.unwrap();
            
            // Create a client
            let mut client = SSEClientTransport::new("http://localhost:3001/sse".to_string());
            
            // Set up a test message handler
            let received_messages = Arc::new(Mutex::new(Vec::new()));
            let rm_clone = received_messages.clone();
            client.set_onmessage(Box::new(move |msg| {
                let mut messages = rm_clone.lock().unwrap();
                messages.push(msg);
            }));
            
            // Start the client
            client.start().await.unwrap();
            
            // Send a test message to the client
            let test_message = json!({
                "jsonrpc": "2.0",
                "method": "test",
                "params": { "value": "test" }
            });
            
            mock_server.send_event(test_message.to_string()).await.unwrap();
            
            // Wait a bit for message processing
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Check that we received the message
            let messages = received_messages.lock().unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0]["method"], "test");
            
            // Clean up
            client.close().await.unwrap();
            mock_server.stop().await.unwrap();
        });
    }
    
    #[test]
    fn test_sse_client_send_receive() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Similar to above, but test both sending and receiving
            let mock_server = MockServer::start(3002).await.unwrap();
            
            // Set up the mock server to respond to a specific request
            mock_server.expect_request("initialize")
                .respond_with(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "serverInfo": { "name": "test", "version": "1.0.0" },
                        "capabilities": {}
                    }
                }));
            
            // Create a client
            let mut client = SSEClientTransport::new("http://localhost:3002/sse".to_string());
            client.start().await.unwrap();
            
            // Send a request
            let response = client.send(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {}
            })).await.unwrap();
            
            // Check the response
            assert_eq!(response["result"]["serverInfo"]["name"], "test");
            
            // Clean up
            client.close().await.unwrap();
            mock_server.stop().await.unwrap();
        });
    }
}
```

### 6.2 Integration Tests

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use shared_protocol_objects::{ToolInfo, CallToolResult};
    
    #[tokio::test]
    async fn test_mcp_client_with_sse_transport() {
        // Start a real MCP server using SSE, or connect to a test server
        // This would be set up in your test environment
        
        // Create an SSE transport
        let mut transport = SSEClientTransport::new("http://localhost:3003/sse".to_string());
        
        // Start the transport
        transport.start().await.unwrap();
        
        // Create an MCP client with this transport
        let client = MCPClient::new(transport);
        
        // Initialize the client
        let init_result = client.initialize().await.unwrap();
        assert!(init_result.capabilities.tools.is_some());
        
        // List tools
        let tools = client.list_tools().await.unwrap();
        assert!(!tools.is_empty());
        
        // Call a tool
        let echo_tool = tools.iter().find(|t| t.name == "echo").unwrap();
        let result = client.call_tool("echo", json!({ "message": "Hello, world!" })).await.unwrap();
        assert_eq!(result.content[0].text, "Hello, world!");
        
        // Clean up
        client.close().await.unwrap();
    }
}
```

## 7. Performance Considerations

### 7.1 Optimizing SSE for Long-Lived Connections

```rust
impl SSEClientTransport {
    // ... existing code ...
    
    fn new_optimized(url: String) -> Self {
        // Create an HTTP client with optimized settings for long-lived connections
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(300)) // 5 minute timeout for requests
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(10)
            .tcp_keepalive(Duration::from_secs(60))
            .http2_keep_alive_interval(Duration::from_secs(30))
            .http2_keep_alive_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| HttpClient::new());
            
        Self {
            url,
            http_client,
            // ... other fields
        }
    }
    
    // Method to detect and handle stalled connections
    async fn monitor_connection_health(&self) {
        let event_stream = self.event_stream.clone();
        let onerror = self.onerror.clone();
        
        tokio::spawn(async move {
            let mut last_activity = std::time::Instant::now();
            
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                
                // Check if we've received any events recently
                let elapsed = last_activity.elapsed();
                if elapsed > Duration::from_secs(120) {
                    // No activity for 2 minutes, connection might be stalled
                    if let Some(handler) = onerror.lock().unwrap().as_ref() {
                        handler(anyhow!("Connection appears to be stalled (no activity for {} seconds)", elapsed.as_secs()));
                    }
                    
                    // Force a reconnection
                    if let Some(es) = event_stream.lock().unwrap().as_ref() {
                        es.close();
                    }
                }
            }
        });
    }
}
```

## 8. Security Considerations

### 8.1 Implementing Authentication and TLS

```rust
impl SSEClientTransport {
    // ... existing