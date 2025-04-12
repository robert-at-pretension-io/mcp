use serde_json::Value;
use serde::Serialize;
use shared_protocol_objects::{
    JsonRpcRequest, JsonRpcResponse, 
    success_response, error_response
};
use anyhow::Result;

/// Mock IdGenerator for tests
pub struct IdGenerator {
    use_uuid: bool,
    counter: std::sync::atomic::AtomicI64,
}

impl IdGenerator {
    pub fn new(use_uuid: bool) -> Self {
        Self {
            use_uuid,
            counter: std::sync::atomic::AtomicI64::new(1),
        }
    }
    
    pub fn next_id(&self) -> Value {
        if self.use_uuid {
            Value::String(uuid::Uuid::new_v4().to_string())
        } else {
            let id = self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Value::Number(id.into())
        }
    }
}

/// This file provides wrappers for JSON-RPC functionality

/// Helper functions to create standard responses
pub fn create_success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    // Reimplement using rmcp types
    JsonRpcResponse {
        jsonrpc: JsonRpcVersion::V2_0,
        id: id.map(rmcp::model::NumberOrString::from_value).transpose().ok().flatten(), // Convert Value to Option<NumberOrString>
        result: Some(result),
        error: None,
    }
}

pub fn create_error_response(id: Option<Value>, code: i64, message: &str) -> JsonRpcResponse {
    // Reimplement using rmcp types
    JsonRpcResponse {
        jsonrpc: JsonRpcVersion::V2_0,
        id: id.map(rmcp::model::NumberOrString::from_value).transpose().ok().flatten(), // Convert Value to Option<NumberOrString>
        result: None,
        error: Some(JsonRpcError {
            code: code.into(), // Convert i64 to ErrorCode
            message: message.to_string(),
            data: None,
        }),
    }
}

/// Create a JSON-RPC request using the shared library's structures
pub fn create_request<P: Serialize>(method: &str, params: Option<P>, id_generator: &IdGenerator) -> Result<JsonRpcRequest> {
    let id = id_generator.next_id();

    let params_value = match params {
        Some(p) => Some(serde_json::to_value(p)?),
        None => None,
    };
    
    Ok(JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id,
        method: method.to_string(),
        params: params_value,
    })
}
