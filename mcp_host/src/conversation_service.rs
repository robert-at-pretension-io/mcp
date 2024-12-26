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

#[derive(Debug)]
pub enum ToolCallResult {
    Success(String, Value),
    NearMiss(Vec<String>),
    NoMatch,
}

pub fn parse_tool_call(response: &str) -> ToolCallResult {
    lazy_static! {
        // Single strict pattern
        static ref TOOL_PATTERN: Regex = Regex::new(
            r"(?s)Let me call ([a-zA-Z_][a-zA-Z0-9_]*)\n```json\n(\{.*?\})\n```"
        ).unwrap();

        // "Near miss" patterns to provide helpful feedback
        static ref NEAR_MISS_PATTERNS: Vec<(Regex, &'static str)> = vec![
            (
                Regex::new(r"`([^`]+)`").unwrap(),
                "Tool name should not be in backticks. Use: Let me call tool_name"
            ),
            (
                Regex::new(r"```\s*\{").unwrap(),
                "JSON block must start with ```json on its own line"
            ),
            (
                Regex::new(r"\{.*\}\s*```").unwrap(),
                "JSON block must end with ``` on its own line"
            ),
            (
                Regex::new(r"Let me use|I'll use|Using the|Call the").unwrap(),
                "Must start with exactly 'Let me call'"
            ),
        ];
    }

    // Try the strict pattern first
    if let Some(captures) = TOOL_PATTERN.captures(response) {
        let tool_name = captures[1].to_string();
        if let Ok(args) = serde_json::from_str(&captures[2]) {
            return ToolCallResult::Success(tool_name, args);
        } else {
            log::warn!("Found tool call pattern but JSON parsing failed: {}", &captures[2]);
            return ToolCallResult::NearMiss(vec!["JSON parsing failed".to_string()]);
        }
    }

    // If no match, check for near misses and collect feedback
    let mut feedback = Vec::new();
    for (pattern, message) in NEAR_MISS_PATTERNS.iter() {
        if pattern.is_match(response) {
            log::warn!("Tool call format near miss: {}", message);
            feedback.push(message.to_string());
        }
    }

    if !feedback.is_empty() {
        ToolCallResult::NearMiss(feedback)
    } else {
        ToolCallResult::NoMatch
    }
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
                match parse_tool_call(chunk) {
                    ToolCallResult::Success(tool_name, args) => {
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
                    ToolCallResult::NearMiss(feedback) => {
                        let feedback_msg = feedback.join("\n");
                        println!("{}", style("\nTool Call Format Error:").red().bold());
                        println!("└─ {}\n", feedback_msg);
                        state.add_assistant_message(&format!("Tool call format error:\n{}", feedback_msg));
                    }
                    ToolCallResult::NoMatch => {
                        // No tool call found in this chunk, continue
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
