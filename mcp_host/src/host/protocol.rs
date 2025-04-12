use serde_json::Value;
// Removed unused serde::Serialize import
// Removed shared_protocol_objects imports
use rmcp::model::{
    ErrorCode, ErrorCodeValue, ErrorData, JsonRpcError, JsonRpcResponse, JsonRpcVersion2_0, NumberOrString, WithMeta // Added ErrorCodeValue
}; // Import correct rmcp types
use rmcp::Error::ErrorCodeValue;
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
        Some(Value::Number(n)) => Some(NumberOrString::Number((u32::from(n) as i64).into())),
        Some(Value::String(s)) => Some(NumberOrString::String(s.into())),
        _ => None,
    }
}

/// Helper functions to create standard responses using rmcp structure
pub fn create_success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    // rmcp::JsonRpcResponse uses a Result for success/error
    // We need to wrap the result Value in WithMeta
    // Assuming the result Value is the payload (e.g., a Map<String, Value>)
    // let response_payload = WithMeta { payload: result, meta: BTreeMap::new() }; // Use struct literal
    let result_with_meta = WithMeta {
        _meta: None,
        inner: result,
        
    }; 
    
    JsonRpcResponse {
        jsonrpc: JsonRpcVersion2_0, // Use the unit struct
        id: value_to_id(id).unwrap(), // This expects Option<NumberOrString>, which value_to_id returns
        result: result_with_meta, // Wrap success payload in Ok
    }
}

pub fn create_error_response(id: Option<Value>, code: i64, message: &str) -> JsonRpcError {

    let error_data = ErrorData {
        code: ErrorCodeValue::Integer(code),
        message: message.to_string().into(),
        data: None,
    };

    JsonRpcError {
        jsonrpc: JsonRpcVersion2_0, // Use the unit struct
        id: value_to_id(id).unwrap(), // This expects Option<NumberOrString>, which value_to_id returns
        error: error_data, 
    }
}

