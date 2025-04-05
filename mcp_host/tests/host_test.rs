use mcp_host::host::{MCPHost, MCPHostBuilder, config::Config};
use shared_protocol_objects::ToolInfo;
use serde_json::json;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Simplified mock AI client for testing
struct MockAIClient;

#[async_trait::async_trait]
impl mcp_host::ai_client::AIClient for MockAIClient {
    fn builder(&self) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        unimplemented!("Not needed for host tests")
    }
    
    fn raw_builder(&self) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        unimplemented!("Not needed for host tests")
    }
    
    fn model_name(&self) -> String {
        "mock-model".to_string()
    }
    
    fn capabilities(&self) -> mcp_host::ai_client::ModelCapabilities {
        mcp_host::ai_client::ModelCapabilities::default()
    }
}

// Test the MCPHostBuilder functionality
#[tokio::test]
async fn test_host_builder() -> Result<()> {
    // Create a basic host with builder
    let host = MCPHostBuilder::new()
        .ai_client(Box::new(MockAIClient))
        .build().await?;
    
    // Verify AI client is set
    assert!(host.ai_client().is_some());
    assert_eq!(host.ai_client().unwrap().model_name(), "mock-model");
    
    Ok(())
}

// Test entering chat mode
#[tokio::test]
async fn test_enter_chat_mode() -> Result<()> {
    // Create a mock server tracker for testing
    let server_tracker = Arc::new(Mutex::new(HashMap::new()));
    
    // Create a host with the mock server tracker
    let host = MCPHost {
        server_manager: None,
        server_tracker: server_tracker.clone(),
        ai_client: Some(Box::new(MockAIClient)),
    };
    
    // Add a mock server with tools
    let tools = vec![
        ToolInfo {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "param1": {"type": "string"}
                }
            }),
        }
    ];
    
    // Insert a mock server in the tracker
    {
        let mut tracker = server_tracker.lock().unwrap();
        tracker.insert("test-server".to_string(), tools.clone());
    }
    
    // Test entering chat mode with a valid server
    let state = host.enter_chat_mode("test-server").await?;
    
    // Verify the state has the correct tools
    assert_eq!(state.tools.len(), 1);
    assert_eq!(state.tools[0].name, "test_tool");
    
    // Test entering chat with an invalid server
    let result = host.enter_chat_mode("non-existent-server").await;
    assert!(result.is_err());
    
    Ok(())
}

// Test calling tools
#[tokio::test]
async fn test_call_tool() -> Result<()> {
    // Create a mock server tracker for testing
    let server_tracker = Arc::new(Mutex::new(HashMap::new()));
    
    // Create a host with the mock server tracker
    let host = MCPHost {
        server_manager: None,
        server_tracker: server_tracker.clone(),
        ai_client: Some(Box::new(MockAIClient)),
    };
    
    // Add a mock server with tools
    let tools = vec![
        ToolInfo {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "param1": {"type": "string"}
                }
            }),
        }
    ];
    
    // Insert a mock server in the tracker
    {
        let mut tracker = server_tracker.lock().unwrap();
        tracker.insert("test-server".to_string(), tools.clone());
    }
    
    // Test calling non-existent tool
    let result = host.call_tool("test-server", "invalid-tool", json!({"param1": "value"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Tool not found"));
    
    // Test calling tool on non-existent server
    let result = host.call_tool("invalid-server", "test_tool", json!({"param1": "value"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Server not found"));
    
    Ok(())
}

// Test system prompt generation
#[tokio::test]
async fn test_generate_system_prompt() {
    // Create a host
    let host = MCPHost {
        servers: Arc::new(Mutex::new(HashMap::new())),
        client_info: shared_protocol_objects::Implementation {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
        },
        request_timeout: std::time::Duration::from_secs(5),
        ai_client: Some(Box::new(MockAIClient)),
    };
    
    // Test with empty tools
    let prompt = host.generate_system_prompt(&[]);
    assert!(prompt.contains("assistant"));
    assert!(prompt.contains("No tools available"));
    
    // Test with some tools
    let tools = vec![
        json!({
            "name": "calculator",
            "description": "Perform math calculations",
            "input_schema": {
                "type": "object",
                "properties": {
                    "expression": {"type": "string"}
                }
            }
        }),
        json!({
            "name": "search",
            "description": "Search the web",
            "input_schema": {
                "type": "object", 
                "properties": {
                    "query": {"type": "string"}
                }
            }
        })
    ];
    
    let prompt = host.generate_system_prompt(&tools);
    assert!(prompt.contains("assistant"));
    assert!(prompt.contains("calculator"));
    assert!(prompt.contains("Perform math calculations"));
    assert!(prompt.contains("search"));
    assert!(prompt.contains("Search the web"));
    assert!(prompt.contains("JSON schema"));
}

// Test configuration loading
#[tokio::test]
async fn test_load_config() -> Result<()> {
    // Create a host
    let host = MCPHost {
        servers: Arc::new(Mutex::new(HashMap::new())),
        client_info: shared_protocol_objects::Implementation {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
        },
        request_timeout: std::time::Duration::from_secs(5),
        ai_client: Some(Box::new(MockAIClient)),
    };
    
    // Create a test config
    let mut config = Config::default();
    config.ai_provider.provider = "mock".to_string();
    config.ai_provider.model = "mock-model".to_string();
    
    // Test configuring the host
    let result = host.configure(config).await;
    
    // This will fail since we don't have a real server manager
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Server manager not initialized"));
    
    Ok(())
}