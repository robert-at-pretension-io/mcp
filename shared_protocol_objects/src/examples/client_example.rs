use shared_protocol_objects::rpc::{McpClientBuilder, ProcessTransport};
use serde_json::json;
use std::time::Duration;
use tokio::process::Command;
use anyhow::Result;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a command to launch mcp_tools
    let mut command = Command::new("cargo");
    command.args(["run", "--bin", "mcp_tools"]);
    
    // Create a transport that communicates with the child process
    let transport = ProcessTransport::new(command).await?;
    
    // Create and initialize the client
    let client = McpClientBuilder::new(transport)
        .client_info("mcp-example-client", "0.1.0")
        .timeout(Duration::from_secs(60))
        .connect().await?;
        
    println!("Connected to server: {:?}", client.server_info());
    println!("Server capabilities: {:?}", client.capabilities());
    
    // List available tools
    let tools = client.list_tools().await?;
    println!("Available tools:");
    for tool in &tools {
        println!("  - {}: {}", 
            tool.name, 
            tool.description.as_deref().unwrap_or("No description"));
    }
    
    // Call bash tool if available
    if tools.iter().any(|t| t.name == "bash") {
        println!("\nExecuting bash command...");
        
        let result = client.call_tool("bash", json!({
            "command": "ls -la"
        })).await?;
        
        for content in result.content {
            println!("{}", content.text);
        }
    }
    
    // List available resources
    println!("\nListing resources...");
    match client.list_resources().await {
        Ok(resources) => {
            println!("Available resources:");
            for resource in resources {
                println!("  - {} ({})", resource.name, resource.uri);
            }
        }
        Err(e) => {
            println!("Failed to list resources: {}", e);
        }
    }
    
    // Clean up
    client.close().await?;
    println!("Client closed successfully");
    
    Ok(())
}