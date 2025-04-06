use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use console::style;
use serde_json::Value;
use std::sync::Arc;
use crate::conversation_state::ConversationState;
use crate::ai_client::{ AIClient };
use crate::host::MCPHost;
use crate::tool_parser::{ToolParser}; // Keep ToolParser import, remove ToolCall

use shared_protocol_objects::Role;

// Removed local ToolCall struct definition (now handled internally by ToolParser)


// Removed leftover SmileyToolParser struct definition and methods


/// Function to parse JSON response and extract tool calls
pub fn parse_json_response(response: &str) -> Option<(String, Option<Value>)> {
    // First try traditional JSON format
    if let Ok(json_value) = serde_json::from_str::<Value>(response) {
        // Check for tool call
        if let Some(tool_name) = json_value.get("tool").and_then(|t| t.as_str()) {
            if let Some(args) = json_value.get("arguments") {
                return Some((tool_name.to_string(), Some(args.clone())));
            }
        }
    }

    // If no direct JSON tool call, try to find delimited tool calls using the ToolParser
    // Note: ToolParser::parse_tool_calls returns Vec<crate::tool_parser::ToolCall>
    // We need to adapt the logic if we want to use the parsed results here,
    // but for now, this function only handles the *old* JSON format check.
    // The main handle_assistant_response function uses ToolParser directly.

    // No tool calls found in the old JSON format
    None
}

pub async fn handle_assistant_response(
    host: &MCPHost,
    incoming_response: &str,
    server_name: &str,
    state: &mut ConversationState,
    client: Arc<dyn AIClient>, // Change to Arc<dyn AIClient>
    mut socket: Option<&mut WebSocket>
) -> Result<()> {
    // Use Box::pin for recursive async functions
    Box::pin(async {
    // Record the incoming response
    state.add_assistant_message(incoming_response);

    // Parse all tool calls using the new parser
    let tool_calls = ToolParser::parse_tool_calls(incoming_response); // Use new parser

    if tool_calls.is_empty() {
        // If no delimited tool calls, check for standard JSON format (this logic remains the same)
        // Note: parse_json_response currently only checks the old format.
        // This block might need adjustment if you want parse_json_response
        // to also understand the new delimited format.
        if let Some((tool_name, Some(args))) = parse_json_response(incoming_response) {
            // Found a single JSON tool call (old format)
            // Display the tool call before executing
            println!(
                "\n{}",
                style(format!("Assistant wants to call tool: {}", style(&tool_name).yellow())).italic() // Style tool name yellow
            );
            println!(
                "{}",
                style(format!(
                    "Arguments:\n{}",
                    serde_json::to_string_pretty(&args).unwrap_or_else(|_| "Invalid JSON".to_string())
                )).dim() // Dim arguments
            );

            return execute_tool_and_continue(
                host,
                server_name,
                &tool_name,
                args,
                state,
                client,
                &mut socket
            ).await;
        }
    } else {
        // We have delimited tool calls - execute them all in sequence
        for tool_call in tool_calls { // tool_call here is crate::tool_parser::ToolCall
            // Display the tool call before executing
            println!(
                "\n{}",
                 style(format!("Assistant wants to call tool: {}", style(&tool_call.name).yellow())).italic() // Style tool name yellow
            );
            println!(
                "{}",
                style(format!(
                    "Arguments:\n{}",
                    serde_json::to_string_pretty(&tool_call.arguments).unwrap_or_else(|_| "Invalid JSON".to_string())
                )).dim() // Dim arguments
            );

            // Execute each tool call
            let tool_result = execute_single_tool(
                host,
                server_name,
                &tool_call.name,
                tool_call.arguments.clone(),
                state,
                &mut socket
            ).await?;

            // Add tool result to conversation
            let result_msg = format!("Tool '{}' returned: {}", tool_call.name, tool_result.trim());
            state.add_assistant_message(&result_msg);
        }

        // After all tools have been called, get the next response from AI
        return continue_conversation_after_tools(host, server_name, state, client, &mut socket).await;
    }

    // If no tool calls were found, treat as normal response
    println!(
        "\n{}",
        // Use the formatting function from conversation_state
        crate::conversation_state::format_chat_message(&Role::Assistant, incoming_response)
    );

    // Send the final text to client
    if let Some(ref mut ws) = socket {
        let _ = ws.send(Message::Text(incoming_response.to_string())).await;
    }

    Ok(())
    }).await
}

