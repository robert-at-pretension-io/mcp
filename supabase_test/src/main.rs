use anyhow::{anyhow, Result as AnyhowResult};
use log::{debug, error, info, warn};
use serde_json::{self, json, Value};
use shared_protocol_objects::{
    JsonRpcRequest, JsonRpcNotification,
    rpc::{ProcessTransport, Transport},
};
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};
use std::time::Instant;

// Create a more explicit JSON-RPC request
fn create_request(method: &str, params: Option<Value>, id: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
        id: json!(id),
    }
}

// Create a JSON-RPC notification (no ID field)
fn create_notification(method: &str, params: Option<Value>) -> JsonRpcNotification {
    JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params: params.unwrap_or(json!(null)),
    }
}

#[tokio::main]
async fn main() -> AnyhowResult<()> {
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

    // Using direct transport access for more explicit control
    // This approach mirrors the Python script's method
    
    // Step 1: Send initialize request manually
    info!("Sending manual initialize request...");
    let init_params = json!({
        "protocolVersion": "2025-03-26",
        "capabilities": {
            "experimental": {},
            "sampling": {},
            "roots": {"list_changed": false}
        },
        "clientInfo": {
            "name": "supabase-rust-test",
            "version": "1.0.0"
        }
    });
    
    let init_request = create_request("initialize", Some(init_params), "init-123");
    info!("Request: {}", serde_json::to_string_pretty(&init_request).unwrap());
    
    // Send the request and wait for response
    let init_start = Instant::now();
    let init_response = match timeout(Duration::from_secs(10), transport.send_request(init_request)).await {
        Ok(result) => {
            match result {
                Ok(response) => {
                    info!("Got initialize response in {:?}", init_start.elapsed());
                    debug!("Response: {}", serde_json::to_string_pretty(&response).unwrap());
                    response
                }
                Err(e) => {
                    error!("Initialize request failed: {:?}", e);
                    return Err(e.into());
                }
            }
        }
        Err(_) => {
            error!("Initialize request timed out");
            return Err(anyhow!("Initialize request timed out"));
        }
    };
    
    // Check for errors in the response
    if let Some(error) = init_response.error {
        error!("Initialize error: {:?}", error);
        return Err(anyhow!("Initialize failed: {:?}", error));
    }
    
    // Extract relevant information from the response
    let result = init_response.result.ok_or_else(|| anyhow!("No result in initialize response"))?;
    info!("Server info: {}", result);
    
    // Step 2: Send initialized notification - CRITICAL for MCP protocol
    info!("Sending initialized notification...");
    let initialized_notification = create_notification("notifications/initialized", None);
    info!("Notification: {}", serde_json::to_string_pretty(&initialized_notification).unwrap());
    
    if let Err(e) = transport.send_notification(initialized_notification).await {
        error!("Failed to send initialized notification: {:?}", e);
        return Err(anyhow!("Failed to send initialized notification: {}", e));
    }
    
    // Wait after sending notification to ensure it's processed
    info!("Waiting for server to process notification...");
    sleep(Duration::from_millis(500)).await;
    
    // Step 3: Send tools/list request
    info!("Sending manual tools/list request...");
    let tools_request = create_request("tools/list", None, "tools-123");
    info!("Request: {}", serde_json::to_string_pretty(&tools_request).unwrap());
    
    // Send the request and wait for response
    let tools_start = Instant::now();
    let tools_response = match timeout(Duration::from_secs(10), transport.send_request(tools_request)).await {
        Ok(result) => {
            match result {
                Ok(response) => {
                    info!("Got tools/list response in {:?}", tools_start.elapsed());
                    debug!("Response: {}", serde_json::to_string_pretty(&response).unwrap());
                    response
                }
                Err(e) => {
                    error!("tools/list request failed: {:?}", e);
                    return Err(e.into());
                }
            }
        }
        Err(_) => {
            error!("tools/list request timed out");
            return Err(anyhow!("tools/list request timed out"));
        }
    };
    
    // Check for errors in the response
    if let Some(error) = tools_response.error {
        error!("tools/list error: {:?}", error);
        return Err(anyhow!("tools/list failed: {:?}", error));
    }
    
    // Extract tools information from the response
    let tools_result = tools_response.result.ok_or_else(|| anyhow!("No result in tools/list response"))?;
    match tools_result.get("tools") {
        Some(tools) => {
            if let Some(tools_array) = tools.as_array() {
                info!("Found {} tools", tools_array.len());
                
                // Print detailed tool information
                for (i, tool) in tools_array.iter().enumerate() {
                    if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                        info!("Tool {}: {}", i + 1, name);
                        
                        if let Some(desc) = tool.get("description").and_then(|d| d.as_str()) {
                            debug!("  Description: {}", desc);
                        }
                        
                        if let Some(schema) = tool.get("inputSchema") {
                            debug!("  Schema: {}", serde_json::to_string_pretty(schema).unwrap_or_default());
                        }
                    }
                }
            } else {
                warn!("tools is not an array: {:?}", tools);
            }
        }
        None => {
            warn!("No 'tools' field in response: {:?}", tools_result);
        }
    }
    
    // Clean shutdown
    info!("Test completed");
    Ok(())
}
