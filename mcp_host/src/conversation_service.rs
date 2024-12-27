use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use serde_json::Value;
use crate::MCPHost;
use crate::conversation_state::ConversationState;
use crate::ai_client::{ AIClient };
use console::style;

use lazy_static::lazy_static;
use regex::Regex;

use shared_protocol_objects::Role;

#[derive(Debug)]
pub enum ToolCallResult {
    Success(String, Value),
    NearMiss(Vec<String>),
    NoMatch,
}

lazy_static! {
    /// A regex that finds "Let me call <tool_name>" (or one of several synonyms) 
    /// plus optional code fences, capturing the tool name and the starting brace.
    static ref MASTER_REGEX: Regex = Regex::new(
        r#"(?sx)
        (?:Let\ me\ call|I(?:'|’)ll\ use|Using\ the)    # Phrases like "Let me call", "I'll use", "Using the"
        \s+`?([a-zA-Z_][a-zA-Z0-9_]*)`?\s*(?:tool)?      # The tool name, optionally in backticks, optionally ending with "tool"
        (?:\s*with\s+(?:these\s+)?parameters)?          # Possibly "with these parameters"
        :?                                              # optional colon
        \s*                                             # optional whitespace
        (?:```(?:json)?\s*)?                            # Possibly triple-backtick plus "json"
        (\{)                                            # Capture the STARTING brace for JSON in group 2
        "#).unwrap();
}

/// Attempts to parse a single tool call from `response`.
pub fn parse_tool_call(response: &str) -> ToolCallResult {
    log::debug!("Parsing response for tool calls:\n{}", response);

    // 1) Scan the entire response for a pattern: “Let me call <toolname> ... {”
    //    The MASTER_REGEX returns two captures: 
    //      capture 1 = the tool name
    //      capture 2 = the literal “{” from which we will parse balanced braces.
    if let Some(caps) = MASTER_REGEX.captures(response) {
        let tool_name = caps[1].to_string();
        let brace_start_index = caps.get(2).unwrap().start();

        // 2) Attempt to extract a balanced JSON block from that point.
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
        // If the MASTER_REGEX fails, we might do other fallback attempts, or return NoMatch:
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
    response: &str,
    server_name: &str,
    state: &mut ConversationState,
    client: &Box<dyn AIClient>,
    socket: Option<&mut WebSocket>
) -> Result<()> {
    state.add_assistant_message(response);

    let mut current_response = response.to_string();
    let mut iteration = 0;
    const MAX_ITERATIONS: i32 = 15;

    while iteration < MAX_ITERATIONS {
        log::debug!("\nStarting iteration {} of response handling", iteration + 1);
        log::debug!("Current response length: {} chars", current_response.len());
        log::debug!("Current response content:\n{}", current_response);

        let chunks: Vec<&str> = current_response.split("```").collect();
        log::debug!("Split response into {} chunks", chunks.len());
        
        // Process the entire response as a single unit
        log::debug!("Checking response for tool calls");
        match parse_tool_call(&current_response) {
                    ToolCallResult::Success(tool_name, args) => {
                        found_tool_call = true;
                        log::debug!("Tool: {}", tool_name);
                        log::debug!(
                            "Arguments: {}",
                            serde_json::to_string_pretty(&args).unwrap_or_default()
                        );
                        log::debug!("Tool call patterns matched successfully");

                        println!("{}", style("\nTool Call:").green().bold());
                        println!(
                            "└─ {}: {}\n",
                            style(&tool_name).yellow(),
                            crate::conversation_state::format_json_output(
                                &serde_json::to_string_pretty(&args)?
                            )
                        );

                        // Send tool start notification
                        if let Some(ref mut socket) = socket {
                            let start_msg = serde_json::json!({
                                "type": "tool_call_start",
                                "tool_name": tool_name
                            });
                            let _ = socket.send(Message::Text(start_msg.to_string())).await;
                        }

                        match host.call_tool(server_name, &tool_name, args).await {
                            Ok(result) => {
                                // Send tool end notification
                                if let Some(ref mut socket) = socket {
                                    let end_msg = serde_json::json!({
                                        "type": "tool_call_end",
                                        "tool_name": tool_name
                                    });
                                    let _ = socket.send(Message::Text(end_msg.to_string())).await;
                                }

                                println!(
                                    "{}",
                                    crate::conversation_state::format_tool_response(
                                        &tool_name,
                                        &result
                                    )
                                );
                                state.add_assistant_message(
                                    &format!("Tool '{}' returned: {}", tool_name, result.trim())
                                );
                            }
                            Err(e) => {
                                println!("{}: {}\n", style("Error").red().bold(), e);
                                state.add_assistant_message(
                                    &format!("Tool '{}' error: {}", tool_name, e).trim()
                                );
                                // Continue to next iteration even if tool call fails
                                continue;
                            }
                        }
                    }
                    ToolCallResult::NearMiss(feedback) => {
                        let feedback_msg = feedback.join("\n");
                        log::debug!("Near miss detected with feedback: {}", feedback_msg);
                        println!("{}", style("\nTool Call Format Error:").red().bold());
                        println!("└─ {}\n", feedback_msg);
                        state.add_assistant_message(&format!("Tool call format error:\n{}", feedback_msg));
                    }
                    ToolCallResult::NoMatch => {
                        log::debug!("No tool call pattern matched at ALL");
                        // Try to find any JSON-like content for debugging
                        
                    }
                }


        let mut builder = client.raw_builder();
        for msg in &state.messages {
            match msg.role {
                Role::System => {
                    builder = builder.system(msg.content.clone());
                }
                Role::User => {
                    builder = builder.user(msg.content.clone());
                }
                Role::Assistant => {
                    builder = builder.assistant(msg.content.clone());
                }
            }
        }

        log::debug!("Sending updated conversation to AI");
        match builder.execute().await {
            Ok(response_string) => {
                println!(
                    "\n{}",
                    crate::conversation_state::format_chat_message(
                        &Role::Assistant,
                        &response_string
                    )
                );
                state.add_assistant_message(&response_string);
                current_response = response_string;
            }
            Err(e) => {
                log::info!("Error getting response from API: {}", e);
                break;
            }
        }

        iteration += 1;
    }

    if iteration >= MAX_ITERATIONS {
        log::info!("Warning: Reached maximum number of tool call iterations");
    }

    Ok(())
}
