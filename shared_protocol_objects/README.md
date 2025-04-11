# MCP Client Library

This library provides a standard JSON-RPC client implementation for the Model Context Protocol (MCP).

## Features

* Fully async implementation using Tokio
* Pluggable transport system (process, SSE, potentially TCP/WebSocket)
* Support for notifications and progress tracking
* Type-safe API for common MCP operations
* Error handling with detailed error types

## Quick Start

```rust
use shared_protocol_objects::rpc::{McpClientBuilder, ProcessTransport};
use serde_json::json;
use tokio::process::Command;
use std::time::Duration; // Added for timeout example

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a command to launch an MCP server
    let mut command = Command::new("cargo");
    command.args(["run", "--bin", "mcp_tools"]);
    
    // Create transport
    let transport = ProcessTransport::new(command).await?;

    // Build and connect client
    let client = McpClientBuilder::new(transport)
        .client_info("my-app", "1.0.0")
        .timeout(Duration::from_secs(60)) // Example: Set timeout
        .connect().await?;

    println!("Connected to server: {:?}", client.server_info());

    // List available tools
    let list_tools_result = client.list_tools().await?;
    println!("Available tools:");
    for tool in &list_tools_result.tools {
        println!(" - {}", tool.name);
    }

    // Call a tool
    let result = client.call_tool("bash", json!({
        "command": "ls -la"
    }))
    .await?;

    // Process the result
    println!("Tool result:");
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
4. **SSEClientTransport** - Implementation for Server-Sent Events communication (Client-side)
5. **SSEServerTransport** - Implementation for Server-Sent Events communication (Server-side, requires `sse_server` feature)
6. **ProgressTracker** - Helper for managing progress notifications
7. **IdGenerator** - Generates unique request IDs

## Transport System

The `Transport` trait allows implementing different communication methods:

```rust
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()>;
    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()>;
    async fn close(&self) -> Result<()>;
}

Available implementations:
*   `ProcessTransport`: Communicates with a child process via stdin/stdout.
*   `SSEClientTransport`: Communicates with an HTTP server using Server-Sent Events.

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
    RpcError {
        code: i64,
        message: String,
        data: Option<Value>,
    },
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
// Initialize with default capabilities
// let init_result = client.initialize(ClientCapabilities::default()).await?;
// Or customize capabilities:
// let capabilities = ClientCapabilities {
//     experimental: json!({ "my_feature": true }),
//     sampling: json!({}), // Assuming default derived
//     roots: RootsCapability { list_changed: true },
// };
// let init_result = client.initialize(capabilities).await?;
// println!("Server capabilities: {:?}", init_result.capabilities);
```

### Calling Tools

```rust
// Simple tool call
let result = client.call_tool("tool_name", json!({
    "param1": "value1",
    "param2": 42
})).await?;

// Tool call with progress tracking
let result = client
    .call_tool_with_progress(
        "long_running_tool",
        json!({ "duration": 30 }),
        |progress_params| {
            Box::pin(async move {
                println!(
                    "Progress ({}): {}/{} - {}",
                    progress_params.progress_token,
                    progress_params.progress,
                    progress_params.total.map(|t| t.to_string()).unwrap_or_else(|| "?".to_string()),
                    progress_params.message.as_deref().unwrap_or("")
                );
            })
        },
    )
    .await?;
```

### Working with Resources

```rust
// List resources
let list_resources_result = client.list_resources().await?;
println!("Resources: {:?}", list_resources_result.resources);

// Read a resource
let read_result = client.read_resource("file:///path/to/resource").await?;
println!("Resource content: {:?}", read_result.contents);
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
// Example using ProcessTransport
let mut command = Command::new("path/to/server");
let transport = ProcessTransport::new(command).await?;
let client = McpClientBuilder::new(transport).connect().await?;

// Example using SSEClientTransport
let transport = SSEClientTransport::new("http://localhost:3000/".to_string());
// transport.add_header("Authorization", "Bearer token"); // Optional headers
let client = McpClientBuilder::new(transport).connect().await?; // connect() will also start the SSE listener
```