/// Execute a single tool and get its result
async fn execute_single_tool(
    host: &MCPHost,
    server_name: &str,
    tool_name: &str,
    args: serde_json::Value,
    _state: &mut ConversationState,
    socket: &mut Option<&mut WebSocket>
) -> Result<String> {
    // Display tool call start if using WebSocket
    if let Some(ref mut ws) = socket {
        let start_msg = serde_json::json!({ "type": "tool_call_start", "tool_name": tool_name });
        let _ = ws.send(Message::Text(start_msg.to_string())).await;
    }

    // Call the tool through the MCP host with progress indicator
    // Style the progress message here
    let progress_msg = format!("Calling tool '{}' on server '{}'...", style(tool_name).yellow(), style(server_name).green());
    match crate::repl::with_progress(
        progress_msg,
        host.call_tool(server_name, tool_name, args.clone())
    ).await {
        Ok(result_string) => {
            // Truncate the result before further processing
            let truncated_result = crate::repl::truncate_lines(&result_string, 150);

            if let Some(ref mut ws) = socket {
                let end_msg = serde_json::json!({
                    "type": "tool_call_end",
                    "tool_name": tool_name
                });
                let _ = ws.send(Message::Text(end_msg.to_string())).await;
            }

            // Use the formatting function from conversation_state
            println!("\n{}", crate::conversation_state::format_tool_response(tool_name, &truncated_result));

            Ok(truncated_result) // Return truncated result
        },
        Err(error) => {
            let error_msg = format!("Error: {}", error);
            log::error!("Tool '{}' error: {}", tool_name, error_msg);

            // Return error as result, styled
            let formatted_error = format!("{} executing tool '{}': {}", style("Error").red(), style(tool_name).yellow(), error_msg);
            println!("\n{}", formatted_error); // Print styled error
            Ok(formatted_error) // Return the styled error string
        }
    }
}

/// Execute a tool and continue the conversation
async fn execute_tool_and_continue(
    host: &MCPHost,
    server_name: &str,
    tool_name: &str,
    args: serde_json::Value,
    state: &mut ConversationState,
    client: Arc<dyn AIClient>, // Change to Arc
    socket: &mut Option<&mut WebSocket>
) -> Result<()> {
    // Execute the tool
    let result = execute_single_tool(host, server_name, tool_name, args, state, socket).await?;

    // Add tool result to conversation
    let result_msg = format!("Tool '{}' returned: {}", tool_name, result.trim());
    state.add_assistant_message(&result_msg);

    // Continue conversation
    continue_conversation_after_tools(host, server_name, state, client, socket).await
}

/// Continue conversation after one or more tool calls
async fn continue_conversation_after_tools(
    host: &MCPHost,
    server_name: &str,
    state: &mut ConversationState,
    client: Arc<dyn AIClient>, // Change to Arc
    socket: &mut Option<&mut WebSocket>
) -> Result<()> {
    // Create a builder with all conversation context
    let mut builder = client.raw_builder();

    // Add all messages for context
    for msg in &state.messages {
        match msg.role {
            Role::System => builder = builder.system(msg.content.clone()),
            Role::User => builder = builder.user(msg.content.clone()),
            Role::Assistant => builder = builder.assistant(msg.content.clone()),
        }
    }

    // Add a more directive prompt after tool results
    builder = builder.system(
        "Analyze the tool results provided above. Based on those results and the user's original request, decide the next step:\n\
        1. Call another tool if necessary (using the smiley-delimited format).\n\
        2. Provide a final response to the user.".to_string()
    );

    // Get next response from the AI
    match builder.execute().await {
        Ok(next_response) => {
            // Process this next response for more tool calls or final answer
            // Need to transfer ownership of the socket reference
            let socket_copy = socket.take();
            // Clone the Arc for the recursive call
            Box::pin(handle_assistant_response(host, &next_response, server_name, state, client.clone(), socket_copy)).await
        },
        Err(e) => {
            log::error!("Error getting next response: {}", e);
            if let Some(ref mut ws) = socket {
                let err_msg = serde_json::json!({
                    "type": "error",
                    "data": e.to_string()
                });
                let _ = ws.send(Message::Text(err_msg.to_string())).await;
            }
            Ok(())
        }
    }
}

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
