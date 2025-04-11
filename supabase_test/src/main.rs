use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde_json::Value;
use shared_protocol_objects::{
    ClientCapabilities, JsonRpcRequest, JsonRpcResponse, RootsCapability, ToolsCapability,
    rpc::{ProcessTransport, McpClientBuilder},
};
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger with detailed output
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_millis()
        .init();
    
    info!("Supabase MCP Server Test Starting");

    // Set up command to run Supabase MCP server
    let mut command = Command::new("npx");
    command
        .arg("-y")
        .arg("@supabase/mcp-server-supabase@latest")
        .arg("--access-token")
        .arg("test_token") // Replace with your actual token or use SUPABASE_ACCESS_TOKEN
        .env("SUPABASE_ACCESS_TOKEN", "test_token"); // Redundant but ensures env var is set

    // Create transport with more detailed logging
    info!("Creating transport...");
    let start_time = Instant::now();
    let transport_result = ProcessTransport::new(command).await;
    
    let transport = match transport_result {
        Ok(t) => {
            info!("Transport created successfully in {:?}", start_time.elapsed());
            t
        }
        Err(e) => {
            error!("Failed to create transport: {:?}", e);
            return Err(anyhow!("Transport creation failed: {}", e));
        }
    };

    // Create client with improved error handling
    info!("Creating MCP client...");
    let client_start = Instant::now();
    let client = McpClientBuilder::new(transport)
        .client_info("supabase-test", "1.0.0")
        .timeout(Duration::from_secs(30)) // Set a timeout for requests
        .build();
    debug!("Client created in {:?}", client_start.elapsed());

    // Set up capabilities with detailed debugging
    let capabilities = ClientCapabilities {
        client: Some(shared_protocol_objects::Implementation {
            name: "supabase-test".to_string(),
            version: "1.0.0".to_string(),
        }),
        roots: Some(RootsCapability {
            list_changed: true,
        }),
        tools: Some(ToolsCapability {
            list_changed: true,
        }),
        ..Default::default()
    };
    
    debug!("Initializing client with capabilities: {:?}", capabilities);
    
    // Initialize the connection with timeout handling
    let init_start = Instant::now();
    debug!("Sending initialize request...");
    
    match timeout(Duration::from_secs(10), client.initialize(capabilities)).await {
        Ok(result) => {
            match result {
                Ok(server_capabilities) => {
                    info!("Successfully initialized connection in {:?}", init_start.elapsed());
                    debug!("Server capabilities: {:?}", server_capabilities);
                }
                Err(e) => {
                    error!("Initialization failed: {:?}", e);
                    return Err(anyhow!("Failed to initialize connection: {}", e));
                }
            }
        }
        Err(_) => {
            error!("Initialization timed out after 10 seconds");
            return Err(anyhow!("Connection initialization timed out"));
        }
    }
    
    // Allow server to fully initialize
    info!("Waiting for server to be ready...");
    sleep(Duration::from_millis(500)).await;
    
    // List tools with detailed error handling and debugging
    info!("Listing available tools...");
    let tools_start = Instant::now();
    
    match timeout(Duration::from_secs(10), client.list_tools()).await {
        Ok(result) => {
            match result {
                Ok(tools_result) => {
                    let elapsed = tools_start.elapsed();
                    info!("Successfully retrieved {} tools in {:?}", tools_result.tools.len(), elapsed);
                    
                    // Print detailed tool information
                    for (i, tool) in tools_result.tools.iter().enumerate() {
                        info!("Tool {}: {}", i + 1, tool.name);
                        if let Some(desc) = &tool.description {
                            debug!("  Description: {}", desc);
                        }
                        debug!("  Schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default());
                    }
                    
                    if let Some(cursor) = tools_result.next_cursor {
                        debug!("More tools available with cursor: {}", cursor);
                    }
                }
                Err(e) => {
                    error!("Failed to list tools: {:?}", e);
                }
            }
        }
        Err(_) => {
            warn!("List tools request timed out");
        }
    }

    // Demonstrate calling a tool with proper JSON-RPC handling
    if let Some(tool_name) = get_first_available_tool(&client).await {
        info!("Calling tool: {}", tool_name);
        let call_start = Instant::now();
        
        // Create a simple arguments object
        let args = serde_json::json!({
            "text": "Hello, world!",
        });
        
        debug!("Tool arguments: {}", serde_json::to_string_pretty(&args).unwrap_or_default());
        
        match timeout(Duration::from_secs(20), client.call_tool(&tool_name, args)).await {
            Ok(result) => {
                match result {
                    Ok(call_result) => {
                        info!("Tool call successful in {:?}", call_start.elapsed());
                        debug!("Response content count: {}", call_result.content.len());
                        
                        // Print each content item
                        for (i, content) in call_result.content.iter().enumerate() {
                            info!("Content {}: Type={}", i + 1, content.type_);
                            debug!("Content text: {}", content.text);
                            
                            if let Some(annotations) = &content.annotations {
                                debug!("Annotations: {}", serde_json::to_string_pretty(annotations).unwrap_or_default());
                            }
                        }
                    }
                    Err(e) => {
                        error!("Tool call failed: {:?}", e);
                    }
                }
            }
            Err(_) => {
                error!("Tool call timed out after 20 seconds");
            }
        }
    } else {
        warn!("No tools available to call");
    }

    // Clean shutdown
    info!("Test completed, shutting down");
    if let Err(e) = client.shutdown().await {
        warn!("Shutdown request failed: {:?}", e);
    }

    Ok(())
}

// Helper function to get the first available tool
async fn get_first_available_tool(client: &shared_protocol_objects::rpc::McpClient<ProcessTransport>) -> Option<String> {
    match client.list_tools().await {
        Ok(result) => {
            if let Some(tool) = result.tools.first() {
                Some(tool.name.clone())
            } else {
                None
            }
        }
        Err(e) => {
            warn!("Failed to get tools for demonstration: {:?}", e);
            None
        }
    }
