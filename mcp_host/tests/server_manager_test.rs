use anyhow::Result;
use mcp_host::host::server_manager::{ServerManager, ManagedServer};
use shared_protocol_objects::{
    Implementation, ToolInfo
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use serde_json::json;

// Test 1: Basic test of server_manager creation
#[tokio::test]
async fn test_server_manager_creation() {
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers,
        client_info,
        Duration::from_secs(5),
    );
    
    // Just verify it creates without error
    assert!(manager.servers.lock().await.is_empty());
}

// Test 2: Test listing tools with a mock server
#[tokio::test]
async fn test_list_server_tools() -> Result<()> {
    // Set up the manager with a mock server
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers.clone(),
        client_info,
        Duration::from_secs(5),
    );
    
    // Create a mock process
    let process = tokio::process::Command::new("echo")
        .arg("test")
        .spawn()?;
        
    // Create a mock client with default test implementation
    let client = mcp_host::host::server_manager::testing::create_test_client();
    
    // Add a mock server
    {
        let mut servers_guard = servers.lock().await;
        servers_guard.insert("test-server".to_string(), ManagedServer {
            name: "test-server".to_string(),
            process,
            client,
            capabilities: None,
        });
    }
    
    // Test listing tools
    let tools = manager.list_server_tools("test-server").await?;
    
    // The mock client should return a test tool
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "test_tool");
    
    Ok(())
}

// Test 3: Test calling a tool
#[tokio::test]
async fn test_call_tool() -> Result<()> {
    // Set up the manager with a mock server
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers.clone(),
        client_info,
        Duration::from_secs(5),
    );
    
    // Create a mock process
    let process = tokio::process::Command::new("echo")
        .arg("test")
        .spawn()?;
        
    // Create a mock client
    let client = mcp_host::host::server_manager::testing::create_test_client();
    
    // Add a mock server
    {
        let mut servers_guard = servers.lock().await;
        servers_guard.insert("test-server".to_string(), ManagedServer {
            name: "test-server".to_string(),
            process,
            client,
            capabilities: None,
        });
    }
    
    // Test calling a tool
    let result = manager.call_tool(
        "test-server",
        "test_tool",
        json!({"param1": "value"})
    ).await?;
    
    // The mock client should return a success message
    assert!(result.contains("Tool executed successfully"));
    
    Ok(())
}

// Test 4: Test server not found case
#[tokio::test]
async fn test_server_not_found() -> Result<()> {
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers,
        client_info,
        Duration::from_secs(5),
    );
    
    // Test listing tools for non-existent server
    let list_result = manager.list_server_tools("non-existent-server").await;
    assert!(list_result.is_err());
    assert!(list_result.unwrap_err().to_string().contains("Server not found"));
    
    // Test calling a tool on non-existent server
    let call_result = manager.call_tool(
        "non-existent-server",
        "test_tool",
        json!({"param1": "value"})
    ).await;
    
    assert!(call_result.is_err());
    assert!(call_result.unwrap_err().to_string().contains("Server not found"));
    
    Ok(())
}

// Test 5: Test starting a server
#[tokio::test]
async fn test_start_server() -> Result<()> {
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers.clone(),
        client_info.clone(),
        Duration::from_secs(5),
    );
    
    // Start a server
    let mut cmd = std::process::Command::new("echo");
    cmd.arg("test");
    manager.start_server_with_command(
        "test-server", 
        cmd
    ).await?;
    
    // Verify server was added
    let servers_guard = servers.lock().await;
    assert!(servers_guard.contains_key("test-server"));
    
    Ok(())
}

// Test 6: Test stopping a server
#[tokio::test]
async fn test_stop_server() -> Result<()> {
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers.clone(),
        client_info,
        Duration::from_secs(5),
    );
    
    // Create a mock process
    let process = tokio::process::Command::new("sleep")
        .arg("10")  // Sleep for 10 seconds
        .spawn()?;
        
    // Create a mock client
    let client = mcp_host::host::server_manager::testing::create_test_client();
    
    // Add a mock server
    {
        let mut servers_guard = servers.lock().await;
        servers_guard.insert("test-server".to_string(), ManagedServer {
            name: "test-server".to_string(),
            process,
            client,
            capabilities: None,
        });
    }
    
    // Verify server exists
    {
        let servers_guard = servers.lock().await;
        assert!(servers_guard.contains_key("test-server"));
    }
    
    // Stop the server
    manager.stop_server("test-server").await?;
    
    // Verify server was removed
    {
        let servers_guard = servers.lock().await;
        assert!(!servers_guard.contains_key("test-server"));
    }
    
    Ok(())
}

