use anyhow::{Result, anyhow};
use serde_json::Value;
use crate::MCPHost;
use crate::conversation_state::ConversationState;
use crate::ai_client::{AIClient};
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
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = start_pos + i + 1;
                        break;
                    }
                }
                _ => continue,
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
    let mut start_indices: Vec<usize> = text.match_indices('{').map(|(i, _)| i).collect();
    start_indices.sort_unstable();

    for start in start_indices {
        let mut brace_count = 0;
        let mut end_pos = start;
        
        for (i, c) in text[start..].chars().enumerate() {
            match c {
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = start + i + 1;
                        break;
                    }
                }
                _ => continue,
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

pub fn parse_tool_call(response: &str) -> Option<(String, Value)> {
    lazy_static! {
        static ref TOOL_PATTERNS: Vec<Regex> = vec![
            Regex::new(
                r"(?s)Let me call the `([^`]+)` tool with these parameters:\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            Regex::new(
                r"(?s)Let me call `([^`]+)` with these parameters:\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            Regex::new(
                r"(?s)Using the `([^`]+)` tool:?\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            Regex::new(
                r"(?s)I'll use `([^`]+)`:?\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            Regex::new(
                r"(?s)`([^`]+)`.*?```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            Regex::new(
                r"(?s)```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
        ];
    }

    // First try to match explicit tool call patterns
    for pattern in TOOL_PATTERNS.iter() {
        if let Some(captures) = pattern.captures(response) {
            match captures.len() {
                2 => {
                    if let Ok(json) = serde_json::from_str::<Value>(&captures[1]) {
                        if let Some(action) = json.get("action").and_then(|v| v.as_str()) {
                            return Some((action.to_string(), json));
                        }
                        return infer_tool_from_json(&json);
                    }
                },
                3 => {
                    let tool_name = captures[1].to_string();
                    if let Ok(args) = serde_json::from_str::<Value>(&captures[2]) {
                        return Some((tool_name.trim().to_string(), args));
                    }
                },
                _ => continue,
            }
        }
    }

    // If no explicit pattern matched, try to find any JSON and infer tool
    if let Some(json) = find_any_json(response) {
        if let Some(action) = json.get("action").and_then(|v| v.as_str()) {
            return Some((action.to_string(), json));
        }
        return infer_tool_from_json(&json);
    }

    None
}

pub async fn handle_assistant_response(
    host: &MCPHost,
    response: &str,
    server_name: &str,
    state: &mut ConversationState,
    client: &Box<dyn AIClient>,
) -> Result<()> {
    state.add_assistant_message(response);
    
    let mut current_response = response.to_string();
    let mut iteration = 0;
    const MAX_ITERATIONS: i32 = 15;
    
    while iteration < MAX_ITERATIONS {
        log::debug!("\nStarting iteration {} of response handling", iteration + 1);
        
        let mut found_tool_call = false;
        let chunks: Vec<&str> = current_response.split("```").collect();
        for (i, chunk) in chunks.iter().enumerate() {
            if i % 2 == 1 {
                if let Some((tool_name, args)) = parse_tool_call(chunk) {
                    found_tool_call = true;
                    log::debug!("Found tool call in chunk {}:", i);
                    log::debug!("Tool: {}", tool_name);
                    log::debug!("Arguments: {}", serde_json::to_string_pretty(&args).unwrap_or_default());
                    
                    println!("{}", style("\nTool Call:").green().bold());
                    println!("└─ {}: {}\n", 
                        style(&tool_name).yellow(),
                        crate::conversation_state::format_json_output(&serde_json::to_string_pretty(&args)?));

                    match host.call_tool(server_name, &tool_name, args).await {
                        Ok(result) => {
                            println!("{}", crate::conversation_state::format_tool_response(&tool_name, &result));
                            state.add_assistant_message(&format!("Tool '{}' returned: {}", tool_name, result.trim()));
                        }
                        Err(e) => {
                            println!("{}: {}\n", style("Error").red().bold(), e);
                            state.add_assistant_message(&format!("Tool '{}' error: {}", tool_name, e).trim());
                            // Continue to next iteration even if tool call fails
                            continue;
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
                Role::System => builder = builder.system(msg.content.clone()),
                Role::User => builder = builder.user(msg.content.clone()),
                Role::Assistant => builder = builder.assistant(msg.content.clone()),
            }
        }

        log::debug!("Sending updated conversation to AI");
        match builder.execute().await {
            Ok(response_string) => {
                println!("\n{}", crate::conversation_state::format_chat_message(&Role::Assistant, &response_string));
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
