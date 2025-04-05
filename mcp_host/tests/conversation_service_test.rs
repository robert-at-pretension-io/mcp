use mcp_host::conversation_service::parse_json_response;

// Test 1: Basic choice parsing for direct response
#[tokio::test]
async fn test_parse_json_response_finish_choice() {
    let response = r#"{"choice": "finish_response"}"#;
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (choice, args) = result.unwrap();
    assert_eq!(choice, "finish_response");
    assert!(args.is_none());
}

// Test 2: Basic choice parsing for tool call
#[tokio::test]
async fn test_parse_json_response_tool_choice() {
    let response = r#"{"choice": "tool_call"}"#;
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (choice, args) = result.unwrap();
    assert_eq!(choice, "tool_call");
    assert!(args.is_none());
}

// Test 3: Simple tool call with basic arguments
#[tokio::test]
async fn test_parse_json_response_simple_tool_call() {
    let response = r#"{"tool": "search", "arguments": {"query": "rust programming"}}"#;
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "search");
    assert!(args_opt.is_some());
    let args = args_opt.unwrap();
    assert_eq!(args["query"], "rust programming");
}

// Test 4: Tool call with complex nested arguments
#[tokio::test]
async fn test_parse_json_response_complex_tool_call() {
    let response = r#"{
        "tool": "database_query", 
        "arguments": {
            "query": {
                "table": "users",
                "conditions": {
                    "age": {"gt": 18},
                    "status": "active"
                },
                "limit": 10
            }
        }
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "database_query");
    assert!(args_opt.is_some());
    
    let args = args_opt.unwrap();
    assert_eq!(args["query"]["table"], "users");
    assert_eq!(args["query"]["conditions"]["age"]["gt"], 18);
    assert_eq!(args["query"]["conditions"]["status"], "active");
    assert_eq!(args["query"]["limit"], 10);
}

// Test 5: Tool call with array arguments
#[tokio::test]
async fn test_parse_json_response_array_tool_call() {
    let response = r#"{
        "tool": "batch_process", 
        "arguments": {
            "items": [
                {"id": 1, "name": "Item 1"},
                {"id": 2, "name": "Item 2"},
                {"id": 3, "name": "Item 3"}
            ]
        }
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "batch_process");
    assert!(args_opt.is_some());
    
    let args = args_opt.unwrap();
    let items = &args["items"];
    assert!(items.is_array());
    assert_eq!(items.as_array().unwrap().len(), 3);
    assert_eq!(items[0]["id"], 1);
    assert_eq!(items[2]["name"], "Item 3");
}

// Test 6: Invalid JSON syntax
#[tokio::test]
async fn test_parse_json_response_invalid_syntax() {
    let response = "not a json string";
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    let response = r#"{"unclosed: "object"#;
    let result = parse_json_response(response);
    assert!(result.is_none());
}

// Test 7: Valid JSON but missing required fields
#[tokio::test]
async fn test_parse_json_response_missing_fields() {
    // Missing both choice and tool
    let response = r#"{"something": "else"}"#;
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Has tool but missing arguments
    let response = r#"{"tool": "calculator"}"#;
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Has arguments but missing tool
    let response = r#"{"arguments": {"value": 42}}"#;
    let result = parse_json_response(response);
    assert!(result.is_none());
}

// Test 8: Edge cases with empty values
#[tokio::test]
async fn test_parse_json_response_empty_values() {
    // Empty choice string
    let response = r#"{"choice": ""}"#;
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (choice, args) = result.unwrap();
    assert_eq!(choice, "");
    assert!(args.is_none());
    
    // Empty tool name
    let response = r#"{"tool": "", "arguments": {}}"#;
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "");
    assert!(args_opt.is_some());
    
    // Empty arguments object
    let response = r#"{"tool": "calculator", "arguments": {}}"#;
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "calculator");
    assert!(args_opt.is_some());
    let args = args_opt.unwrap();
    assert!(args.is_object());
    assert_eq!(args.as_object().unwrap().len(), 0);
}

// Test 9: Handling of different data types in arguments
#[tokio::test]
async fn test_parse_json_response_data_types() {
    let response = r#"{
        "tool": "mixed_types", 
        "arguments": {
            "string_value": "text",
            "integer_value": 42,
            "float_value": 3.14,
            "boolean_value": true,
            "null_value": null,
            "array_value": [1, 2, 3]
        }
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "mixed_types");
    assert!(args_opt.is_some());
    
    let args = args_opt.unwrap();
    assert_eq!(args["string_value"], "text");
    assert_eq!(args["integer_value"], 42);
    assert_eq!(args["float_value"], 3.14);
    assert_eq!(args["boolean_value"], true);
    assert!(args["null_value"].is_null());
    assert!(args["array_value"].is_array());
    assert_eq!(args["array_value"][0], 1);
}

// Test 10: Case sensitivity in field names
#[tokio::test]
async fn test_parse_json_response_case_sensitivity() {
    // Different case for "choice"
    let response = r#"{"Choice": "finish_response"}"#;
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Different case for "tool"
    let response = r#"{"Tool": "calculator", "arguments": {}}"#;
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Different case for "arguments"
    let response = r#"{"tool": "calculator", "Arguments": {}}"#;
    let result = parse_json_response(response);
    assert!(result.is_none());
}

// Test 11: Extra fields should not affect parsing
#[tokio::test]
async fn test_parse_json_response_extra_fields() {
    // Choice with extra fields
    let response = r#"{"choice": "finish_response", "extra": "field", "more": 123}"#;
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (choice, args) = result.unwrap();
    assert_eq!(choice, "finish_response");
    assert!(args.is_none());
    
    // Tool call with extra fields
    let response = r#"{
        "tool": "calculator", 
        "arguments": {"value": 42},
        "meta": "data",
        "timestamp": 1625097600
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "calculator");
    assert!(args_opt.is_some());
    let args = args_opt.unwrap();
    assert_eq!(args["value"], 42);
}

// Test 12: Whitespace handling
#[tokio::test]
async fn test_parse_json_response_whitespace() {
    // Leading/trailing whitespace
    let response = "  \n  {\"choice\": \"finish_response\"}  \t  ";
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (choice, _) = result.unwrap();
    assert_eq!(choice, "finish_response");
    
    // Internal whitespace
    let response = r#"{
        "tool"    :    "calculator"   ,
        "arguments"    :    {
            "value"   :   42
        }
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    let (tool, args_opt) = result.unwrap();
    assert_eq!(tool, "calculator");
    assert!(args_opt.is_some());
    let args = args_opt.unwrap();
    assert_eq!(args["value"], 42);
}