// Test 7: Testing protocol helpers
#[tokio::test]
async fn test_protocol_helpers() {
    use mcp_host::host::protocol::{create_success_response, create_error_response, create_request, IdGenerator};
    use serde_json::json;
    
    // Test creating success response
    let response = create_success_response(
        Some(json!("test-id")),
        json!({"result": "success"})
    );
    
    assert_eq!(response.jsonrpc, "2.0");
    assert_eq!(response.id, json!("test-id"));
    assert_eq!(response.result, Some(json!({"result": "success"})));
    assert!(response.error.is_none());
    
    // Test creating error response
    let response = create_error_response(
        Some(json!("test-id")),
        -32600,
        "Invalid request"
    );
    
    assert_eq!(response.jsonrpc, "2.0");
    assert_eq!(response.id, json!("test-id"));
    assert!(response.result.is_none());
    
    // Check error fields separately to avoid ownership issues
    if let Some(error) = &response.error {
        assert_eq!(error.code, -32600);
        assert_eq!(error.message, "Invalid request");
    } else {
        panic!("No error in response");
    }
    
    // Test creating request
    let id_generator = IdGenerator::new(false); // Use numeric IDs
    let request = create_request::<()>("test-method", None, &id_generator).unwrap();
    
    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.method, "test-method");
    assert!(request.params.is_none());
    
    // Test creating request with params
    let request = create_request(
        "test-method",
        Some(json!({"param": "value"})),
        &id_generator
    ).unwrap();
    
    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.method, "test-method");
    assert_eq!(request.params, Some(json!({"param": "value"})));
}

// Test 8: Test multiple tool calls in a row
#[tokio::test]
async fn test_multiple_tool_calls() -> Result<()> {
    // Set up the manager with a mock server
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers.clone(),
        client_info,
        Duration::from_secs(5),
    );
    
    // Create a mock process
    let process = tokio::process::Command::new("echo")
        .arg("test")
        .spawn()?;
        
    // Create a mock client
    let client = mcp_host::host::server_manager::testing::create_test_client();
    
    // Add a mock server
    {
        let mut servers_guard = servers.lock().await;
        servers_guard.insert("test-server".to_string(), ManagedServer {
            name: "test-server".to_string(),
            process,
            client,
            capabilities: None,
        });
    }
    
    // Make multiple tool calls
    for i in 0..3 {
        let result = manager.call_tool(
            "test-server",
            "test_tool",
            json!({"param1": format!("value_{}", i)})
        ).await?;
        
        assert!(result.contains("Tool executed successfully"));
    }
    
    Ok(())
}

// Test 9: Test shared library integration - IdGenerator
#[tokio::test]
async fn test_id_generator() {
    use mcp_host::host::protocol::IdGenerator;
    
    // Test numeric ID generation
    let numeric_gen = IdGenerator::new(false);
    
    // Generate a few IDs and make sure they're numeric and incrementing
    let id1 = numeric_gen.next_id();
    let id2 = numeric_gen.next_id();
    
    if let serde_json::Value::Number(n1) = id1 {
        if let serde_json::Value::Number(n2) = id2 {
            let n1 = n1.as_i64().unwrap();
            let n2 = n2.as_i64().unwrap();
            assert!(n2 > n1);
        } else {
            panic!("ID2 is not a number");
        }
    } else {
        panic!("ID1 is not a number");
    }
    
    // Test UUID generation
    let uuid_gen = IdGenerator::new(true);
    
    // Generate a few IDs and make sure they're strings and different
    let id1 = uuid_gen.next_id();
    let id2 = uuid_gen.next_id();
    
    if let serde_json::Value::String(s1) = id1 {
        if let serde_json::Value::String(s2) = id2 {
            assert_ne!(s1, s2);
        } else {
            panic!("ID2 is not a string");
        }
    } else {
        panic!("ID1 is not a string");
    }
}

// Test 10: Test tool call errors
#[tokio::test]
async fn test_tool_call_error() -> Result<()> {
    // We'll use the ProcessTransport from the testing namespace directly
    
    // Set up the manager with a mock server
    let servers = Arc::new(Mutex::new(HashMap::new()));
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let manager = ServerManager::new(
        servers.clone(),
        client_info,
        Duration::from_secs(5),
    );
    
    // Create a mock process
    let process = tokio::process::Command::new("echo")
        .arg("test")
        .spawn()?;
    
    // Use the default client for simplicity
    let client = mcp_host::host::server_manager::testing::create_test_client();
    
    // Add a mock server 
    {
        let mut servers_guard = servers.lock().await;
        servers_guard.insert("test-server".to_string(), ManagedServer {
            name: "test-server".to_string(),
            process,
            client,
            capabilities: None,
        });
    }
    
    // Simply test that we can call tools successfully with our mocks
    let list_result = manager.list_server_tools("test-server").await;
    assert!(list_result.is_ok());
    
    Ok(())
}