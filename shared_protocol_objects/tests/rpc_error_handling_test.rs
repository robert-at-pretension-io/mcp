use serde_json::{json, Value};
use tokio::test;

use shared_protocol_objects::{
    JsonRpcResponse, JsonRpcError,
    error_response, success_response,
    PARSE_ERROR, INVALID_REQUEST, METHOD_NOT_FOUND, INVALID_PARAMS, INTERNAL_ERROR,
};

#[test]
async fn test_standard_error_codes() {
    // Verify standard JSON-RPC error codes
    assert_eq!(PARSE_ERROR, -32700);
    assert_eq!(INVALID_REQUEST, -32600);
    assert_eq!(METHOD_NOT_FOUND, -32601);
    assert_eq!(INVALID_PARAMS, -32602);
    assert_eq!(INTERNAL_ERROR, -32603);
}

#[test]
async fn test_error_response_creation() {
    // Create an error response with the helper function
    let id = json!(1);
    let error_resp = error_response(Some(id.clone()), INVALID_PARAMS, "Missing required parameter 'name'");
    
    // Verify the response structure
    assert_eq!(error_resp.jsonrpc, "2.0");
    assert_eq!(error_resp.id, id);
    assert!(error_resp.result.is_none(), "Result should be None in error response");
    
    // Verify the error object
    let error = error_resp.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert_eq!(error.message, "Missing required parameter 'name'");
    assert!(error.data.is_none(), "Default error has no data");
}

#[test]
async fn test_error_with_data() {
    // Create an error with additional data
    let error = JsonRpcError {
        code: INTERNAL_ERROR,
        message: "Database connection failed".to_string(),
        data: Some(json!({
            "server": "db1",
            "timestamp": 1619734221,
            "retry_after": 30
        })),
    };
    
    // Create a response with this error
    let resp = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: json!(5),
        result: None,
        error: Some(error),
    };
    
    // Verify the error data
    let error_data = resp.error.unwrap().data.unwrap();
    assert_eq!(error_data.get("server").unwrap(), "db1");
    assert_eq!(error_data.get("timestamp").unwrap(), 1619734221);
    assert_eq!(error_data.get("retry_after").unwrap(), 30);
}

#[test]
async fn test_error_response_serialization() {
    // Create an error response
    let error_resp = error_response(Some(json!("request-123")), METHOD_NOT_FOUND, "Method 'unknown' not found");
    
    // Serialize to JSON
    let json_str = serde_json::to_string(&error_resp).unwrap();
    
    // Verify the JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["id"], "request-123");
    assert!(parsed.get("result").is_none(), "Result field should not be present");
    assert_eq!(parsed["error"]["code"], -32601);
    assert_eq!(parsed["error"]["message"], "Method 'unknown' not found");
}

#[test]
async fn test_null_id_handling() {
    // Create an error response with null id (for parse errors)
    let error_resp = error_response(None, PARSE_ERROR, "Invalid JSON");
    
    // Verify null ID is handled correctly
    assert_eq!(error_resp.id, Value::Null);
    
    // Serialize to JSON
    let json_str = serde_json::to_string(&error_resp).unwrap();
    
    // Verify the JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["id"], Value::Null);
}

#[test]
async fn test_success_and_error_distinction() {
    // Create a success response
    let success_resp = success_response(Some(json!(123)), json!({
        "result": "Operation completed",
        "count": 5
    }));
    
    // Create an error response
    let error_resp = error_response(Some(json!(123)), INTERNAL_ERROR, "Operation failed");
    
    // Verify success response has result but no error
    assert!(success_resp.result.is_some());
    assert!(success_resp.error.is_none());
    
    // Verify error response has error but no result
    assert!(error_resp.error.is_some());
    assert!(error_resp.result.is_none());
    
    // Test serialization
    let success_json = serde_json::to_string(&success_resp).unwrap();
    let error_json = serde_json::to_string(&error_resp).unwrap();
    
    // Success response should have "result" field but no "error" field
    assert!(success_json.contains("\"result\""));
    assert!(!success_json.contains("\"error\""));
    
    // Error response should have "error" field but no "result" field
    assert!(error_json.contains("\"error\""));
    assert!(!error_json.contains("\"result\""));
}

#[test]
async fn test_custom_error_codes() {
    // Server-defined error codes should be between -32000 and -32099
    let server_error_code = -32050;
    
    // Create a custom error response
    let error_resp = error_response(
        Some(json!(1)),
        server_error_code,
        "Custom server error: resource limit exceeded"
    );
    
    // Verify the error code
    assert_eq!(error_resp.error.unwrap().code, server_error_code);
    
    // Create another with application-specific error code (outside the reserved range)
    let app_error_code = -10001;
    let error_resp = error_response(
        Some(json!(2)),
        app_error_code,
        "Application error: invalid configuration"
    );
    
    // Verify the error code
    assert_eq!(error_resp.error.unwrap().code, app_error_code);
}