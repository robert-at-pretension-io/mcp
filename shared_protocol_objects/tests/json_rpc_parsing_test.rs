use shared_protocol_objects::{
    JsonRpcRequest, JsonRpcResponse, JsonRpcError, 
    PARSE_ERROR, INVALID_REQUEST, METHOD_NOT_FOUND, INVALID_PARAMS, INTERNAL_ERROR
};
use serde_json::{json, Value};

#[test]
fn test_json_rpc_request_parsing() {
    // Valid request
    let valid_request_str = r#"{
        "jsonrpc": "2.0",
        "method": "test_method",
        "params": {"key": "value"},
        "id": 1
    }"#;
    
    let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(valid_request_str);
    assert!(parsed.is_ok(), "Valid request should parse correctly");
    
    let request = parsed.unwrap();
    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.method, "test_method");
    assert_eq!(request.params, Some(json!({"key": "value"})));
    assert_eq!(request.id, json!(1));
    
    // Missing jsonrpc version
    let invalid_version = r#"{
        "method": "test_method",
        "params": {"key": "value"},
        "id": 1
    }"#;
    
    let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(invalid_version);
    assert!(parsed.is_err(), "Request without jsonrpc version should fail");
    
    // Missing method
    let missing_method = r#"{
        "jsonrpc": "2.0",
        "params": {"key": "value"},
        "id": 1
    }"#;
    
    let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(missing_method);
    assert!(parsed.is_err(), "Request without method should fail");
    
    // Missing ID
    let missing_id = r#"{
        "jsonrpc": "2.0",
        "method": "test_method",
        "params": {"key": "value"}
    }"#;
    
    let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(missing_id);
    assert!(parsed.is_err(), "Request without ID should fail");
    
    // Different ID types
    let string_id = r#"{
        "jsonrpc": "2.0",
        "method": "test_method",
        "params": {"key": "value"},
        "id": "string-id"
    }"#;
    
    let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(string_id);
    assert!(parsed.is_ok(), "Request with string ID should parse correctly");
    assert_eq!(parsed.unwrap().id, json!("string-id"));
    
    let null_id = r#"{
        "jsonrpc": "2.0",
        "method": "test_method",
        "params": {"key": "value"},
        "id": null
    }"#;
    
    let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(null_id);
    assert!(parsed.is_ok(), "Request with null ID should parse correctly");
    assert_eq!(parsed.unwrap().id, Value::Null);
}

#[test]
fn test_json_rpc_response_parsing() {
    // Successful response
    let success_response_str = r#"{
        "jsonrpc": "2.0",
        "result": {"message": "success"},
        "id": 1
    }"#;
    
    let parsed: Result<JsonRpcResponse, _> = serde_json::from_str(success_response_str);
    assert!(parsed.is_ok(), "Success response should parse correctly");
    
    let response = parsed.unwrap();
    assert_eq!(response.jsonrpc, "2.0");
    assert_eq!(response.result, Some(json!({"message": "success"})));
    assert_eq!(response.id, json!(1));
    assert!(response.error.is_none());
    
    // Error response
    let error_response_str = r#"{
        "jsonrpc": "2.0",
        "error": {
            "code": -32600,
            "message": "Invalid request"
        },
        "id": 1
    }"#;
    
    let parsed: Result<JsonRpcResponse, _> = serde_json::from_str(error_response_str);
    assert!(parsed.is_ok(), "Error response should parse correctly");
    
    let response = parsed.unwrap();
    assert_eq!(response.jsonrpc, "2.0");
    assert!(response.result.is_none());
    assert_eq!(response.id, json!(1));
    
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_REQUEST);
    assert_eq!(error.message, "Invalid request");
    
    // Can't have both result and error
    let invalid_response_str = r#"{
        "jsonrpc": "2.0",
        "result": {"message": "success"},
        "error": {
            "code": -32600,
            "message": "Invalid request"
        },
        "id": 1
    }"#;
    
    let parsed: Result<JsonRpcResponse, _> = serde_json::from_str(invalid_response_str);
    assert!(parsed.is_ok(), "Response with both result and error should parse (last field wins in serde)");
    
    // Responses must have ID
    let missing_id = r#"{
        "jsonrpc": "2.0",
        "result": {"message": "success"}
    }"#;
    
    let parsed: Result<JsonRpcResponse, _> = serde_json::from_str(missing_id);
    assert!(parsed.is_err(), "Response without ID should fail");
}

