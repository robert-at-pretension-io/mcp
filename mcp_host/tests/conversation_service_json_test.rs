use mcp_host::conversation_service::parse_json_response;
use serde_json::Value;

#[test]
fn test_parse_response_with_only_text() {
    // Test a response with just normal text, no JSON
    let response = "This is a plain text response, not JSON.";
    let result = parse_json_response(response);
    assert!(result.is_none());
}

#[test]
fn test_parse_whitespace_and_newlines() {
    // Test with leading and trailing whitespace
    let response = "  \n\t  {\"choice\": \"tool_call\"}  \n  ";
    let result = parse_json_response(response);
    
    assert!(result.is_some());
    let (choice, args) = result.unwrap();
    assert_eq!(choice, "tool_call");
    assert!(args.is_none());
    
    // Test with newlines inside the JSON
    let response = "{\n  \"choice\":\n  \"finish_response\"\n}";
    let result = parse_json_response(response);
    
    assert!(result.is_some());
    let (choice, args) = result.unwrap();
    assert_eq!(choice, "finish_response");
    assert!(args.is_none());
}

#[test]
fn test_parse_invalid_json_formats() {
    // Test with invalid JSON format
    let response = "{choice: tool_call}"; // Missing quotes
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Test with unclosed JSON
    let response = "{\"tool\": \"search\", \"arguments\": {\"query\":";
    let result = parse_json_response(response);
    assert!(result.is_none());
}

#[test]
fn test_parse_response_choice_variations() {
    // Test with different case (should be case-sensitive)
    let response = "{\"CHOICE\": \"tool_call\"}";
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Test with choice as number (should be string)
    let response = "{\"choice\": 123}";
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Test with choice as boolean (should be string)
    let response = "{\"choice\": true}";
    let result = parse_json_response(response);
    assert!(result.is_none());
}

#[test]
fn test_parse_response_tool_with_invalid_arguments() {
    // Test with tool but missing arguments
    let response = "{\"tool\": \"search\"}";
    let result = parse_json_response(response);
    assert!(result.is_none());
    
    // Test with tool but null arguments
    let response = "{\"tool\": \"search\", \"arguments\": null}";
    let result = parse_json_response(response);
    
    assert!(result.is_some());
    let (tool, args) = result.unwrap();
    assert_eq!(tool, "search");
    assert!(args.is_some());
    assert!(args.unwrap().is_null());
}

#[test]
fn test_parse_response_tool_with_complex_arguments() {
    // Test with complex nested arguments
    let response = r#"{
        "tool": "database_query",
        "arguments": {
            "query": {
                "table": "users",
                "filter": {
                    "age": {"$gt": 18},
                    "status": ["active", "pending"],
                    "location": {
                        "city": "New York",
                        "coordinates": [40.7128, -74.0060]
                    }
                },
                "sort": {"created_at": "desc"},
                "limit": 50
            }
        }
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    
    let (tool, args) = result.unwrap();
    assert_eq!(tool, "database_query");
    assert!(args.is_some());
    
    let args = args.unwrap();
    let query = &args["query"];
    assert_eq!(query["table"], "users");
    assert_eq!(query["filter"]["age"]["$gt"], 18);
    assert_eq!(query["filter"]["status"][0], "active");
    assert_eq!(query["filter"]["status"][1], "pending");
    assert_eq!(query["filter"]["location"]["city"], "New York");
    assert_eq!(query["filter"]["location"]["coordinates"][0], 40.7128);
    assert_eq!(query["filter"]["location"]["coordinates"][1], -74.0060);
    assert_eq!(query["sort"]["created_at"], "desc");
    assert_eq!(query["limit"], 50);
}

#[test]
fn test_parse_mixed_json_data_types() {
    // Test with mixed data types (arrays, numbers, booleans, strings, null)
    let response = r#"{
        "tool": "multi_type_tool",
        "arguments": {
            "string_val": "text",
            "int_val": 42,
            "float_val": 3.14159,
            "bool_val": true,
            "null_val": null,
            "array_val": [1, "two", 3.0, false, null],
            "nested_array": [[1, 2], [3, 4]],
            "empty_obj": {},
            "empty_array": []
        }
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    
    let (tool, args) = result.unwrap();
    assert_eq!(tool, "multi_type_tool");
    assert!(args.is_some());
    
    let args = args.unwrap();
    assert_eq!(args["string_val"], "text");
    assert_eq!(args["int_val"], 42);
    assert_eq!(args["float_val"], 3.14159);
    assert_eq!(args["bool_val"], true);
    assert!(args["null_val"].is_null());
    
    // Test array values
    assert!(args["array_val"].is_array());
    assert_eq!(args["array_val"][0], 1);
    assert_eq!(args["array_val"][1], "two");
    assert_eq!(args["array_val"][2], 3.0);
    assert_eq!(args["array_val"][3], false);
    assert!(args["array_val"][4].is_null());
    
    // Test nested arrays
    assert!(args["nested_array"].is_array());
    assert_eq!(args["nested_array"][0][0], 1);
    assert_eq!(args["nested_array"][0][1], 2);
    assert_eq!(args["nested_array"][1][0], 3);
    assert_eq!(args["nested_array"][1][1], 4);
    
    // Test empty objects and arrays
    assert!(args["empty_obj"].is_object());
    assert_eq!(args["empty_obj"].as_object().unwrap().len(), 0);
    assert!(args["empty_array"].is_array());
    assert_eq!(args["empty_array"].as_array().unwrap().len(), 0);
}

#[test]
fn test_parse_response_with_extra_fields() {
    // Test with both choice and tool (choice should take precedence)
    let response = r#"{
        "choice": "finish_response", 
        "tool": "calculator", 
        "arguments": {"input": "2+2"}
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    
    let (choice, args) = result.unwrap();
    assert_eq!(choice, "finish_response");
    assert!(args.is_none());
    
    // Test with tool and extra fields
    let response = r#"{
        "tool": "search",
        "arguments": {"query": "rust"},
        "metadata": {
            "timestamp": 1656042000,
            "source": "user_prompt"
        },
        "debug": true
    }"#;
    
    let result = parse_json_response(response);
    assert!(result.is_some());
    
    let (tool, args) = result.unwrap();
    assert_eq!(tool, "search");
    assert!(args.is_some());
    let args = args.unwrap();
    assert_eq!(args["query"], "rust");
    // Extra fields shouldn't be part of the args
    assert!(!args.as_object().unwrap().contains_key("metadata"));
    assert!(!args.as_object().unwrap().contains_key("debug"));
}