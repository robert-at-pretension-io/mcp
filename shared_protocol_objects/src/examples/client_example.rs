use anyhow::Result;
use serde_json::json;
use shared_protocol_objects::rpc::{McpClientBuilder, ProcessTransport};
use std::time::Duration;
use tokio::process::Command;


#[tokio::main]
async fn main() -> Result<()> { // Use anyhow::Result
    // Create a command to launch mcp_tools
    let mut command = Command::new("cargo");
    command.args(["run", "--bin", "mcp_tools"]);
    
    // Create a transport that communicates with the child process
    let transport = ProcessTransport::new(command).await?;
    
    // Create and initialize the client
    let client = McpClientBuilder::new(transport)
        .client_info("mcp-example-client", "0.1.0")
        .timeout(Duration::from_secs(60))
        .connect()
        .await?;

    println!("Connected to server: {:?}", client.server_info());
    println!("Server capabilities: {:?}", client.capabilities());

    // List available tools
    let list_tools_result = client.list_tools().await?;
    println!("Available tools:");
    for tool in &list_tools_result.tools {
        println!(
            "  - {}: {}",
            tool.name,
            tool.description.as_deref().unwrap_or("No description")
        );
    }
    if let Some(cursor) = list_tools_result.next_cursor {
         println!("  (More tools available, next cursor: {})", cursor);
    }


    // Call bash tool if available
    if list_tools_result.tools.iter().any(|t| t.name == "bash") {
        println!("\nExecuting bash command 'ls -la'...");

        let result = client.call_tool("bash", json!({
            "command": "ls -la",
            // Example: Add cwd if needed by the tool schema
            // "cwd": "/tmp"
        }))
        .await?;

        println!("Bash tool result:");
        for content in result.content {
            println!("{}", content.text);
        }
        if result.is_error.unwrap_or(false) {
             println!("(Tool reported an error)");
        }
    } else {
         println!("\nBash tool not found in server capabilities.");
    }

    // List available resources
    println!("\nListing resources...");
    match client.list_resources().await {
        Ok(list_resources_result) => {
            println!("Available resources:");
            if list_resources_result.resources.is_empty() {
                 println!("  (No resources listed by server)");
            }
            for resource in list_resources_result.resources {
                println!("  - {} ({})", resource.name, resource.uri);
            }
            if let Some(cursor) = list_resources_result.next_cursor {
                 println!("  (More resources available, next cursor: {})", cursor);
            }
        }
        Err(e) => {
            eprintln!("Failed to list resources: {}", e); // Use eprintln for errors
        }
    }

    // Example: Read a resource if one exists (replace with actual URI if known)
    // if let Some(resource_uri) = get_some_resource_uri() {
    //     println!("\nReading resource: {}...", resource_uri);
    //     match client.read_resource(&resource_uri).await {
    //         Ok(read_result) => {
    //             for content in read_result.contents {
    //                 println!("Content ({}):", content.mime_type.as_deref().unwrap_or("unknown"));
    //                 println!("{}", content.text.as_deref().unwrap_or("(no text content)"));
    //             }
    //         }
    //         Err(e) => {
    //             eprintln!("Failed to read resource {}: {}", resource_uri, e);
    //         }
    //     }
    // }


    // Clean up
    client.close().await?;
    println!("Client closed successfully");
    
    Ok(())
}