#[test]
fn test_json_rpc_errors() {
    // Test standard error codes
    assert_eq!(PARSE_ERROR, -32700);
    assert_eq!(INVALID_REQUEST, -32600);
    assert_eq!(METHOD_NOT_FOUND, -32601);
    assert_eq!(INVALID_PARAMS, -32602);
    assert_eq!(INTERNAL_ERROR, -32603);
    
    // Test error response creation
    let error = JsonRpcError {
        code: INVALID_PARAMS,
        message: "Invalid parameters".to_string(),
        data: Some(json!({"details": "Missing required field"})),
    };
    
    assert_eq!(error.code, INVALID_PARAMS);
    assert_eq!(error.message, "Invalid parameters");
    assert_eq!(error.data, Some(json!({"details": "Missing required field"})));
    
    // Test with additional data
    let error_with_data = JsonRpcError {
        code: INTERNAL_ERROR,
        message: "Server error".to_string(),
        data: Some(json!({
            "trace_id": "abc123",
            "details": "Database connection failed"
        })),
    };
    
    let expected_data = json!({
        "trace_id": "abc123",
        "details": "Database connection failed"
    });
    
    assert_eq!(error_with_data.data, Some(expected_data));
}

#[test]
fn test_request_id_parsing() {
    // Test with numeric ID
    let request_str = r#"{"jsonrpc": "2.0", "method": "test", "id": 123}"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    assert_eq!(request.id, json!(123));
    
    // Test with string ID
    let request_str = r#"{"jsonrpc": "2.0", "method": "test", "id": "abc-123"}"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    assert_eq!(request.id, json!("abc-123"));
    
    // Test with null ID
    let request_str = r#"{"jsonrpc": "2.0", "method": "test", "id": null}"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    assert_eq!(request.id, Value::Null);
    
    // Test with float ID (should be preserved, not converted to integer)
    let request_str = r#"{"jsonrpc": "2.0", "method": "test", "id": 123.45}"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    assert_eq!(request.id, json!(123.45));
}

#[test]
fn test_edge_case_request_parsing() {
    // Test with empty params
    let request_str = r#"{"jsonrpc": "2.0", "method": "test", "params": {}, "id": 1}"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    assert_eq!(request.params, Some(json!({})));
    
    // Test with array params
    let request_str = r#"{"jsonrpc": "2.0", "method": "test", "params": [1, 2, 3], "id": 1}"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    assert_eq!(request.params, Some(json!([1, 2, 3])));
    
    // Test with very long method name
    let long_method = "a".repeat(1000);
    let request_str = format!(r#"{{"jsonrpc": "2.0", "method": "{}", "id": 1}}"#, long_method);
    let request: JsonRpcRequest = serde_json::from_str(&request_str).unwrap();
    assert_eq!(request.method, long_method);
    
    // Test with Unicode characters in method
    let unicode_method = "تجربة-测试-ทดสอบ";
    let request_str = format!(r#"{{"jsonrpc": "2.0", "method": "{}", "id": 1}}"#, unicode_method);
    let request: JsonRpcRequest = serde_json::from_str(&request_str).unwrap();
    assert_eq!(request.method, unicode_method);
    
    // Test with deeply nested params
    let request_str = r#"{
        "jsonrpc": "2.0", 
        "method": "test", 
        "params": {
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "value": "deep"
                        }
                    }
                }
            }
        }, 
        "id": 1
    }"#;
    let request: JsonRpcRequest = serde_json::from_str(request_str).unwrap();
    let expected = json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "value": "deep"
                    }
                }
            }
        }
    });
    assert_eq!(request.params, Some(expected));
}