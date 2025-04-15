Model Context Protocol (`rmcp`) Client Implementation Guide
1. Protocol Specifications (`rmcp::model`)
Core Protocol Definitions
The Model Context Protocol uses JSON-RPC 2.0 as its base protocol. Key `rmcp` components include:
Protocol Versions (`rmcp::model::ProtocolVersion`)
```rust
pub const V_2025_03_26: Self = Self(Cow::Borrowed("2025-03-26"));
pub const V_2024_11_05: Self = Self(Cow::Borrowed("2024-11-05"));
pub const LATEST: Self = Self::V_2025_03_26;
```
Message Structure (`rmcp::model::JsonRpc*`)
All messages follow the JSON-RPC 2.0 format:

*   `JsonRpcRequest`: For client/server requests.
*   `JsonRpcResponse`: For server/client responses.
*   `JsonRpcNotification`: For notifications.
*   `JsonRpcError`: For error responses.

Request and Response Identifiers (`rmcp::model::RequestId`)
Requests and responses are correlated using `RequestId`, which wraps `rmcp::model::NumberOrString`.
Message Types and Schemas
The SDK defines specific request, response, and notification types (e.g., `InitializeRequest`, `InitializeResult`, `CallToolRequest`, `CallToolResult`, `CancelledNotification`). These are typically used internally by the `Peer` but can be referenced.
Protocol Negotiation (`InitializeRequest`, `InitializeResult`)
Protocol version negotiation occurs during the initialization handled by `serve_client`/`serve_server`.

*   Client sends `InitializeRequest` (handled by `serve_client`).
*   Server responds with `InitializeResult` (handled by `serve_server`).
*   Client confirms with `InitializedNotification` (handled by `serve_client`).

```rust
// Relevant structs (usually handled internally by serve_*)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequestParam {
    pub protocol_version: ProtocolVersion,
    pub capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: ProtocolVersion,
    pub capabilities: ServerCapabilities,
    pub server_info: Implementation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}
```
2. Client Implementation Details (`rmcp::service`, `rmcp::handler::client`)
Initialization Sequence
Client initialization is simplified using `serve_client`:

1.  Create a transport (e.g., `TokioChildProcess`).
2.  Create a client handler (can be `()` for default behavior, or implement `ClientHandler` for custom notification handling).
3.  Call `serve_client(handler, transport).await`. This handles the handshake.
4.  The result is a `RunningService`, from which you get the `Peer` for interaction.

Example:
```rust
use rmcp::{
    service::{serve_client, RoleClient},
    transport::child_process::TokioChildProcess,
    ServiceExt, // Required for .serve() if using that pattern
};
use tokio::process::Command;

let mut cmd = Command::new("mcp-server-executable");
let transport = TokioChildProcess::new(&mut cmd)?;
let client_handler = (); // Use default handler

// Initialize and get the running service
let running_service = serve_client(client_handler, transport).await?;

// Get the peer for communication
let peer = running_service.peer();

// Get server info (available after initialization)
if let Some(server_info) = running_service.peer_info() {
    println!("Connected to: {} {}", server_info.server_info.name, server_info.server_info.version);
}

// Use the peer to make requests
let tools = peer.list_tools(None).await?;
```
Required Parameters
Client information (`client_info: rmcp::model::Implementation`) and capabilities (`capabilities: rmcp::model::ClientCapabilities`) are provided during the `serve_client` call implicitly or explicitly depending on the handler. The default `()` handler uses default capabilities.
Authentication
The protocol itself doesn't specify authentication. Implementations typically use:

*   API keys in HTTP headers (for HTTP/SSE/WebSocket transports).
*   Environment variables (for child process transports).
*   OS security (for local sockets).

Error Handling (`rmcp::Error`, `rmcp::ServiceError`)
Errors generally conform to JSON-RPC standards. The SDK uses:

*   `rmcp::Error`: For general SDK errors.
*   `rmcp::ServiceError`: For errors during service operation (returned by `Peer` methods).

Error responses include `code`, `message`, and optional `data`.
3. Transport Layer (`rmcp::transport`)
Supported Transports
The SDK provides several transport implementations:

*   **Standard I/O:** `rmcp::transport::stdio()` - For communication via stdin/stdout.
*   **Child Process:** `rmcp::transport::child_process::TokioChildProcess` - Spawns and communicates with an external process via stdio.
*   **HTTP/SSE:** `rmcp::transport::sse::SseTransport` (client), `rmcp::transport::sse::SseServer` (server).
*   **WebSockets:** (May require external crates like `tokio-tungstenite` and implementing `IntoTransport`).

Message Framing
Transports handle JSON serialization/deserialization and newline delimiting. The core abstraction is the `IntoTransport` trait:
```rust
pub trait IntoTransport<R, E, A>: Send + 'static
where
    R: ServiceRole, // RoleClient or RoleServer
    E: std::error::Error + Send + 'static,
{
    fn into_transport(
        self,
    ) -> (
        // Sink for sending messages
        impl Sink<TxJsonRpcMessage<R>, Error = E> + Send + 'static,
        // Stream for receiving messages
        impl Stream<Item = RxJsonRpcMessage<R>> + Send + 'static,
    );
}
```
Reconnection Strategies
The SDK itself doesn't automatically handle reconnection. Client applications need to:

