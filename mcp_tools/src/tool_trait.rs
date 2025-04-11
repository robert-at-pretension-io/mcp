use anyhow::Result;
use serde_json::Value;
use shared_protocol_objects::{CallToolParams, JsonRpcResponse};
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

/// Type alias for the async execute result
pub type ExecuteFuture = Pin<Box<dyn Future<Output = Result<JsonRpcResponse>> + Send>>;

/// Trait for implementing MCP tools
pub trait Tool: Send + Sync + Debug {
    /// Get the name of the tool
    fn name(&self) -> &str;
    
    /// Get the tool info for registration
    fn info(&self) -> shared_protocol_objects::ToolInfo;
    
    /// Execute the tool with the given parameters
    /// 
    /// This returns a boxed future instead of being an async function
    /// to make the trait object-safe.
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture;
}

/// Helper function to standardize ID handling
pub fn ensure_id(id: Option<Value>) -> Value {
    id.unwrap_or(Value::Number(1.into()))
}

/// Helper function to create a standard error response
pub fn standard_error_response(
    id: Option<Value>, 
    code: i64, 
    message: &str
) -> JsonRpcResponse {
    shared_protocol_objects::error_response(Some(ensure_id(id)), code, message)
}

/// Helper function to create a standard success response
pub fn standard_success_response(
    id: Option<Value>,
    result: Value
) -> JsonRpcResponse {
    shared_protocol_objects::success_response(Some(ensure_id(id)), result)
}

/// Helper function to create a standard tool result
pub fn standard_tool_result(
    text: String, 
    is_error: Option<bool>
) -> shared_protocol_objects::CallToolResult {
    shared_protocol_objects::CallToolResult {
        content: vec![shared_protocol_objects::ToolResponseContent {
            type_: "text".into(),
            text,
            annotations: None,
        }],
        is_error,
        // Removed _meta, progress, total fields as they don't exist in shared_protocol_objects::CallToolResult
    }
}
