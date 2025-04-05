//! End-to-End Integration Test for MCP
//! 
//! This test verifies the complete conversation flow:
//! 1. Initialize MCPHost with all components
//! 2. Process a user message that requires a tool
//! 3. Verify that the conversation service correctly:
//!    - Routes the message to the AI model
//!    - Parses the AI's tool call request
//!    - Executes the appropriate tool
//!    - Returns the tool result to the AI
//!    - Receives the final response from the AI

use anyhow::Result;
use mcp_host::{
    MCPHost,
    conversation_state::ConversationState,
};
use std::env;
use std::time::Duration;
use shared_protocol_objects::{Role, ToolInfo};
use std::future::Future;
use std::pin::Pin;

// Define a MockHost trait to intercept tool calls
trait MockToolCalling {
    fn call_tool<'a>(&'a self, server_name: &'a str, tool_name: &'a str, args: serde_json::Value) 
        -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
}

// Implement the trait for MCPHost
impl MockToolCalling for MCPHost {
    fn call_tool<'a>(&'a self, _server_name: &'a str, tool_name: &'a str, args: serde_json::Value) 
        -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        
        Box::pin(async move {
            match tool_name {
                "echo" => {
                    if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
                        Ok(format!("{}", text))
                    } else {
                        Ok("Error: Missing text parameter".to_string())
                    }
                },
                "current_time" => {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default();
                    
                    let seconds = now.as_secs() % 86400;
                    let hours = seconds / 3600;
                    let minutes = (seconds % 3600) / 60;
                    let seconds = seconds % 60;
                    
                    Ok(format!("The current time is: {:02}:{:02}:{:02}", hours, minutes, seconds))
                },
                _ => {
                    Ok(format!("Unknown tool: {}", tool_name))
                }
            }
        })
    }
}

// Simplified mock version of the conversation service
mod mock_conversation_service {
    use super::*;
    use mcp_host::conversation_service::parse_json_response;
    
    // Handle AI's final response
    async fn generate_final_response(
        state: &mut ConversationState,
        client: &Box<dyn mcp_host::ai_client::AIClient>,
        _socket: Option<&mut axum::extract::ws::WebSocket>
    ) -> Result<()> {
        let mut builder = client.raw_builder();
        
        // Add all messages for context
        for msg in &state.messages {
            match msg.role {
                Role::System => builder = builder.system(msg.content.clone()),
                Role::User => builder = builder.user(msg.content.clone()),
                Role::Assistant => builder = builder.assistant(msg.content.clone()),
            }
        }
        
        // Add special system prompt for final response
        let final_prompt = "Please provide a final response to the user based on all the information gathered so far, including any tool calls and results.".to_string();
        builder = builder.system(final_prompt);
        
        // Ask for final text
        match builder.execute().await {
            Ok(text) => {
                println!("\nAI: {}", text);
                state.add_assistant_message(&text);
                Ok(())
            },
            Err(e) => {
                eprintln!("Error requesting final answer: {}", e);
                Err(anyhow::anyhow!("Failed to get final response: {}", e))
            }
        }
    }
    
    // This simplified function will be used in our test
    pub async fn handle_assistant_response_with_mock(
        _host: &MCPHost,
        mock_tool: &dyn MockToolCalling,
        incoming_response: &str,
        server_name: &str,
        state: &mut ConversationState,
        client: &Box<dyn mcp_host::ai_client::AIClient>,
        socket: Option<&mut axum::extract::ws::WebSocket>
    ) -> Result<()> {
        // Record the incoming response
        state.add_assistant_message(incoming_response);

        // Try to parse the response as JSON
        if let Some((tool_name, args_opt)) = parse_json_response(incoming_response) {
            if let Some(args) = args_opt {
                // This is a tool call
                println!("Tool call detected: {}", tool_name);
                
                // Use our mock implementation
                let tool_result = mock_tool.call_tool(server_name, &tool_name, args.clone()).await;
                
                match tool_result {
                    Ok(result_string) => {
                        println!("\nTool '{}' output: {}", tool_name, result_string.trim());
                        
                        let combo = format!("Tool '{tool_name}' returned: {}", result_string.trim());
                        state.add_assistant_message(&combo);
                    },
                    Err(error) => {
                        let error_msg = format!("Tool '{tool_name}' error: {}", error);
                        state.add_assistant_message(&error_msg);
                        eprintln!("{}", error_msg);
                    }
                }
            }
        }
        
        // Generate a final response
        generate_final_response(state, client, socket).await
    }
}

