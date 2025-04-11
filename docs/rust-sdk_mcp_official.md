Model Context Protocol (MCP) Client Implementation Guide
1. Protocol Specifications
Core Protocol Definitions
The Model Context Protocol uses JSON-RPC 2.0 as its base protocol with specific message types and structures. The core protocol is defined in the following key components:
Protocol Versions
rustpub const V_2025_03_26: Self = Self(Cow::Borrowed("2025-03-26"));
pub const V_2024_11_05: Self = Self(Cow::Borrowed("2024-11-05"));
pub const LATEST: Self = Self::V_2025_03_26;
Message Structure
All messages follow the JSON-RPC 2.0 format with these primary types:

JsonRpcRequest: For client requests
JsonRpcResponse: For server responses
JsonRpcNotification: For notifications (events without responses)
JsonRpcError: For error responses

Request and Response Identifiers
Requests and responses are correlated using RequestId, which can be either:

A number (NumberOrString::Number(u32))
A string (NumberOrString::String(Arc<str>))

Message Types and Schemas
The protocol supports several message types, organized as client/server requests, responses, and notifications:
Client Requests:
rustexport type ClientRequest =
    | PingRequest
    | InitializeRequest
    | CompleteRequest
    | SetLevelRequest
    | GetPromptRequest
    | ListPromptsRequest
    | ListResourcesRequest
    | ListResourceTemplatesRequest
    | ReadResourceRequest
    | SubscribeRequest
    | UnsubscribeRequest
    | CallToolRequest
    | ListToolsRequest;
Server Requests:
rustexport type ServerRequest =
    | PingRequest
    | CreateMessageRequest
    | ListRootsRequest;
Client Notifications:
rustexport type ClientNotification =
    | CancelledNotification
    | ProgressNotification
    | InitializedNotification
    | RootsListChangedNotification;
Server Notifications:
rustexport type ServerNotification =
    | CancelledNotification
    | ProgressNotification
    | LoggingMessageNotification
    | ResourceUpdatedNotification
    | ResourceListChangedNotification
    | ToolListChangedNotification
    | PromptListChangedNotification;
Protocol Negotiation
Protocol version negotiation occurs during initialization:

Client sends an InitializeRequest with its supported protocol version and capabilities
Server responds with an InitializeResult containing its protocol version and capabilities
Client confirms with an InitializedNotification

rust#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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
2. Client Implementation Details
Initialization Sequence
Client initialization follows these steps:

Create a transport layer (WebSockets, stdio, etc.)
Create a client handler that implements the ClientHandler trait
Send an InitializeRequest to establish capabilities
Wait for the server's InitializeResult
Send InitializedNotification to confirm readiness

Example:
rustlet service = ()
    .serve(TokioChildProcess::new(Command::new("uvx").arg("mcp-server-git"))?)
    .await?;

// Server info is available after initialization
let server_info = service.peer_info();
Required Parameters
The client must provide these parameters during initialization:

protocol_version: The MCP version to use (e.g., "2025-03-26")
capabilities: What features the client supports
client_info: Client name and version

rust// Example of setting up client capabilities
let capabilities = ClientCapabilities::builder()
    .enable_experimental()
    .enable_roots()
    .enable_roots_list_changed()
    .build();
Authentication
The protocol itself doesn't specify authentication methods, but implementations typically use:

API keys in HTTP headers for HTTP/WebSocket transports
Environment variables for process-based transports
Standard OS security for local socket transports

Error Handling
Errors follow JSON-RPC standard error codes with additional MCP-specific codes:
rustpub const RESOURCE_NOT_FOUND: Self = Self(-32002);
pub const INVALID_REQUEST: Self = Self(-32600);
pub const METHOD_NOT_FOUND: Self = Self(-32601);
pub const INVALID_PARAMS: Self = Self(-32602);
pub const INTERNAL_ERROR: Self = Self(-32603);
pub const PARSE_ERROR: Self = Self(-32700);
Error responses include:

code: Numeric error code
message: Human-readable error message
data: Optional additional error details

3. Transport Layer
Supported Transports
The SDK supports multiple transport mechanisms:

Standard I/O:

Uses tokio::io::stdin/stdout for process-based communication
Suitable for child processes and embedding


Child Process:

TokioChildProcess for spawning and communicating with external processes
Handles standard I/O communication with spawned processes


HTTP/SSE (Server-Sent Events):

SseTransport for client-side connections
SseServer for server-side implementations
Uses HTTP-based event streaming


WebSockets (referenced in code but implementation details not fully examined)

Message Framing
Messages are encoded as newline-delimited JSON. The transport layer handles:

Serializing messages to JSON
Adding newline delimiters
Parsing incoming messages from JSON-RPC format

