# MCP Client Library

This library provides a standard JSON-RPC client implementation for the Model Context Protocol (MCP).

## Features

* Fully async implementation using Tokio
* Pluggable transport system (process, TCP, WebSocket)
* Support for notifications and progress tracking
* Type-safe API for common MCP operations
* Error handling with detailed error types

## Quick Start

```rust
use shared_protocol_objects::rpc::{McpClientBuilder, ProcessTransport};
use serde_json::json;
use tokio::process::Command;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a command to launch an MCP server
    let mut command = Command::new("cargo");
    command.args(["run", "--bin", "mcp_tools"]);
    
    // Create transport and connect
    let transport = ProcessTransport::new(command).await?;
    let client = McpClientBuilder::new(transport)
        .client_info("my-app", "1.0.0")
        .connect().await?;
        
    // List available tools
    let tools = client.list_tools().await?;
    println!("Available tools: {:?}", tools);
    
    // Call a tool
    let result = client.call_tool("bash", json!({
        "command": "ls -la"
    })).await?;
    
    // Process the result
    for content in result.content {
        println!("{}", content.text);
    }
    
    // Close the connection
    client.close().await?;
    
    Ok(())
}
```

## Architecture

The client is built around these key components:

1. **McpClient** - The main client interface with methods for MCP operations
2. **Transport** - A trait abstracting the communication channel
3. **ProcessTransport** - Implementation for child process communication
4. **ProgressTracker** - Helper for managing progress notifications
5. **IdGenerator** - Generates unique request IDs

## Transport System

The Transport trait allows implementing different communication methods:

```rust
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()>;
    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()>;
    async fn close(&self) -> Result<()>;
}
```

## Error Handling

Errors are handled using the custom `McpError` type which provides detailed information about what went wrong:

```rust
#[derive(Debug, Error)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(#[from] anyhow::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Request timeout")]
    Timeout,
    #[error("Client not initialized")]
    NotInitialized,
    #[error("RPC error {code}: {message}")]
    RpcError { code: i64, message: String, data: Option<Value> },
    #[error("No result in response")]
    NoResult,
    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
}
```

## Common Operations

### Initialization

```rust
// Initialize with custom capabilities
let capabilities = ClientCapabilities {
    experimental: Some(json!({ "my_feature": true })),
    sampling: Some(json!({})),
    roots: Some(RootsCapability { list_changed: true }),
};

let server_caps = client.initialize(capabilities).await?;
```

### Calling Tools

```rust
// Simple tool call
let result = client.call_tool("tool_name", json!({
    "param1": "value1",
    "param2": 42
})).await?;

// Tool call with progress tracking
let result = client.call_tool_with_progress(
    "long_running_tool", 
    json!({ "duration": 30 }),
    |progress| Box::pin(async move {
        println!("Progress: {}/{}", 
            progress.progress, 
            progress.total.unwrap_or(100));
    })
).await?;
```

### Working with Resources

```rust
// List resources
let resources = client.list_resources().await?;

// Read a resource
let contents = client.read_resource("file:///path/to/resource").await?;
```

## Extending

To add a new transport, implement the `Transport` trait:

```rust
pub struct WebSocketTransport {
    // Your fields here
}

#[async_trait]
impl Transport for WebSocketTransport {
    // Implement required methods
}
```

Then use it with the client:

```rust
let transport = WebSocketTransport::new("ws://localhost:8080").await?;
let client = McpClientBuilder::new(transport).connect().await?;
```