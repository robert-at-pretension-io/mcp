use anyhow::{ Result, anyhow };
use serde_json::Value;
use crate::MCPHost;
use crate::conversation_state::ConversationState;
use crate::ai_client::{ AIClient };
use console::style;

use lazy_static::lazy_static;
use regex::Regex;

use shared_protocol_objects::Role;

// Tool call parsing helpers
pub fn extract_json_after_position(text: &str, pos: usize) -> Option<Value> {
    if let Some(json_start) = text[pos..].find('{') {
        let start_pos = pos + json_start;
        let mut brace_count = 0;
        let mut end_pos = start_pos;

        for (i, c) in text[start_pos..].chars().enumerate() {
            match c {
                '{' => {
                    brace_count += 1;
                }
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = start_pos + i + 1;
                        break;
                    }
                }
                _ => {
                    continue;
                }
            }
        }

        if brace_count == 0 {
            if let Ok(json) = serde_json::from_str(&text[start_pos..end_pos]) {
                return Some(json);
            }
        }
    }
    None
}

pub fn find_any_json(text: &str) -> Option<Value> {
    let mut start_indices: Vec<usize> = text
        .match_indices('{')
        .map(|(i, _)| i)
        .collect();
    start_indices.sort_unstable();

    for start in start_indices {
        let mut brace_count = 0;
        let mut end_pos = start;

        for (i, c) in text[start..].chars().enumerate() {
            match c {
                '{' => {
                    brace_count += 1;
                }
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = start + i + 1;
                        break;
                    }
                }
                _ => {
                    continue;
                }
            }
        }

        if brace_count == 0 {
            if let Ok(json) = serde_json::from_str(&text[start..end_pos]) {
                return Some(json);
            }
        }
    }
    None
}

pub fn infer_tool_from_json(json: &Value) -> Option<(String, Value)> {
    if let Some(action) = json.get("action").and_then(|v| v.as_str()) {
        return Some((action.to_string(), json.clone()));
    }
    if json.get("query").is_some() {
        return Some(("brave_search".to_string(), json.clone()));
    }
    if json.get("url").is_some() {
        return Some(("scrape_url".to_string(), json.clone()));
    }
    if json.get("command").is_some() {
        return Some(("bash".to_string(), json.clone()));
    }
    if json.get("sequential_thinking").is_some() {
        return Some(("sequential_thinking".to_string(), json.clone()));
    }
    if json.get("memory").is_some() {
        return Some(("memory".to_string(), json.clone()));
    }
    if json.get("task_planning").is_some() {
        return Some(("task_planning".to_string(), json.clone()));
    }
    None
}

#[derive(Debug)]
pub enum ToolCallResult {
    Success(String, Value),
    NearMiss(Vec<String>),
    NoMatch,
}

pub fn parse_tool_call(response: &str) -> ToolCallResult {
    lazy_static! {
        static ref TOOL_CALL_REGEXES: Vec<Regex> = vec![
            // Pattern 1: Standard format with code fences
            Regex::new(
                r"(?s)(?:Let me call|I'll use|Using the)\s+`?([a-zA-Z_][a-zA-Z0-9_]*)`?\s*(?:tool)?(?:\s*with\s+(?:these\s+)?parameters)?:?\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),

            // Pattern 2: No code fences, direct JSON after tool name
            Regex::new(
                r"(?s)(?:Let me call|I'll use|Using the)\s+`?([a-zA-Z_][a-zA-Z0-9_]*)`?\s*(?:tool)?(?:\s*with\s+(?:these\s+)?parameters)?:?\s*\n*(\{.*?\})"
            ).unwrap(),

            // Pattern 3: Fallback - tool name followed by JSON block
            Regex::new(
                r"(?s)`?([a-zA-Z_][a-zA-Z0-9_]*)`?\s*(?:tool)?:?\s*\n*(?:```(?:json)?\s*)?(\{.*?\})(?:\s*```)?",
            ).unwrap(),
        ];
    }

    log::debug!("Parsing response for tool calls:\n{}", response);

    // Try each pattern in turn
    for (i, re) in TOOL_CALL_REGEXES.iter().enumerate() {
        log::debug!("Trying pattern {}", i + 1);
        
        for caps in re.captures_iter(response) {
            if caps.len() < 3 {
                log::debug!("Pattern {} matched but without enough capture groups", i + 1);
                continue;
            }

            let tool_name = caps[1].trim().to_string();
            let json_block = caps[2].trim();

            log::debug!("Found potential tool call: {}", tool_name);
            log::debug!("JSON block: {}", json_block);

            // Try parsing the JSON
            match serde_json::from_str::<Value>(json_block) {
                Ok(args) => {
                    log::debug!("Successfully parsed JSON for tool '{}'", tool_name);
                    return ToolCallResult::Success(tool_name, args);
                }
                Err(e) => {
                    let error = format!(
                        "Found tool call pattern for '{}' but JSON parsing failed: {}\nJSON block was: {}",
                        tool_name, e, json_block
                    );
                    log::warn!("{}", error);
                    return ToolCallResult::NearMiss(vec![error]);
                }
            }
        }
    }

    // If we get here, try to find any valid JSON that might contain a tool call
    log::debug!("No standard patterns matched, looking for any JSON blocks");
    if let Some(json) = find_any_json(response) {
        if let Some((tool_name, args)) = infer_tool_from_json(&json) {
            log::debug!("Inferred tool '{}' from JSON content", tool_name);
            return ToolCallResult::Success(tool_name, args);
        }
    }

    // If we get here, no valid tool calls were found
    log::debug!("No valid tool calls found in response");
    ToolCallResult::NoMatch
}

pub async fn handle_assistant_response(
    host: &MCPHost,
    response: &str,
    server_name: &str,
    state: &mut ConversationState,
    client: &Box<dyn AIClient>
) -> Result<()> {
    state.add_assistant_message(response);

    let mut current_response = response.to_string();
    let mut iteration = 0;
    const MAX_ITERATIONS: i32 = 15;

    while iteration < MAX_ITERATIONS {
        log::debug!("\nStarting iteration {} of response handling", iteration + 1);
        log::debug!("Current response length: {} chars", current_response.len());
        log::debug!("Current response content:\n{}", current_response);

        let mut found_tool_call = false;
        let chunks: Vec<&str> = current_response.split("```").collect();
        log::debug!("Split response into {} chunks", chunks.len());
        
        for (i, chunk) in chunks.iter().enumerate() {
            log::debug!("Processing chunk {} of length {}", i, chunk.len());
            log::debug!("Chunk content:\n{}", chunk);
            
            if i % 2 == 1 {
                log::debug!("Checking chunk {} for tool calls", i);
                match parse_tool_call(chunk) {
                    ToolCallResult::Success(tool_name, args) => {
                        found_tool_call = true;
                        log::debug!("Found tool call in chunk {}:", i);
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

                        match host.call_tool(server_name, &tool_name, args).await {
                            Ok(result) => {
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
                        log::debug!("No tool call pattern matched in this chunk");
                        // Try to find any JSON-like content for debugging
                        if chunk.contains("{") && chunk.contains("}") {
                            log::debug!("Found JSON-like content but no valid tool call pattern");
                        }
                    }
                }
            }
        }

        if !found_tool_call {
            break;
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