The IntoTransport trait defines how different transports can be used:
rustpub trait IntoTransport<R, E, A>: Send + 'static
where
    R: ServiceRole,
    E: std::error::Error + Send + 'static,
{
    fn into_transport(
        self,
    ) -> (
        impl Sink<TxJsonRpcMessage<R>, Error = E> + Send + 'static,
        impl Stream<Item = RxJsonRpcMessage<R>> + Send + 'static,
    );
}
Reconnection Strategies
The SDK doesn't explicitly handle reconnection. Clients are expected to:

Detect transport failures through errors
Close the existing connection
Re-establish a new connection and re-initialize

4. Message Types
Content Types
Messages can contain different content types:
rust#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RawContent {
    Text(RawTextContent),
    Image(RawImageContent),
    Resource(RawEmbeddedResource),
    Audio(AudioContent),
}
Request/Response Pairs
Key request/response pairs include:

Initialization:

Request: InitializeRequest
Response: InitializeResult


Tool Invocation:

Request: CallToolRequest
Response: CallToolResult


Resource Management:

Request: ListResourcesRequest
Response: ListResourcesResult
Request: ReadResourceRequest
Response: ReadResourceResult


Message Generation:

Request: CreateMessageRequest
Response: CreateMessageResult



Notification Types
Notifications are used for asynchronous events:

Cancellation: CancelledNotification - For stopping in-progress requests
Progress: ProgressNotification - For reporting progress on long-running operations
Resource Updates: ResourceUpdatedNotification, ResourceListChangedNotification - For resource changes
Tool Updates: ToolListChangedNotification - When available tools change

Progress Reporting
Progress is reported through ProgressNotification:
rust#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotificationParam {
    pub progress_token: ProgressToken,
    pub progress: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
Cancellation Protocol
Cancellation is handled through CancelledNotification:
rust#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CancelledNotificationParam {
    pub request_id: RequestId,
    pub reason: Option<String>,
}
5. Tool Integration
Tool Definition
Tools are defined with:

Name
Description
Input schema (JSON Schema)
Optional annotations about behavior

rust#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Cow<'static, str>>,
    pub input_schema: Arc<JsonObject>,
    pub annotations: Option<ToolAnnotations>,
}
Tool Registration
Tools can be registered using macros:
rust#[tool(tool_box)]
impl Calculator {
    #[tool(description = "Calculate the sum of two numbers")]
    async fn sum(&self, #[tool(aggr)] SumRequest { a, b }: SumRequest) -> String {
        (a + b).to_string()
    }
}
Tool Invocation
Tools are invoked using the CallToolRequest:
rustlet tool_result = service
    .call_tool(CallToolRequestParam {
        name: "git_status".into(),
        arguments: serde_json::json!({ "repo_path": "." }).as_object().cloned(),
    })
    .await?;
Tool Response Format
Tool responses are returned in CallToolResult:
rust#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}
Practical Implementation Steps
To implement an MCP client with your own transport layer:

Set up the transport:
rust// For a custom transport, implement the IntoTransport trait
struct MyTransport { /* ... */ }

impl IntoTransport<RoleClient, MyError, ()> for MyTransport {
    fn into_transport(self) -> (
        impl Sink<ClientJsonRpcMessage, Error = MyError> + Send + 'static,
        impl Stream<Item = ServerJsonRpcMessage> + Send + 'static,
    ) {
        // Split your transport into message sink and stream
    }
}

Create a client handler:
ruststruct MyClient {
    peer: Option<Peer<RoleClient>>,
}

impl ClientHandler for MyClient {
    fn get_peer(&self) -> Option<Peer<RoleClient>> {
        self.peer.clone()
    }
    
    fn set_peer(&mut self, peer: Peer<RoleClient>) {
        self.peer = Some(peer);
    }
    
    // Implement handlers for server requests if needed
}

Initialize the connection:
rustlet my_transport = MyTransport::new(/* ... */);
let client = MyClient { peer: None };
let service = client.serve(my_transport).await?;

// Now you can use service to interact with the server
let tools = service.list_tools(Default::default()).await?;

Invoke tools:
rustlet result = service.call_tool(CallToolRequestParam {
    name: "tool_name".into(),
    arguments: Some(serde_json::json!({ 
        "param1": "value1",
        "param2": 42
    }).as_object().unwrap().clone()),
}).await?;

Handle server notifications:
rustimpl ClientHandler for MyClient {
    // ...
    
    fn on_tool_list_changed(&self) -> impl Future<Output = ()> + Send + '_ {
        async move {
            // Tool list changed, may want to refresh your tool cache
            if let Some(peer) = &self.peer {
                let tools = peer.list_tools(Default::default()).await.unwrap();
                // Update your tools
            }
        }
    }
}


Conclusion
The Model Context Protocol provides a standardized way for AI systems to interact with external tools and resources. By implementing an MCP client, you can connect your application to any MCP-compatible server, enabling tool usage, resource access, and message generation in a consistent manner.
The protocol's flexibility in transport mechanisms means you can implement it over various communication channels, while the structured message formats ensure compatibility across different implementations.RetryClaude does not have the ability to run the code it generates yet.Claude can make mistakes. Please double-check responses.7 3.7 Sonnet