use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde_json::{self, json, Value};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};
use std::thread;

fn main() -> Result<()> {
    // Initialize logger
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .init();
    
    info!("Direct Supabase MCP Server Test Starting");

    // Set up command to run Supabase MCP server
    let mut command = Command::new("npx");
    command
        .arg("-y")
        .arg("@supabase/mcp-server-supabase@latest")
        .arg("--access-token")
        .arg("test_token") // Replace with actual token if needed
        .env("SUPABASE_ACCESS_TOKEN", "test_token")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    
    info!("Starting Supabase MCP server process...");
    let mut process = command.spawn()?;
    
    // Get handles to stdin and stdout
    let stdin = process.stdin.take().expect("Failed to open stdin");
    let stdout = process.stdout.take().expect("Failed to open stdout");
    let stderr = process.stderr.take().expect("Failed to open stderr");
    
    // Create thread for reading stderr
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => warn!("[STDERR] {}", line),
                Err(e) => error!("Error reading stderr: {}", e),
            }
        }
    });
    
    // Create buffered reader for stdout
    let mut stdout_reader = BufReader::new(stdout);
    let mut stdin_writer = stdin;
    
    // Give process time to start up
    info!("Waiting 1 second for server to start...");
    thread::sleep(Duration::from_secs(1));
    
    // Step 1: Send initialize request
    info!("Sending initialize request...");
    let init_request = json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "experimental": {},
                "sampling": {},
                "roots": {"list_changed": false}
            },
            "clientInfo": {
                "name": "direct-supabase-test",
                "version": "1.0.0"
            }
        },
        "id": "init-123"
    });
    
    let init_str = serde_json::to_string(&init_request)? + "\n";
    info!("Request: {}", init_request);
    
    stdin_writer.write_all(init_str.as_bytes())?;
    stdin_writer.flush()?;
    
    // Read the response
    let mut init_response = String::new();
    stdout_reader.read_line(&mut init_response)?;
    
    info!("Initialize response: {}", init_response.trim());
    
    // Parse the response
    let init_json: Value = serde_json::from_str(&init_response)?;
    
    // Step 2: Send initialized notification
    info!("Sending initialized notification...");
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": null
    });
    
    let notification_str = serde_json::to_string(&notification)? + "\n";
    info!("Notification: {}", notification);
    
    stdin_writer.write_all(notification_str.as_bytes())?;
    stdin_writer.flush()?;
    
    // Wait a bit for notification to be processed
    thread::sleep(Duration::from_millis(500));
    
    // Step 3: Send tools/list request
    info!("Sending tools/list request...");
    let tools_request = json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": null,
        "id": "tools-123"
    });
    
    let tools_str = serde_json::to_string(&tools_request)? + "\n";
    info!("Request: {}", tools_request);
    
    stdin_writer.write_all(tools_str.as_bytes())?;
    stdin_writer.flush()?;
    
    // Read the response with a timeout
    let start = Instant::now();
    let timeout = Duration::from_secs(10);
    
    info!("Waiting for tools/list response...");
    
    let mut tools_response = String::new();
    
    // Use read_line which is better for line-based protocols like JSON-RPC
    match stdout_reader.read_line(&mut tools_response) {
        Ok(n) => {
            info!("Read {} bytes from stdout", n);
            info!("Got tools/list response: {}", tools_response.trim());
            
            // Parse the tools list
            match serde_json::from_str::<Value>(&tools_response) {
                Ok(json) => {
                    if let Some(result) = json.get("result") {
                        if let Some(tools) = result.get("tools") {
                            if let Some(tools_array) = tools.as_array() {
                                info!("Found {} tools", tools_array.len());
                                
                                // Print details of each tool
                                for (i, tool) in tools_array.iter().enumerate() {
                                    info!("Tool {}: {}", i+1, tool.get("name").and_then(|n| n.as_str()).unwrap_or("unknown"));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to parse tools response: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Error reading from stdout: {}", e);
        }
    }
    
    // Clean up
    info!("Test completed, killing process");
    let _ = process.kill();
    
    Ok(())
}
