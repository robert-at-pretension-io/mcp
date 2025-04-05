use anyhow::Result;
use mcp_host::host::MCPHost;
use mcp_host::conversation_state::ConversationState;
use mcp_host::smiley_tool_parser::SmileyToolParser;
use serde_json::json;
use shared_protocol_objects::ToolInfo;

/// Test to verify that the smiley-delimited approach integrates with the REPL and chat mode
#[tokio::test]
async fn test_smiley_repl_integration() -> Result<()> {
    // Create a simple set of tools
    let tools = vec![
        ToolInfo {
            name: "calculator".to_string(),
            description: Some("Calculate a mathematical expression".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "The mathematical expression to calculate"
                    }
                },
                "required": ["expression"]
            }),
        },
    ];
    
    // Create a conversation state with these tools
    let state = ConversationState::new(
        "You are a helpful assistant.".to_string(),
        tools.clone()
    );
    
    // Verify that the system prompt contains the smiley format instructions
    let system_msg = &state.messages[0];
    assert_eq!(system_msg.role, shared_protocol_objects::Role::System);
    assert!(system_msg.content.contains("😊😊😊😊😊😊😊😊😊😊😊😊😊😊"));
    assert!(system_msg.content.contains("\"name\""));
    assert!(system_msg.content.contains("\"arguments\""));
    
    // Simulate a response with a smiley-delimited tool call
    let sample_response = r#"I'll calculate that for you:

😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "calculator",
  "arguments": {
    "expression": "42 * 7"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊

Let me process that calculation for you."#;
    
    // Parse the tool calls from the response
    let tool_calls = SmileyToolParser::parse_tool_calls(sample_response);
    
    // Verify that the tool call was correctly parsed
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "calculator");
    assert_eq!(tool_calls[0].arguments["expression"], "42 * 7");
    
    Ok(())
}

/// Test chat mode integration with smiley-delimited tool calling
#[tokio::test]
async fn test_chat_mode_smiley_format() -> Result<()> {
    // Don't try to actually initialize a real host with connections
    // since this is just a unit test for the integration points
    
    // Create a mock server tool list
    let tools = vec![
        ToolInfo {
            name: "test_tool".to_string(),
            description: Some("Test tool description".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "test_param": {
                        "type": "string"
                    }
                }
            }),
        },
    ];
    
    // Generate the smiley system prompt
    let smiley_prompt = mcp_host::conversation_service::generate_smiley_tool_system_prompt(&tools);
    
    // Verify the prompt format
    assert!(smiley_prompt.contains("😊😊😊😊😊😊😊😊😊😊😊😊😊😊"));
    assert!(smiley_prompt.contains("\"name\": \"tool_name\""));
    assert!(smiley_prompt.contains("\"arguments\": {"));
    
    // Simulate an AI response with tool call
    let ai_response = r#"I'll help you with that.

😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "test_tool",
  "arguments": {
    "test_param": "test_value"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊

Let me know if you need anything else."#;

    // Parse the tool calls
    let tool_calls = SmileyToolParser::parse_tool_calls(ai_response);
    
    // Verify correct extraction
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "test_tool");
    assert_eq!(tool_calls[0].arguments["test_param"], "test_value");
    
    Ok(())
}