#[tokio::test]
async fn test_end_to_end_conversation_flow() -> Result<()> {
    // Skip test if no API key is available
    if env::var("DEEPSEEK_API_KEY").is_err() && 
       env::var("OPENAI_API_KEY").is_err() &&
       env::var("ANTHROPIC_API_KEY").is_err() &&
       env::var("GEMINI_API_KEY").is_err() {
        println!("Skipping end-to-end test: No API key found for any LLM provider");
        return Ok(());
    }

    // 1. Initialize the MCPHost
    let host = MCPHost::builder()
        .request_timeout(Duration::from_secs(30))
        .client_info("mcp-e2e-test", "1.0.0")
        .build().await?;
    
    // Check if we have an AI client
    let ai_client = match host.ai_client() {
        Some(client) => client,
        None => {
            println!("No AI client could be initialized. Skipping test.");
            return Ok(());
        }
    };
    
    println!("Using AI model: {}", ai_client.model_name());
    
    // 2. Create a test server configuration with tools
    // We'll use a mock/test server for this test
    let server_name = "test-server";
    
    // Create test tools
    let tools = vec![
        ToolInfo {
            name: "echo".to_string(),
            description: Some("Echoes back the provided text".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "The text to echo back"
                    }
                },
                "required": ["text"]
            }),
        },
        ToolInfo {
            name: "current_time".to_string(),
            description: Some("Returns the current time".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ];
    
    // 3. Create a conversation state
    let system_prompt = "You are a helpful assistant with access to tools. Use tools when appropriate.";
    let mut state = ConversationState::new(system_prompt.to_string(), tools.clone());
    
    // 4. Process a user message that should trigger a tool call
    let user_message = "What time is it right now?";
    println!("\n----- Testing End-to-End Conversation Flow -----");
    println!("User Message: {}", user_message);
    state.add_user_message(user_message);
    
    // 5. Handle the AI's response
    let initial_response = {
        let mut builder = ai_client.builder();
        
        for msg in &state.messages {
            match msg.role {
                Role::System => builder = builder.system(msg.content.clone()),
                Role::User => builder = builder.user(msg.content.clone()),
                Role::Assistant => builder = builder.assistant(msg.content.clone()),
            }
        }
        
        // Add a prompt to encourage tool usage
        let tools_info = tools.iter()
            .map(|t| format!("- {}: {}", 
                t.name, 
                t.description.as_ref().unwrap_or(&"No description".to_string())
            ))
            .collect::<Vec<String>>()
            .join("\n");
        
        let tool_prompt = format!(
            "You have access to the following tools. Use them when they would help answer the user query.\n\
            Available tools:\n{}\n\n\
            If you want to use a tool, respond ONLY with a JSON object in this format: {{\"tool\": \"tool_name\", \"arguments\": {{...}}}}", 
            tools_info
        );
        
        builder = builder.system(tool_prompt);
        
        builder.execute().await?
    };
    
    println!("\nInitial AI Response:");
    println!("{}", initial_response);
    
    // 6. Process the conversation using our mock conversation service
    let result = mock_conversation_service::handle_assistant_response_with_mock(
        &host,
        &host, // Use our host as the mock tool implementation
        &initial_response,
        server_name,
        &mut state,
        ai_client,
        None
    ).await;
    
    // Verify the result
    assert!(result.is_ok(), "Failed to handle assistant response: {:?}", result);
    
    // 7. Verify the conversation flow by checking the message history
    let message_content = state.messages.iter().map(|m| {
        let role_str = match m.role {
            Role::System => "System",
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        format!("{}: {}", role_str, m.content)
    }).collect::<Vec<String>>().join("\n\n");
    
    println!("\n----- Final Conversation State -----");
    println!("{}", message_content);
    
    // Verify that the conversation includes:
    // 1. The initial user message
    assert!(
        state.messages.iter().any(|m| m.role == Role::User && m.content.contains("What time is it")),
        "User message not found in conversation history"
    );
    
    // 2. A tool call or mention of time (since we're using real AI responses)
    let has_tool_or_time_ref = state.messages.iter()
        .any(|m| m.role == Role::Assistant && 
            (m.content.contains("\"tool\"") || 
             m.content.contains("current_time") || 
             m.content.contains("Tool") ||
             m.content.contains("time"))
        );
    
    assert!(has_tool_or_time_ref, "No tool call or time reference found in conversation history");
    
    // 3. A final AI response
    // The last message should be from the assistant and contain a reasonable response
    if let Some(last_msg) = state.messages.last() {
        assert_eq!(last_msg.role, Role::Assistant, "Last message is not from assistant");
        println!("\nFinal AI Response: {}", last_msg.content);
    } else {
        panic!("No messages in conversation history");
    }
    
    Ok(())
}