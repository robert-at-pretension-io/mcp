use std::io::{Read, Write};
use std::process::{Command, Stdio};
use serde_json::{json, Value};

fn main() {
    // Start the MCP tools server process
    println!("Starting MCP Tools server...");
    let mut child = Command::new("../target/debug/mcp_tools")
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
            "protocolVersion": "2025-03-26",
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
    let mut buffer = [0; 4096];
    let n = child_stdout.read(&mut buffer).expect("Failed to read stdout");
    let response_str = String::from_utf8_lossy(&buffer[0..n]);
    
    println!("Response: {}", response_str);
    
    // Parse the response
    if !response_str.is_empty() {
        match serde_json::from_str::<Value>(&response_str) {
            Ok(response) => println!("Parsed response: {:?}", response),
            Err(e) => println!("Failed to parse response: {}", e),
        }
    } else {
        println!("Empty response");
    }
    
    // Now try listing tools
    let list_tools_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });
    
    let list_request_str = serde_json::to_string(&list_tools_request).expect("Failed to serialize tools request");
    println!("\nSending tools/list request: {}", list_request_str);
    
    child_stdin.write_all(list_request_str.as_bytes()).expect("Failed to write to stdin");
    child_stdin.write_all(b"\n").expect("Failed to write newline");
    child_stdin.flush().expect("Failed to flush stdin");
    
    // Read the tools list response in chunks
    let mut buffer2 = [0; 4096];
    let mut tools_response = String::new();
    
    // Read in a loop until we get a complete response
    let mut attempts = 0;
    while attempts < 10 {
        match child_stdout.read(&mut buffer2) {
            Ok(n) => {
                if n > 0 {
                    let chunk = String::from_utf8_lossy(&buffer2[0..n]);
                    tools_response.push_str(&chunk);
                    println!("Received chunk of {} bytes", n);
                    
                    // Just collect chunks, we'll check for completeness outside the loop
                    if n < buffer2.len() {
                        // If we received fewer bytes than the buffer size, likely done
                        println!("Partial buffer - likely complete");
                        break;
                    }
                } else {
                    println!("EOF reached");
                    break;
                }
            },
            Err(e) => {
                println!("Error reading from stdout: {}", e);
                break;
            }
        }
        
        attempts += 1;
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    
    println!("Tools response length: {} bytes", tools_response.len());
    
    // Parse the tools response
    if !tools_response.is_empty() {
        match serde_json::from_str::<Value>(&tools_response) {
            Ok(response) => {
                println!("Successfully parsed JSON response");
                
                // Try to extract the tools list
                if let Some(result) = response.get("result") {
                    if let Some(tools) = result.get("tools") {
                        if let Some(tools_array) = tools.as_array() {
                            println!("Found {} tools:", tools_array.len());
                            for (i, tool) in tools_array.iter().enumerate() {
                                if let Some(name) = tool.get("name") {
                                    if let Some(name_str) = name.as_str() {
                                        println!("Tool {}: {}", i+1, name_str);
                                    }
                                }
                            }
                            println!("The tools server is working correctly!");
                        }
                    }
                }
            },
            Err(e) => println!("Failed to parse tools response: {}", e),
        }
    }
    
    // Kill the child process
    child.kill().expect("Failed to kill child process");
    
    println!("Test completed");
}