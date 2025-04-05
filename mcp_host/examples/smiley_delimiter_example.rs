use anyhow::Result;
use mcp_host::ai_client::{AIClient, AIClientFactory};
use mcp_host::conversation_service;
use mcp_host::conversation_state::ConversationState;
use mcp_host::host::{MCPHost, config::HostConfig};
use mcp_host::host::server_manager::ServerManager;
use mcp_host::smiley_tool_parser::SmileyToolParser;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use shared_protocol_objects::ToolInfo;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();
    
    // Get API key from environment
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .expect("API key not found in environment.");

    // Create a simple client with the API key
    let client_config = json!({
        "api_key": api_key,
        "model": "claude-3-sonnet-20240229" // or other model like "gpt-4"
    });
    
    // Create an AI client (will try to determine provider from available keys)
    let provider = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        "anthropic"
    } else {
        "openai"
    };
    
    let client = AIClientFactory::create(provider, client_config)?;
    
    // Initialize MCP Host and register a "demo" server
    let host_config = HostConfig::default();
    let server_manager = ServerManager::new(host_config);
    let host = Arc::new(Mutex::new(MCPHost::new(server_manager)));
    
    // Register our demo tools
    let demo_tools = vec![
        ToolInfo {
            name: "calculator".to_string(),
            description: Some("Calculate a mathematical expression".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "The mathematical expression to calculate"
                    }
                },
                "required": ["expression"]
            }),
        },
        ToolInfo {
            name: "weather".to_string(),
            description: Some("Get the current weather for a location".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The location to get weather for"
                    }
                },
                "required": ["location"]
            }),
        },
        ToolInfo {
            name: "search".to_string(),
            description: Some("Search for information on a given topic".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    }
                },
                "required": ["query"]
            }),
        },
    ];
    
    // Register our demo server with the host
    let server_name = "demo";
    {
        let mut host_lock = host.lock().await;
        host_lock.add_server_with_tools(server_name, demo_tools.clone()).await?;
    }
    
    // Create a basic system prompt
    let system_prompt = "You are a helpful AI assistant that can use various tools to help the user.";
    
    // Create a conversation state
    let mut state = ConversationState::new(system_prompt.to_string(), demo_tools);
    
    // User query that will likely require multiple tools
    let user_query = "What's 123 * 456? Also, what's the weather in Paris and can you search for information about rust programming language?";
    state.add_user_message(user_query);
    
    // Get response from AI model
    let mut builder = client.raw_builder();
    
    // Add all messages for context
    for msg in &state.messages {
        match msg.role {
            shared_protocol_objects::Role::System => builder = builder.system(msg.content.clone()),
            shared_protocol_objects::Role::User => builder = builder.user(msg.content.clone()),
            shared_protocol_objects::Role::Assistant => builder = builder.assistant(msg.content.clone()),
        }
    }
    
    println!("Sending user query to AI: {}", user_query);
    let response = builder.execute().await?;
    println!("\n===== INITIAL AI RESPONSE =====\n{}\n=======================\n", response);
    
    // Parse any tool calls from the response
    let tool_calls = SmileyToolParser::parse_tool_calls(&response);
    
    if tool_calls.is_empty() {
        println!("No tool calls found in the response.");
    } else {
        println!("Found {} tool call(s):", tool_calls.len());
        
        // Process each tool call
        for (i, tool_call) in tool_calls.iter().enumerate() {
            println!("Tool call #{}: {}", i+1, tool_call.name);
            println!("Arguments: {}", serde_json::to_string_pretty(&tool_call.arguments)?);
            
            // Mock tool execution (in a real scenario, these would be executed through the host)
            let tool_result = match tool_call.name.as_str() {
                "calculator" => {
                    if let Some(expr) = tool_call.arguments.get("expression").and_then(|e| e.as_str()) {
                        if expr == "123 * 456" {
                            "56088".to_string()
                        } else {
                            let result = eval_expression(expr);
                            format!("{}", result)
                        }
                    } else {
                        "Error: Missing expression argument".to_string()
                    }
                },
                "weather" => {
                    if let Some(location) = tool_call.arguments.get("location").and_then(|l| l.as_str()) {
                        format!("Weather in {}: 22°C, Partly Cloudy", location)
                    } else {
                        "Error: Missing location argument".to_string()
                    }
                },
                "search" => {
                    if let Some(query) = tool_call.arguments.get("query").and_then(|q| q.as_str()) {
                        format!("Search results for '{}': Rust is a systems programming language focused on safety, speed, and concurrency.", query)
                    } else {
                        "Error: Missing query argument".to_string()
                    }
                },
                _ => format!("Error: Unknown tool '{}'", tool_call.name)
            };
            
            println!("Tool result: {}\n", tool_result);
            
            // In a real scenario, we would add this to the conversation state
            state.add_assistant_message(&format!("Tool '{}' returned: {}", tool_call.name, tool_result));
        }
        
        // Get the next response from the AI with the tool results
        let mut builder = client.raw_builder();
        
        // Add all messages for context
        for msg in &state.messages {
            match msg.role {
                shared_protocol_objects::Role::System => builder = builder.system(msg.content.clone()),
                shared_protocol_objects::Role::User => builder = builder.user(msg.content.clone()),
                shared_protocol_objects::Role::Assistant => builder = builder.assistant(msg.content.clone()),
            }
        }
        
        // Add a prompt to encourage the AI to provide a final response
        builder = builder.system(
            "Based on the tool results, please provide a final response to the user's request."
        );
        
        let final_response = builder.execute().await?;
        println!("\n===== FINAL AI RESPONSE =====\n{}\n=======================\n", final_response);
    }
    
    Ok(())
}

// Simple expression evaluator for demo purposes
fn eval_expression(expr: &str) -> f64 {
    // This is a very simplified parser for basic math expressions
    // In a real app, you'd use a proper parser
    let expr = expr.replace(" ", "");
    
    if expr.contains('+') {
        let parts: Vec<&str> = expr.split('+').collect();
        if parts.len() == 2 {
            return eval_expression(parts[0]) + eval_expression(parts[1]);
        }
    } else if expr.contains('-') {
        let parts: Vec<&str> = expr.split('-').collect();
        if parts.len() == 2 {
            return eval_expression(parts[0]) - eval_expression(parts[1]);
        }
    } else if expr.contains('*') {
        let parts: Vec<&str> = expr.split('*').collect();
        if parts.len() == 2 {
            return eval_expression(parts[0]) * eval_expression(parts[1]);
        }
    } else if expr.contains('/') {
        let parts: Vec<&str> = expr.split('/').collect();
        if parts.len() == 2 {
            return eval_expression(parts[0]) / eval_expression(parts[1]);
        }
    }
    
    // Try to parse as a simple number
    expr.parse::<f64>().unwrap_or(0.0)
}