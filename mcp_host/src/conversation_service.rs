use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use serde_json::Value;
use crate::conversation_state::ConversationState;
use crate::ai_client::{ AIClient };
use crate::MCPHost;

use shared_protocol_objects::Role;

#[derive(Debug)]
pub enum ToolCallResult {
    Success(String, Value),
    NearMiss(Vec<String>),
    NoMatch,
}


use crate::my_regex::build_tool_call_regex;

/// Attempts to parse a single tool call from `response`.
pub fn parse_tool_call(response: &str, tool_names: &[String]) -> ToolCallResult {
    let dynamic_regex = build_tool_call_regex(tool_names);

    log::debug!("Parsing response for tool calls:\n{}", response);

    // 1) Attempt to match
    if let Some(caps) = dynamic_regex.captures(response) {
        let tool_name = caps[1].to_string();
        let brace_start_index = caps.get(2).unwrap().start();

        // 2) Extract braces
        match extract_balanced_braces(response, brace_start_index) {
            Some((json_str, _end_index)) => {
                // 3) Attempt to parse that JSON
                match serde_json::from_str::<Value>(&json_str) {
                    Ok(obj) => {
                        log::debug!("Successfully parsed JSON for tool '{}'", tool_name);
                        ToolCallResult::Success(tool_name, obj)
                    }
                    Err(e) => {
                        let msg = format!(
                            "Found tool call for '{tool_name}', but JSON parse failed: {e}\nJSON block was: {json_str}"
                        );
                        log::warn!("{}", msg);
                        ToolCallResult::NearMiss(vec![msg])
                    }
                }
            }
            None => {
                let msg = format!(
                    "Found tool call for '{tool_name}', but could not read balanced braces from the text."
                );
                log::warn!("{}", msg);
                ToolCallResult::NearMiss(vec![msg])
            }
        }
    } else {
        // If the dynamic regex fails, we might do fallback attempts, or return NoMatch:
        log::debug!("No valid tool calls found in response");
        ToolCallResult::NoMatch
    }
}

/// Extract a balanced set of braces (from the first `{`) so that we get a complete JSON object
/// even if it contains nested braces. Returns `(json_string, index_after_closing_brace)`.
fn extract_balanced_braces(text: &str, open_brace_index: usize) -> Option<(String, usize)> {
    let mut brace_count = 0;
    let mut end_index = open_brace_index;
    let chars: Vec<char> = text.chars().collect();
    for (i, &ch) in chars[open_brace_index..].iter().enumerate() {
        match ch {
            '{' => brace_count += 1,
            '}' => brace_count -= 1,
            _ => {}
        }
        if brace_count == 0 {
            // i is relative offset
            end_index = open_brace_index + i;
            // substring from open_brace_index..=end_index
            let json_substring = &text[open_brace_index..=end_index];
            return Some((json_substring.to_string(), end_index + 1));
        }
    }
    None
}

pub async fn handle_assistant_response(
    host: &MCPHost,
    incoming_response: &str,
    server_name: &str,
    state: &mut ConversationState,
    client: &Box<dyn AIClient>,
    mut socket: Option<&mut WebSocket>
) -> Result<()> {
    // Record the incoming response
    state.add_assistant_message(incoming_response);

    let tool_names: Vec<String> = state.tools.iter().map(|t| t.name.clone()).collect();
    if let Some((tool_name, args)) = match parse_tool_call(incoming_response, &tool_names) {
        ToolCallResult::Success(name, a) => Some((name, a)),
        ToolCallResult::NearMiss(feedback) => {
            let joined = feedback.join("\n");
            state.add_assistant_message(&joined);
            None
        }
        ToolCallResult::NoMatch => None,
    } {
        // If we found a valid tool call, handle it
        if let Some(ref mut ws) = socket {
            let start_msg = serde_json::json!({ "type": "tool_call_start", "tool_name": &tool_name });
            let _ = ws.send(Message::Text(start_msg.to_string())).await;
        }

        // Call the tool
        let tool_result = host.call_tool(server_name, &tool_name, args.clone()).await;
        
        // Handle success case
        if tool_result.is_ok() {
            let result_string = tool_result.as_ref().unwrap();
            
            if let Some(ref mut ws) = socket {
                let end_msg = serde_json::json!({
                    "type": "tool_call_end",
                    "tool_name": &tool_name
                });
                let _ = ws.send(Message::Text(end_msg.to_string())).await;
            }
            
            println!(
                "{}",
                crate::conversation_state::format_tool_response(&tool_name, result_string)
            );
            let combo = format!("Tool '{tool_name}' returned: {}", result_string.trim());
            state.add_assistant_message(&combo);
        }
        
        // Handle error case
        if tool_result.is_err() {
            let error = tool_result.as_ref().err().unwrap();
            let error_msg = format!("Tool '{tool_name}' error: {}", error);
            state.add_assistant_message(&error_msg);
            log::error!("{}", error_msg);
        }
    }

    // Now generate the final answer again with full conversation context
    let mut builder = client.raw_builder();
    for msg in &state.messages {
        match msg.role {
            Role::System => builder = builder.system(msg.content.clone()),
            Role::User => builder = builder.user(msg.content.clone()),
            Role::Assistant => builder = builder.assistant(msg.content.clone()),
        }
    }

    // Ask for final text
    let final_answer = match builder.execute().await {
        Ok(text) => text,
        Err(e) => {
            log::error!("Error requesting final answer: {}", e);
            if let Some(ref mut ws) = socket {
                let err_msg = serde_json::json!({
                    "type": "error",
                    "data": e.to_string()
                });
                let _ = ws.send(Message::Text(err_msg.to_string())).await;
            }
            return Ok(()); // Early return
        }
    };

    // Display the final text
    println!(
        "\n{}",
        crate::conversation_state::format_chat_message(&Role::Assistant, &final_answer)
    );
    state.add_assistant_message(&final_answer);

    // Send the final text to client
    if let Some(ref mut ws) = socket {
        let _ = ws.send(Message::Text(final_answer)).await;
    }

    Ok(())
}
