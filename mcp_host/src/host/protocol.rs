use serde_json::Value;
// Removed unused serde::Serialize import
// Removed shared_protocol_objects imports
use rmcp::model::{
    JsonRpcResponse, JsonRpcError, JsonRpcVersion2_0, NumberOrString, // Removed unused JsonRpcRequest
    ErrorCode, WithMeta, ErrorCodeValue, // Added ErrorCodeValue
}; // Import correct rmcp types
// Removed unused anyhow::Result import
use std::collections::BTreeMap; // For WithMeta metadata

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

// Helper to convert serde_json::Value ID to Option<NumberOrString>
fn value_to_id(id_value: Option<Value>) -> Option<NumberOrString> {
    match id_value {
        // Correctly map the Option<i64> to Option<NumberOrString>
        Some(Value::Number(n)) => n.as_i64().map(NumberOrString::Number),
        Some(Value::String(s)) => Some(NumberOrString::String(s.into())),
        _ => None,
    }
}

/// Helper functions to create standard responses using rmcp structure
pub fn create_success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    // rmcp::JsonRpcResponse uses a Result for success/error
    // We need to wrap the result Value in WithMeta
    // Assuming the result Value is the payload (e.g., a Map<String, Value>)
    let response_payload = WithMeta { payload: result, meta: BTreeMap::new() }; // Use struct literal
    JsonRpcResponse {
        jsonrpc: JsonRpcVersion2_0, // Use the unit struct
        id: value_to_id(id), // This expects Option<NumberOrString>, which value_to_id returns
        response: Ok(response_payload), // Wrap success payload in Ok
    }
}

pub fn create_error_response(id: Option<Value>, code: i64, message: &str) -> JsonRpcResponse {
    // Construct the rmcp::ErrorCode
    let error_code = rmcp::model::ErrorCode::Known { // Fully qualify the variant
        code: ErrorCodeValue::Integer(code), // Explicitly use Integer variant
        message: message.to_string(),
        data: None, // No additional data for now
    };
    // Wrap the error in WithMeta
    let error_payload = WithMeta { payload: JsonRpcError { error: error_code }, meta: BTreeMap::new() }; // Use struct literal
    JsonRpcResponse {
        jsonrpc: JsonRpcVersion2_0, // Use the unit struct
        id: value_to_id(id), // This expects Option<NumberOrString>, which value_to_id returns
        response: Err(error_payload), // Wrap error payload in Err
    }
}

// Remove the generic create_request function as it's incompatible with rmcp's typed requests.
// Tests needing requests should construct specific rmcp request types directly.
/*
pub fn create_request<P: Serialize>(method: &str, params: Option<P>, id_generator: &IdGenerator) -> Result<JsonRpcRequest> {
    // ... implementation removed ...
}
*/