1.  Detect transport failures (e.g., errors from `Peer` methods or the `running_service.waiting().await` future).
2.  Potentially cancel the `RunningService` task.
3.  Re-establish the transport.
4.  Call `serve_client` again to re-initialize the connection.

4. Message Types (`rmcp::model`)
Content Types (`RawContent`, `Content`)
Messages can contain different content types, defined in the `RawContent` enum and wrapped by `Content`:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RawContent {
    Text(RawTextContent),
    Image(RawImageContent),
    Resource(RawEmbeddedResource),
    Audio(AudioContent),
}
```
Request/Response Pairs
Key interactions are handled via `Peer` methods:

*   **Initialization:** Handled by `serve_client`/`serve_server`.
*   **Tool Invocation:** `peer.call_tool(CallToolRequestParam) -> Result<CallToolResult, ServiceError>`
*   **Resource Management:** `peer.list_resources(...)`, `peer.read_resource(...)`
*   **Message Generation:** `peer.create_message(...)` (Server-side)

Notification Types
Notifications are used for asynchronous events. Clients might receive:

*   `CancelledNotification`: A request was cancelled by the server.
*   `ProgressNotification`: Progress update for a server operation.
*   `ResourceUpdatedNotification`, `ResourceListChangedNotification`: Resource changes.
*   `ToolListChangedNotification`: Available tools changed.

Clients handle these by implementing the `ClientHandler` trait.
Progress Reporting (`ProgressNotificationParam`)
Servers send `ProgressNotification` to report progress on long-running tasks initiated by the client.
```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotificationParam {
    pub progress_token: ProgressToken, // Provided by client in original request
    pub progress: u32,
    // ... other fields (total, message)
}
```
Cancellation Protocol (`CancelledNotificationParam`)
Either side can send a `CancelledNotification` to cancel an ongoing request. The `Peer` handles sending cancellations for client-initiated requests via the `RequestHandle`.
```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CancelledNotificationParam {
    pub request_id: RequestId,
    pub reason: Option<String>,
}
```
5. Tool Integration (`rmcp::model::Tool`, `rmcp::handler::server`)
Tool Definition (`rmcp::model::Tool`)
Tools are defined by the server and listed via `peer.list_tools()`. The client receives `rmcp::model::Tool` structs:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Cow<'static, str>>,
    // Schema for the 'arguments' object in CallToolRequestParam
    pub input_schema: Arc<JsonObject>, // JsonObject = Map<String, Value>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}
```
Tool Invocation (`CallToolRequestParam`, `CallToolResult`)
Clients invoke tools using `peer.call_tool()`:
```rust
use rmcp::{
    model::{CallToolRequestParam, CallToolResult},
    service::{Peer, RoleClient, ServiceError},
};
use serde_json::json;

async fn invoke_my_tool(peer: &Peer<RoleClient>) -> Result<CallToolResult, ServiceError> {
    let params = CallToolRequestParam {
        name: "my_tool_name".into(),
        // Arguments must be a JSON object (or None)
        arguments: Some(json!({ "arg1": "value1", "arg2": 123 }).as_object().unwrap().clone()),
    };
    peer.call_tool(params).await
}
```
Tool Response Format (`CallToolResult`)
The response contains the tool's output:
```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    // Vector of content parts (text, image, etc.)
    pub content: Vec<Content>,
    // Optional flag indicating if the result represents an error from the tool's perspective
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}
```
Practical Implementation Steps (Client)
1.  **Choose Transport:** Select an appropriate transport (e.g., `TokioChildProcess` for a local server).
2.  **Define Handler:** Use `()` for default behavior or implement `ClientHandler` for custom notification handling.
    ```rust
    use rmcp::{handler::client::ClientHandler, service::{Peer, RoleClient}};
    use std::future::Future;

    struct MyClientHandler; // Your custom handler state

    impl ClientHandler for MyClientHandler {
        // Implement methods like on_tool_list_changed, on_progress, etc.
        fn on_tool_list_changed(&self) -> impl Future<Output = ()> + Send + '_ {
            async move {
                println!("Server tool list changed!");
                // Potentially call peer.list_tools() to refresh
            }
        }
        // ... other handlers
    }
    ```
3.  **Initialize:** Call `serve_client(handler, transport).await`.
    ```rust
    let running_service = serve_client(MyClientHandler, transport).await?;
    let peer = running_service.peer();
    ```
4.  **Interact:** Use the `peer` object to call methods like `list_tools`, `call_tool`.
    ```rust
    let tools = peer.list_tools(None).await?;
    let result = peer.call_tool(/* ... */).await?;
    ```
5.  **Handle Lifecycle:** Monitor `running_service.waiting().await` for completion or errors. Cancel the task if needed.

Conclusion
The `rmcp` SDK provides the necessary components to build robust MCP clients. By leveraging `serve_client`, `Peer`, and the defined model types, developers can interact with MCP servers over various transports, invoke tools, and handle server-sent notifications in a standardized way.
