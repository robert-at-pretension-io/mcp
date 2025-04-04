use anyhow::Result;
use console::{style, Term};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use serde::{Deserialize, Serialize};

use crate::conversation_service::handle_assistant_response;

mod ai_client;
mod anthropic;
mod deepseek;
mod gemini;
mod conversation_service;
mod my_regex;
mod repl;
mod main_repl;

// Import MCPHost directly from the crate root
use mcp_host::MCPHost;
use log::{info,warn};
use tokio::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    command: String,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(rename = "mcpServers")]
    servers: HashMap<String, ServerConfig>,
}

use ai_client::AIClient;


mod conversation_state;
use conversation_state::ConversationState;
use std::io;
use anyhow::anyhow;
use log::{error,debug};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;
use regex::Regex;
use lazy_static::lazy_static;

async fn with_progress<F, T>(msg: String, future: F) -> T 
where
    F: std::future::Future<Output = T>,
{
    let term = Term::stderr();
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut i = 0;
    
    // Clone the message and term for the spawned task
    let progress_msg = msg.clone();
    let progress_term = term.clone();
    
    let handle = tokio::spawn(async move {
        loop {
            // Write the spinner and message, staying on same line
            progress_term.write_str(&format!("\r{} {}", spinner[i], progress_msg))
                .unwrap_or_default();
            // Ensure the line is flushed
            progress_term.flush().unwrap_or_default();
            
            i = (i + 1) % spinner.len();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let result = future.await;
    handle.abort();
    // Clear the progress line completely
    term.clear_line().unwrap_or_default();
    result
}

// Helper functions for parsing tool calls
fn extract_json_after_position(text: &str, pos: usize) -> Option<Value> {
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

fn find_any_json(text: &str) -> Option<Value> {
    let mut start_indices: Vec<usize> = text.match_indices('{').map(|(i, _)| i).collect();
    start_indices.sort_unstable(); // Sort in case there are multiple JSON objects

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

fn infer_tool_from_json(json: &Value) -> Option<(String, Value)> {
    // Common patterns to identify tools
    if json.get("action").is_some() {
        return Some(("graph_tool".to_string(), json.clone()));
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
    
    // If we can't infer the tool, return None
    None
}


use shared_protocol_objects::{
    JsonRpcRequest, JsonRpcResponse, ServerCapabilities, Implementation,
    ToolInfo, CallToolResult, RequestId, ListToolsResult, Role
};

// Server Management Types
#[derive(Debug)]
#[allow(dead_code)]
struct ManagedServer {
    name: String, 
    process: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<ChildStdout>>,
    capabilities: Option<ServerCapabilities>,
    initialized: bool,
}

// MCPHost has been moved to host.rs module


// Web interface has been removed

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting mcp_host application");

    info!("Initializing MCPHost");
    let host = MCPHost::new().await?;
    info!("MCPHost initialized successfully");

    let mut args: Vec<String> = std::env::args().collect();
    
    // Handle load_config argument if present
    if args.len() > 2 && args[1] == "load_config" {
        let config_path = &args[2];
        match host.load_config(config_path).await {
            Ok(()) => {
                info!("Successfully loaded configuration from {}", config_path);
            }
            Err(e) => {
                warn!("Error loading configuration: {}. Starting with empty config.", e);
            }
        }
        // Remove the load_config arguments to process remaining args
        args.drain(1..3);
    }

    // Run in CLI mode
    info!("Starting CLI interface");
    host.run_cli().await?;

    // Just exit - the servers will be cleaned up automatically

    Ok(())
}
