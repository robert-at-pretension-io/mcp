use anyhow::Result;
use serde_json::{json, Value};
use tokio::test;

// Import the Tool trait to test validation patterns
use mcp_tools::tool_trait::Tool;
use mcp_tools::bash::{BashExecutor, BashParams};
use shared_protocol_objects::CallToolParams;

// Create a minimal tool for testing schema validation
struct SchemaValidationTestTool;

impl Tool for SchemaValidationTestTool {
    fn name(&self) -> &str {
        "schema_test_tool"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        shared_protocol_objects::ToolInfo {
            name: "schema_test_tool".to_string(),
            description: Some("Tool for testing schema validation".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "required_string": {
                        "type": "string",
                        "description": "A required string parameter"
                    },
                    "optional_number": {
                        "type": "number",
                        "description": "An optional number parameter"
                    },
                    "enum_option": {
                        "type": "string",
                        "enum": ["option1", "option2", "option3"],
                        "description": "A string with enum options"
                    },
                    "nested_object": {
                        "type": "object",
                        "properties": {
                            "nested_prop": {
                                "type": "string"
                            }
                        }
                    }
                },
                "required": ["required_string"],
                "additionalProperties": false
            }),
        }
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> mcp_tools::tool_trait::ExecuteFuture {
        Box::pin(async move {
            // Get the input schema
            let schema = self.info().input_schema;
            
            // Basic validation - check required fields
            let args = params.arguments;
            
            // Check for required fields
            if let Some(required) = schema.get("required") {
                if let Some(required_fields) = required.as_array() {
                    for field in required_fields {
                        if let Some(field_name) = field.as_str() {
                            if !args.get(field_name).is_some() {
                                return Ok(shared_protocol_objects::error_response(
                                    Some(id.unwrap_or(Value::Null)),
                                    -32602, // Invalid params
                                    &format!("Missing required field: {}", field_name)
                                ));
                            }
                        }
                    }
                }
            }
            
            // Type validation
            if let Some(properties) = schema.get("properties") {
                if let Some(prop_obj) = properties.as_object() {
                    for (prop_name, prop_schema) in prop_obj {
                        if let Some(field_value) = args.get(prop_name) {
                            // Check type
                            if let Some(type_value) = prop_schema.get("type") {
                                if let Some(type_str) = type_value.as_str() {
                                    match type_str {
                                        "string" => {
                                            if !field_value.is_string() {
                                                return Ok(shared_protocol_objects::error_response(
                                                    Some(id.unwrap_or(Value::Null)),
                                                    -32602,
                                                    &format!("Field '{}' must be a string", prop_name)
                                                ));
                                            }
                                            
                                            // Check enum constraints if any
                                            if let Some(enum_values) = prop_schema.get("enum") {
                                                if let Some(enum_array) = enum_values.as_array() {
                                                    let field_str = field_value.as_str().unwrap();
                                                    let valid = enum_array.iter().any(|v| {
                                                        v.as_str().map_or(false, |s| s == field_str)
                                                    });
                                                    
                                                    if !valid {
                                                        return Ok(shared_protocol_objects::error_response(
                                                            Some(id.unwrap_or(Value::Null)),
                                                            -32602,
                                                            &format!("Field '{}' must be one of the allowed values", prop_name)
                                                        ));
                                                    }
                                                }
                                            }
                                        },
                                        "number" => {
                                            if !field_value.is_number() {
                                                return Ok(shared_protocol_objects::error_response(
                                                    Some(id.unwrap_or(Value::Null)),
                                                    -32602,
                                                    &format!("Field '{}' must be a number", prop_name)
                                                ));
                                            }
                                        },
                                        "object" => {
                                            if !field_value.is_object() {
                                                return Ok(shared_protocol_objects::error_response(
                                                    Some(id.unwrap_or(Value::Null)),
                                                    -32602,
                                                    &format!("Field '{}' must be an object", prop_name)
                                                ));
                                            }
                                        },
                                        _ => {} // Other types
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Check for additionalProperties if set to false
            if let Some(additional) = schema.get("additionalProperties") {
                if let Some(additional_bool) = additional.as_bool() {
                    if !additional_bool {
                        if let Some(properties) = schema.get("properties") {
                            if let Some(prop_obj) = properties.as_object() {
                                if let Some(args_obj) = args.as_object() {
                                    for arg_key in args_obj.keys() {
                                        if !prop_obj.contains_key(arg_key) {
                                            return Ok(shared_protocol_objects::error_response(
                                                Some(id.unwrap_or(Value::Null)),
                                                -32602,
                                                &format!("Unknown field: {}", arg_key)
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // If validation passes, return success
            let content = shared_protocol_objects::ToolResponseContent {
                type_: "text".to_string(),
                text: "Schema validation passed!".to_string(),
                annotations: None,
            };
            
            let result = shared_protocol_objects::CallToolResult {
                content: vec![content],
                is_error: None,
                _meta: None,
                progress: None,
                total: None,
            };
            
            Ok(shared_protocol_objects::success_response(
                Some(id.unwrap_or(Value::Null)),
                json!(result)
            ))
        })
    }
}

#[test]
async fn test_valid_parameters() -> Result<()> {
    let tool = SchemaValidationTestTool;
    
    // Create valid parameters
    let params = CallToolParams {
        name: "schema_test_tool".to_string(),
        arguments: json!({
            "required_string": "test value",
            "optional_number": 42,
            "enum_option": "option1",
            "nested_object": {
                "nested_prop": "nested value"
            }
        }),
    };
    
    // Execute the tool
    let response = tool.execute(params, Some(json!(1))).await?;
    
    // Verify success
    assert!(response.error.is_none(), "Should not have an error");
    
    // Get the result
    let result_value = response.result.unwrap();
    let result: shared_protocol_objects::CallToolResult = serde_json::from_value(result_value)?;
    
    // Verify content
    assert_eq!(result.content[0].text, "Schema validation passed!", "Should indicate validation passed");
    
    Ok(())
}

#[test]
async fn test_missing_required_field() -> Result<()> {
    let tool = SchemaValidationTestTool;
    
    // Create parameters missing the required field
    let params = CallToolParams {
        name: "schema_test_tool".to_string(),
        arguments: json!({
            "optional_number": 42
        }),
    };
    
    // Execute the tool
    let response = tool.execute(params, Some(json!(1))).await?;
    
    // Verify error
    assert!(response.error.is_some(), "Should have an error");
    assert_eq!(response.error.unwrap().code, -32602, "Should have invalid params error code");
    
    Ok(())
}

#[test]
async fn test_type_mismatch() -> Result<()> {
    let tool = SchemaValidationTestTool;
    
    // Create parameters with a type mismatch
    let params = CallToolParams {
        name: "schema_test_tool".to_string(),
        arguments: json!({
            "required_string": "test value",
            "optional_number": "not a number" // String instead of number
        }),
    };
    
    // Execute the tool
    let response = tool.execute(params, Some(json!(1))).await?;
    
    // Verify error
    assert!(response.error.is_some(), "Should have an error");
    let error = response.error.unwrap();
    assert_eq!(error.code, -32602, "Should have invalid params error code");
    assert!(error.message.contains("must be a number"), "Error should mention type mismatch");
    
    Ok(())
}

#[test]
async fn test_enum_constraint() -> Result<()> {
    let tool = SchemaValidationTestTool;
    
    // Create parameters with an invalid enum value
    let params = CallToolParams {
        name: "schema_test_tool".to_string(),
        arguments: json!({
            "required_string": "test value",
            "enum_option": "invalid_option" // Not in allowed values
        }),
    };
    
    // Execute the tool
    let response = tool.execute(params, Some(json!(1))).await?;
    
    // Verify error
    assert!(response.error.is_some(), "Should have an error");
    let error = response.error.unwrap();
    assert_eq!(error.code, -32602, "Should have invalid params error code");
    assert!(error.message.contains("allowed values"), "Error should mention allowed values");
    
    Ok(())
}

#[test]
async fn test_additional_properties() -> Result<()> {
    let tool = SchemaValidationTestTool;
    
    // Create parameters with an unknown field
    let params = CallToolParams {
        name: "schema_test_tool".to_string(),
        arguments: json!({
            "required_string": "test value",
            "unknown_field": "should not be allowed" // Not in schema
        }),
    };
    
    // Execute the tool
    let response = tool.execute(params, Some(json!(1))).await?;
    
    // Verify error
    assert!(response.error.is_some(), "Should have an error");
    let error = response.error.unwrap();
    assert_eq!(error.code, -32602, "Should have invalid params error code");
    assert!(error.message.contains("Unknown field"), "Error should mention unknown field");
    
    Ok(())
}

#[test]
async fn test_nested_object_validation() -> Result<()> {
    let tool = SchemaValidationTestTool;
    
    // Create parameters with an invalid nested object
    let params = CallToolParams {
        name: "schema_test_tool".to_string(),
        arguments: json!({
            "required_string": "test value",
            "nested_object": "not an object" // String instead of object
        }),
    };
    
    // Execute the tool
    let response = tool.execute(params, Some(json!(1))).await?;
    
    // Verify error
    assert!(response.error.is_some(), "Should have an error");
    let error = response.error.unwrap();
    assert_eq!(error.code, -32602, "Should have invalid params error code");
    assert!(error.message.contains("must be an object"), "Error should mention object type");
    
    Ok(())
}

// Test with BashExecutor to verify real-world tool schema validation
#[test]
async fn test_bash_tool_validation() -> Result<()> {
    // Get the tool info including schema
    let executor = BashExecutor::new();
    let info = executor.tool_info();
    
    // Examine the schema structure
    let schema = info.input_schema;
    assert!(schema.is_object(), "Schema should be a JSON object");
    
    // Check schema constraints
    let properties = schema.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("command"), "Schema should define command property");
    
    let required = schema.get("required");
    assert!(required.is_some(), "Schema should define required fields");
    assert!(required.unwrap().as_array().unwrap().contains(&json!("command")), "command should be required");
    
    // Test execution with valid params
    let params = BashParams {
        command: "echo 'Hello'".to_string(),
        cwd: std::env::current_dir()?.to_string_lossy().to_string(),
    };
    
    let result = executor.execute(params).await?;
    assert!(result.success, "Command should execute successfully");
    assert_eq!(result.stdout.trim(), "Hello", "Output should match");
    
    Ok(())
}