use shared_protocol_objects::rpc::{Transport, ProcessTransport};
use shared_protocol_objects::{JsonRpcRequest, ListToolsResult};
use tokio::process::Command;
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start the server process
    let mut cmd = Command::new("npx");
    cmd.args(["-y", "@supabase/mcp-server-supabase@latest", "--access-token", "sbp_6dd1b03bb0c829ebf4b2607a3a5e114ff607e83f"]);
    
    println!("Creating transport...");
    let transport = ProcessTransport::new(cmd).await?;
    
    // Initialize the server
    println!("Sending initialize request...");
    let init_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!("init_id"),
        method: "initialize".to_string(),
        params: Some(json!({
            "protocolVersion": "1.0",
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            },
            "capabilities": {
                "experimental": {},
                "sampling": {}
            }
        })),
    };
    
    let init_response = transport.send_request(init_request).await?;
    println!("Initialize response: {:?}", init_response);
    
    // Send initialized notification
    println!("Sending initialized notification...");
    let init_notification = shared_protocol_objects::JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "initialized".to_string(),
        params: json!({}),
    };
    transport.send_notification(init_notification).await?;
    
    // List tools
    println!("Sending tools/list request...");
    let list_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!("list_id"),
        method: "tools/list".to_string(),
        params: None,
    };
    
    let list_response = transport.send_request(list_request).await?;
    println!("List tools response: {:?}", list_response);
    
    // Parse the tools list
    if let Some(result) = list_response.result {
        let tools_result: ListToolsResult = serde_json::from_value(result)?;
        println!("Found {} tools:", tools_result.tools.len());
        for tool in tools_result.tools {
            println!("- {}: {}", tool.name, tool.description.unwrap_or_default());
        }
    }
    
    // Clean up
    transport.close().await?;
    Ok(())
}
