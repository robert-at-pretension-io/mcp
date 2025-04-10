use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// Export the new modules
pub mod rpc;
pub mod client;
#[cfg(feature = "examples")]
pub mod examples;

// Helper function to default Value fields to an empty object
fn default_value_object() -> Value {
    serde_json::json!({})
}

/// Core protocol version constants
pub const LATEST_PROTOCOL_VERSION: &str = "2025-03-26";
pub const SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] = ["2025-03-26", "2024-11-05", "2024-10-07"];

/// Standard JSON-RPC error codes
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
    Null,
}

impl From<RequestId> for Value {
    fn from(id: RequestId) -> Self {
        match id {
            RequestId::Number(n) => Value::Number(n.into()),
            RequestId::String(s) => Value::String(s),
            RequestId::Null => Value::Null,
        }
    }
}

/// Role enum for message senders/recipients
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Assistant,
    User,
    System,
}

/// Base JSON-RPC message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
    pub id: Value,  // Required according to JSON-RPC spec
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    // We need to always include id (not make it optional) to satisfy the Claude Desktop client
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Core protocol types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default = "default_value_object")] // Default to {} instead of null
    pub experimental: Value,
    #[serde(default = "default_value_object")] // Default to {} instead of null
    pub sampling: Value,
    #[serde(default)] // Default to RootsCapability::default()
    pub roots: RootsCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RootsCapability {
    #[serde(default)] // Add default attribute here
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsCapability {
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    pub list_changed: bool,
    pub subscribe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)] // Add default attribute here
    pub list_changed: bool,
}

/// Initialize types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: Implementation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: Implementation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>, // Added instructions field as per spec example
}

// --- Resource System ---

/// Information about a resource available on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>, // Added size field as per spec example
}

/// Represents the content of a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    // Removed blob field, not standard in spec examples for read result
}

/// Parameters for the `resources/read` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceParams {
    pub uri: String,
}

/// Result of the `resources/read` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContent>,
    // Removed _meta field, not standard in spec examples for read result
} // <-- Added missing closing brace

/// Result of the `resources/list` method.
#[derive(Debug, Clone, Serialize, Deserialize)] // Added derive
pub struct ListResourcesResult {
    pub resources: Vec<ResourceInfo>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>, // Added nextCursor for pagination as per spec
}

// --- Tool System ---

/// Information about a tool available on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, Value>>, // Added annotations field as per spec example
}

/// Represents a tool definition (used internally or for simpler cases).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    Text { text: String },
    Resource { resource: ResourceContent },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ToolResponseContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    // Removed _meta, progress, total fields. Progress is handled via notifications.
}

/// Represents a piece of content returned by a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponseContent {
    #[serde(rename = "type")]
    pub type_: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, Value>>,
}

/// Result of the `tools/list` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<ToolInfo>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>, // Added nextCursor for pagination consistency
}

// --- Prompts System ---

/// Information about a prompt template available on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<PromptArgument>,
}

/// Describes an argument required by a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

/// Result of the `prompts/list` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPromptsResult {
    pub prompts: Vec<PromptInfo>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for the `prompts/get` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptParams {
    pub name: String,
    #[serde(default = "default_value_object")]
    pub arguments: Value,
}

/// Represents a message within a prompt, typically for chat models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: Role,
    pub content: MessageContent, // Re-use MessageContent for flexibility
}

/// Result of the `prompts/get` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

// --- Sampling System ---

/// Preferences for model selection during sampling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHint {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreferences {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<ModelHint>,
    #[serde(rename = "speedPriority", skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f32>,
    #[serde(rename = "intelligencePriority", skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f32>,
    #[serde(rename = "costPriority", skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f32>,
}

/// Parameters for the `sampling/createMessage` method (server -> client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingParams {
    pub messages: Vec<PromptMessage>, // Re-use PromptMessage
    #[serde(rename = "modelPreferences", skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,
    #[serde(rename = "maxTokens", skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    // Add other sampling parameters like top_p, top_k etc. as needed
}

/// Represents different types of content within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    #[serde(rename = "text")]
    Text { text: String },
    // Add other content types like image, audio, resource as needed, matching spec examples
}

/// Result of the `sampling/createMessage` method (client -> server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingResult {
    pub role: Role, // Should typically be Role::Assistant
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>, // e.g., "endTurn", "maxTokens", "toolUse"
}

// --- Roots System ---

/// Information about a root directory exposed by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootInfo {
    pub uri: String,
    pub name: String,
}

/// Result of the `roots/list` method (client -> server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRootsResult {
    pub roots: Vec<RootInfo>,
}


// --- Helper Functions ---

/// Creates a success JSON-RPC response.
pub fn success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.unwrap_or(Value::Null),
        result: Some(result),
        error: None,
    }
}

pub fn error_response(id: Option<Value>, code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.unwrap_or(Value::Null),
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
            data: None,
        }),
    }
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
            data: None, // Add data field if needed
        }),
    }
}

/// Creates a JSON-RPC notification.
pub fn create_notification(method: &str, params: Value) -> JsonRpcNotification {
    JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
    }
}

// --- Notification Types ---

/// Represents the parameters for a `notifications/progress` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressParams {
    #[serde(rename = "progressToken")]
    pub progress_token: String, // Added progressToken as per spec
    pub progress: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Represents the parameters for a `notifications/resources/updated` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdateParams {
    pub uri: String,
}

/// Represents the parameters for a `notifications/cancelled` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelledParams {
    #[serde(rename = "requestId")]
    pub request_id: Value, // ID of the request being cancelled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Represents the parameters for a `notifications/message` (logging) notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessageParams {
    pub level: String, // e.g., "error", "warning", "info", "debug"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    pub data: String, // The log message content
}


/// Generic structure for any JSON-RPC notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

// Removed the Notification enum as it's less flexible than handling methods directly.
// Consumers will typically match on `notification.method` and deserialize `notification.params`
// into the specific parameter struct (e.g., ProgressParams, ResourceUpdateParams).
