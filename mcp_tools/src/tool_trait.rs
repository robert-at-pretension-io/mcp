use anyhow::Result;
use serde_json::Value;
use shared_protocol_objects::{CallToolParams, JsonRpcResponse};
use std::fmt::Debug;

/// Trait for implementing MCP tools
pub trait Tool: Send + Sync + Debug {
    /// Get the name of the tool
    fn name(&self) -> &str;
    
    /// Get the tool info for registration
    fn info(&self) -> shared_protocol_objects::ToolInfo;
    
    /// Execute the tool with the given parameters
    async fn execute(&self, params: CallToolParams, id: Option<Value>) -> Result<JsonRpcResponse>;
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
        _meta: None,
        progress: None,
        total: None,
    }
}
