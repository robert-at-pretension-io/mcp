use shared_protocol_objects::{Role, ToolInfo};
use serde_json::json;
use mcp_host::conversation_state::{ConversationState, format_chat_message, format_json_output, format_tool_response};

#[test]
fn test_conversation_state_creation() {
    let system_prompt = "You are a helpful assistant".to_string();
    let tools = vec![
        ToolInfo {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "param1": {"type": "string"}
                }
            }),
        }
    ];
    
    let state = ConversationState::new(system_prompt.clone(), tools.clone());
    
    // Verify initialization
    assert_eq!(state.system_prompt, system_prompt);
    assert_eq!(state.tools.len(), 1);
    assert_eq!(state.tools[0].name, "test_tool");
    
    // Should have one message (the system prompt)
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].role, Role::System);
    assert_eq!(state.messages[0].content, system_prompt);
}

#[test]
fn test_add_messages() {
    let system_prompt = "You are a helpful assistant".to_string();
    let tools = vec![];
    
    let mut state = ConversationState::new(system_prompt, tools);
    
    // Add user message
    let user_message = "Hello, how are you?";
    state.add_user_message(user_message);
    
    // Add assistant message
    let assistant_message = "I'm fine, thank you for asking!";
    state.add_assistant_message(assistant_message);
    
    // Verify messages are added correctly
    assert_eq!(state.messages.len(), 3); // system + user + assistant
    assert_eq!(state.messages[1].role, Role::User);
    assert_eq!(state.messages[1].content, user_message);
    assert_eq!(state.messages[2].role, Role::Assistant);
    assert_eq!(state.messages[2].content, assistant_message);
}

#[test]
fn test_format_json_output() {
    // Test valid JSON formatting
    let json_str = r#"{"name":"test","value":42,"nested":{"key":"value"}}"#;
    let formatted = format_json_output(json_str);
    
    // Should be formatted as markdown code block
    assert!(formatted.starts_with("```json\n"));
    assert!(formatted.ends_with("\n```"));
    
    // Should contain properly indented JSON
    assert!(formatted.contains("  \"name\": \"test\""));
    assert!(formatted.contains("  \"value\": 42"));
    assert!(formatted.contains("  \"nested\": {"));
    assert!(formatted.contains("    \"key\": \"value\""));
    
    // Test invalid JSON
    let invalid_json = "not a json string";
    let result = format_json_output(invalid_json);
    assert_eq!(result, invalid_json);
}

#[test]
fn test_format_chat_message() {
    // Test user message formatting
    let user_message = "Hello world";
    let formatted_user = format_chat_message(&Role::User, user_message);
    assert!(formatted_user.contains("User"));
    assert!(formatted_user.contains("Hello world"));
    
    // Test assistant message formatting
    let assistant_message = "I'm an AI assistant";
    let formatted_assistant = format_chat_message(&Role::Assistant, assistant_message);
    assert!(formatted_assistant.contains("Assistant"));
    assert!(formatted_assistant.contains("I'm an AI assistant"));
    
    // Test system message formatting
    let system_message = "System instructions";
    let formatted_system = format_chat_message(&Role::System, system_message);
    assert!(formatted_system.contains("System"));
    assert!(formatted_system.contains("System instructions"));
}

#[test]
fn test_format_tool_response() {
    // Test regular text response
    let tool_name = "calculator";
    let response = "The result is 42";
    let formatted = format_tool_response(tool_name, response);
    
    assert!(formatted.contains("Tool Response:"));
    assert!(formatted.contains(tool_name));
    assert!(formatted.contains(response));
    
    // Test JSON response
    let json_response = r#"{"result": 42, "unit": "meters"}"#;
    let formatted_json = format_tool_response("measurement", json_response);
    
    assert!(formatted_json.contains("Tool Response:"));
    assert!(formatted_json.contains("measurement"));
    assert!(formatted_json.contains("```json"));
    assert!(formatted_json.contains("  \"result\": 42"));
    assert!(formatted_json.contains("  \"unit\": \"meters\""));
}