use std::io::{Read, Write};
use std::process::{Command, Stdio};
use serde_json::{json, Value};

fn main() {
    // Start the MCP tools server process
    println!("Starting MCP Tools server...");
    let mut child = Command::new("./target/debug/mcp_tools")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start mcp_tools");
    
    println!("Tools server started");
    
    // Send an initialize RPC request
    let initialize_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "0.3.0",
            "clientInfo": {
                "name": "test_client",
                "version": "1.0.0"
            },
            "capabilities": {}
        }
    });
    
    // Send the request
    let child_stdin = child.stdin.as_mut().expect("Failed to open stdin");
    let request_str = serde_json::to_string(&initialize_request).expect("Failed to serialize request");
    println!("Sending request: {}", request_str);
    child_stdin.write_all(request_str.as_bytes()).expect("Failed to write to stdin");
    child_stdin.write_all(b"\n").expect("Failed to write newline");
    child_stdin.flush().expect("Failed to flush stdin");
    
    // Read the response
    let mut child_stdout = child.stdout.as_mut().expect("Failed to open stdout");
    let mut buffer = String::new();
    child_stdout.read_to_string(&mut buffer).expect("Failed to read stdout");
    
    println!("Response: {}", buffer);
    
    // Parse the response
    if !buffer.is_empty() {
        let response: Value = serde_json::from_str(&buffer).expect("Failed to parse response");
        println!("Parsed response: {:?}", response);
    } else {
        println!("Empty response");
    }
    
    println!("Test completed");
}