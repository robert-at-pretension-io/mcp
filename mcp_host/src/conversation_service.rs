// Removed unused imports: anyhow::Result, axum::extract::ws::{Message, WebSocket}, console::style, serde_json::Value, std::sync::Arc, crate::conversation_state::ConversationState, crate::host::MCPHost, crate::tool_parser::ToolParser

// Removed local ToolCall struct definition (now handled internally by ToolParser)
// Removed leftover SmileyToolParser struct definition and methods
// Removed parse_json_response function
// Removed handle_assistant_response function
// Removed execute_single_tool function
// Removed execute_tool_and_continue function
// Removed continue_conversation_after_tools function


/// Generate a system prompt instructing the AI about tool usage with text delimiters
pub fn generate_tool_system_prompt(tools: &[shared_protocol_objects::ToolInfo]) -> String { // Renamed function
    // Format tools information
    let tools_info = tools.iter()
        .map(|t| format!(
            "- Name: {}\n  Description: {}\n  Schema: {}",
            t.name,
            t.description.as_ref().unwrap_or(&"No description".to_string()),
            serde_json::to_string_pretty(&t.input_schema).unwrap_or_else(|_| "{}".to_string())
        ))
        .collect::<Vec<String>>()
        .join("\n\n");

    // Create the full system prompt with the new text delimiter instructions
    format!(
        "You have access to the following tools:\n\n{}\n\n\
        When you need to use a tool, you MUST format your request exactly as follows, including the delimiters:\n\
        <<<TOOL_CALL>>>\n\
        {{\n  \
          \"name\": \"tool_name\",\n  \
          \"arguments\": {{\n    \
            \"arg1\": \"value1\",\n    \
            \"arg2\": \"value2\"\n  \
          }}\n\
        }}\n\
        <<<END_TOOL_CALL>>>\n\n\
        Important:\n\
        - You MUST use the exact delimiters `<<<TOOL_CALL>>>` and `<<<END_TOOL_CALL>>>` on separate lines surrounding the JSON.\n\
        - The JSON block MUST contain a `name` field (string) and an `arguments` field (object).\n\
        - The JSON must be valid and the arguments must match the schema for the chosen tool.\n\
        - Only include ONE tool call JSON block per `<<<TOOL_CALL>>>...<<<END_TOOL_CALL>>>` section.\n\
        - If you need to use multiple tools, return them one after another, each in their own delimited section.\n\
        - You can include explanatory text before or after the `<<<TOOL_CALL>>>...<<<END_TOOL_CALL>>>` block. Do NOT put text inside the delimiters other than the JSON.\n\
        - If no tool is needed, just respond normally to the user without using the delimiters.",
        tools_info
    )
}
