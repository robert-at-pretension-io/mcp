#!/bin/bash
# Create a fixed simplified REPL that works with the tools server

# Set up environment
echo "Setting up environment..."
cd /home/elliot/Projects/mcp
export MCP_TOOLS_ENABLED="bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task"
export RUST_LOG=debug

# Make sure we have the tools server built
echo "Building tools server..."
cargo build -p mcp_tools

# Create a directory for our test
mkdir -p repl_test
cd repl_test

# Create a simple configuration file
cat > config.json << EOF
{
  "mcpServers": {
    "default": {
      "command": "../target/debug/mcp_tools",
      "env": {
        "RUST_LOG": "debug",
        "MCP_TOOLS_ENABLED": "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task"
      }
    }
  }
}
EOF

# Create a simple Rust program that uses the REPL components directly
cat > repl_main.rs << EOF
use std::path::PathBuf;
use std::process::Command;
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up logging
    env_logger::init();
    
    println!("Creating command for tool server...");
    let mut command = Command::new("../target/debug/mcp_tools");
    command.env("RUST_LOG", "debug");
    command.env("MCP_TOOLS_ENABLED", "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task");
    
    // Use the shared_protocol_objects to create a client
    use shared_protocol_objects::rpc::{McpClientBuilder, ProcessTransport};
    use shared_protocol_objects::client::ProcessClientAdapter;
    
    println!("Creating transport...");
    let transport = ProcessTransport::new(command).await?;
    
    println!("Creating client...");
    let client = McpClientBuilder::new(transport)
        .client_info("test-repl", "1.0.0")
        .connect().await?;
    
    println!("Creating adapter...");
    let adapter = Box::new(ProcessClientAdapter::new(client, "test".to_string()));
    
    println!("Listing tools...");
    let tools = adapter.list_tools().await?;
    
    println!("Found {} tools:", tools.len());
    for (i, tool) in tools.iter().enumerate() {
        println!("Tool {}: {}", i+1, tool.name);
    }
    
    println!("Running bash tool...");
    let args = serde_json::json!({
        "command": "echo Hello from the bash tool!"
    });
    
    let result = adapter.call_tool("bash", args).await?;
    
    println!("Tool result: ");
    for content in result.content {
        println!("{}", content.text);
    }
    
    println!("Test complete!");
    Ok(())
}
EOF

# Create a Cargo.toml
cat > Cargo.toml << EOF
[package]
name = "repl_test"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.20", features = ["full"] }
env_logger = "0.9"
serde_json = "1.0"
shared_protocol_objects = { path = "../shared_protocol_objects" }
EOF

# Build and run the test
echo "Building and running the test..."
RUST_LOG=debug cargo run

# Return to the original directory
cd ..