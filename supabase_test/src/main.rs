use anyhow::{anyhow, Result};
use log::{error, info};
use shared_protocol_objects::{
    ClientCapabilities, RootsCapability,
    rpc::{ProcessTransport, McpClientBuilder},
};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use std::time::Instant;

// Directly use the lower-level transport and request handling for debugging
use shared_protocol_objects::JsonRpcRequest;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
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

    // Create transport
    info!("Creating transport...");
    let transport = ProcessTransport::new(command).await?;
    info!("Transport created successfully");

    // Create client
    info!("Creating MCP client...");
    let mut client = McpClientBuilder::new(transport)
        .client_info("supabase-test", "1.0.0")
        .timeout(Duration::from_secs(30)) // Set a shorter timeout