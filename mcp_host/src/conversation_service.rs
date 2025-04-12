// Removed unused imports: anyhow::Result, axum::extract::ws::{Message, WebSocket}, console::style, serde_json::Value, std::sync::Arc, crate::conversation_state::ConversationState, crate::host::MCPHost, crate::tool_parser::ToolParser

// Removed local ToolCall struct definition (now handled internally by ToolParser)
// Removed leftover SmileyToolParser struct definition and methods
// Removed parse_json_response function
// Removed handle_assistant_response function
// Removed execute_single_tool function
// Removed execute_tool_and_continue function
// Removed continue_conversation_after_tools function


/// Generate a system prompt instructing the AI about tool usage with text delimiters
pub fn generate_tool_system_prompt(tools: &[rmcp::model::Tool]) -> String {
    // Format tools information
    let tools_info = tools.iter()
        .map(|t| format!(
            "- Name: {}\n  Description: {}\n  Schema: {}",
            t.name.as_ref(),
            t.description.as_deref().unwrap_or(""), // Use as_deref().unwrap_or("")
            serde_json::to_string_pretty(&t.input_schema).unwrap_or_else(|_| "{}".to_string())
        ))
        .collect::<Vec<String>>()
        .join("\n\n");

    // Create the full system prompt with the new text delimiter instructions
    format!(
        "You are a helpful assistant with access to tools. Use tools EXACTLY according to their descriptions and required format.\n\n\
        **Core Instructions for Tool Use:**\n\n\
        1.  **Address the Full Request:** Plan and execute all necessary steps sequentially using tools. If generating information *and* performing an action (like saving), **include the key information/summary in your response** along with action confirmation.\n\
        2.  **Execution Model & Reacting to Results:**\n    \
            *   **Dispatch:** All tools you call in a single response turn are dispatched *before* you receive results for *any* of them.\n    \
            *   **Results:** You *will* receive the results for all dispatched tools in the *next* conversation turn.\n    \
            *   **No Same-Turn Chaining:** Because of the dispatch timing, **you cannot use the result of one tool as input for another tool within the *same* response turn.** Plan sequential, dependent calls across multiple turns.\n    \
            *   **Verification & Adaptation:** Carefully review tool results when you receive them. Verify success/failure, extract data, and **change your plan or response if the results require it.**\n\
        3.  **Be Truthful & Cautious:** Only confirm actions (e.g., \"file saved\") if the tool result explicitly confirms success. Report errors. Be careful with tools that modify external systems.\n\
        4.  **Use Correct Format:** Use the precise `<<<TOOL_CALL>>>...<<<END_TOOL_CALL>>>` format with valid JSON (`name`, `arguments`) for all tool calls.\n\n\
        # Tool Descriptions...\n\
        {}\n\n\
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
        - Only include ONE tool call JSON block per delimiter section. Use multiple sections for multiple parallel calls in one turn.\n\
        - You can include explanatory text before or after the tool call block.\n\
        - If no tool is needed, just respond normally.",
        tools_info // Insert the formatted tool descriptions here
    )
}
