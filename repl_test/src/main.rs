use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    std::env::set_var("RUST_LOG", "trace,tokio=trace,runtime=trace");
    
    // Initialize the tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_file(true)
        .with_line_number(true)
        .init();
    
    println!("Current directory: {:?}", std::env::current_dir()?);
    
    println!("Creating command for mcp_tools process...");
    let mut command = Command::new("../target/debug/mcp_tools");
    command.kill_on_drop(true); // Ensure the process is killed when dropped
    command.env("RUST_LOG", "debug,tokio=debug,runtime=debug");
    command.env("MCP_TOOLS_ENABLED", "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task");
    
    // Add explicit pipe to capture output
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    
    // Spawn the tools server process
    let mut tools_process = command.spawn()?;
    
    // Set up readers for stdout and stderr
    let stdout = tools_process.stdout.take().unwrap();
    let stderr = tools_process.stderr.take().unwrap();
    
    println!("Tools server process spawned successfully with PID: {}", tools_process.id().unwrap_or(0));
    
    // Spawn background tasks to monitor stdout/stderr
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        
        loop {
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    println!("[TOOL STDOUT] {}", line.trim());
                    line.clear();
                },
                Err(e) => {
                    println!("[TOOL STDOUT ERROR] {}", e);
                    break;
                }
            }
        }
    });
    
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        
        loop {
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    println!("[TOOL STDERR] {}", line.trim());
                    line.clear();
                },
                Err(e) => {
                    println!("[TOOL STDERR ERROR] {}", e);
                    break;
                }
            }
        }
    });
    
    // Wait a bit for the process to start up
    println!("Waiting 2 seconds for process to initialize...");
    sleep(Duration::from_secs(2)).await;
    
    // Set up a timeout for the client test operation
    let test_future = async {
        // Let's try to read directly from the tools process's stdout
        println!("Let's run a very simple echo command direct to the tools process");
        
        // Create a direct process command
        let mut cmd = Command::new("../target/debug/mcp_tools");
        cmd.kill_on_drop(true);
        cmd.env("RUST_LOG", "debug,tokio=debug,runtime=debug");
        cmd.env("MCP_TOOLS_ENABLED", "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task");
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        
        println!("Spawning direct process");
        let mut process = cmd.spawn()?;
        
        // Get stdin
        let mut stdin = process.stdin.take().unwrap();
        
        // Write initialize request
        let init_req = r#"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"experimental":null,"roots":null,"sampling":null},"client_info":{"name":"test-repl","version":"1.0.0"},"protocol_version":"2025-03-26"},"id":1}"#;
        println!("Sending request directly: {}", init_req);
        stdin.write_all(format!("{}\n", init_req).as_bytes()).await?;
        stdin.flush().await?;
        
        // Read response directly
        let mut stdout = process.stdout.take().unwrap();
        let mut reader = BufReader::new(&mut stdout);
        let mut response = String::new();
        println!("Reading response directly");
        match reader.read_line(&mut response).await {
            Ok(0) => {
                println!("Process closed stdout without response");
            },
            Ok(n) => {
                println!("Got {} bytes directly: {}", n, response.trim());
            },
            Err(e) => {
                println!("Error reading response: {}", e);
            }
        }
        response.clear();
        
        // Try the tools/list request directly
        let tools_req = r#"{"jsonrpc":"2.0","method":"tools/list","params":null,"id":1}"#;
        println!("Sending tools/list request directly: {}", tools_req);
        stdin.write_all(format!("{}\n", tools_req).as_bytes()).await?;
        stdin.flush().await?;
        
        // Create a new process for the next request since we've consumed the stdout
        let mut cmd2 = Command::new("../target/debug/mcp_tools");
        cmd2.kill_on_drop(true);
        cmd2.env("RUST_LOG", "trace,tokio=trace,runtime=trace"); // Set more verbose logging level
        cmd2.env("MCP_TOOLS_ENABLED", "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task");
        cmd2.stdin(std::process::Stdio::piped());
        cmd2.stdout(std::process::Stdio::piped());
        cmd2.stderr(std::process::Stdio::piped());
        
        println!("Spawning second direct process");
        let mut process2 = cmd2.spawn()?;
        
        // Get stdin/stdout for the new process
        let mut stdin2 = process2.stdin.take().unwrap();
        let mut stdout2 = process2.stdout.take().unwrap();
        let mut reader2 = BufReader::new(&mut stdout2);
        
        // Initialize the new process
        println!("Initializing second process");
        stdin2.write_all(format!("{}\n", init_req).as_bytes()).await?;
        stdin2.flush().await?;
        
        let mut response2 = String::new();
        match reader2.read_line(&mut response2).await {
            Ok(n) => {
                println!("Got initialization response for second process: {} bytes", n);
            },
            Err(e) => {
                println!("Error reading init response for second process: {}", e);
            }
        }
        
        // Test different methods to find what works
        println!("Testing different tools/list formats");
        
        // Method 1: tools/list with null params (as object)
        let tools_req_1 = r#"{"jsonrpc":"2.0","method":"tools/list","params":null,"id":1}"#;
        println!("Sending format 1: {}", tools_req_1);
        stdin2.write_all(format!("{}\n", tools_req_1).as_bytes()).await?;
        stdin2.flush().await?;
        
        let mut resp1 = String::new();
        match reader2.read_line(&mut resp1).await {
            Ok(n) => println!("Response 1 ({} bytes): {}", n, resp1.trim()),
            Err(e) => println!("Error reading response 1: {}", e),
        }
        
        // Method 2: tools/list with empty params object
        let tools_req_2 = r#"{"jsonrpc":"2.0","method":"tools/list","params":{},"id":1}"#;
        println!("Sending format 2: {}", tools_req_2);
        stdin2.write_all(format!("{}\n", tools_req_2).as_bytes()).await?;
        stdin2.flush().await?;
        
        let mut resp2 = String::new();
        match reader2.read_line(&mut resp2).await {
            Ok(n) => println!("Response 2 ({} bytes): {}", n, resp2.trim()),
            Err(e) => println!("Error reading response 2: {}", e),
        }
        
        // Method 3: tools/list with different casing on method name
        let tools_req_3 = r#"{"jsonrpc":"2.0","method":"ToolsList","params":null,"id":1}"#;
        println!("Sending format 3: {}", tools_req_3);
        stdin2.write_all(format!("{}\n", tools_req_3).as_bytes()).await?;
        stdin2.flush().await?;
        
        let mut resp3 = String::new();
        match reader2.read_line(&mut resp3).await {
            Ok(0) => {
                println!("Process closed stdout without response");
            },
            Ok(n) => {
                println!("Response 3 ({} bytes): {}", n, resp3.trim());
            },
            Err(e) => {
                println!("Error reading response: {}", e);
            }
        }
        
        println!("Direct test completed");
        
        // Now use the shared_protocol_objects
        use shared_protocol_objects::rpc::{McpClientBuilder, ProcessTransport};
        
        // Create the client command
        println!("Creating client command...");
        let mut client_command = Command::new("../target/debug/mcp_tools");
        client_command.kill_on_drop(true);
        client_command.env("RUST_LOG", "debug,tokio=debug,runtime=debug");
        client_command.env("MCP_TOOLS_ENABLED", "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task");
        client_command.stdout(std::process::Stdio::piped());
        client_command.stderr(std::process::Stdio::piped());
        
        println!("Using MCP_TOOLS_ENABLED and RUST_LOG environment variables for client command");
        
        println!("Creating transport...");
        let transport = ProcessTransport::new(client_command).await?;
        println!("Transport created successfully");
        
        // Get the protocol version from the shared_protocol_objects
        use shared_protocol_objects::LATEST_PROTOCOL_VERSION;
        use shared_protocol_objects::SUPPORTED_PROTOCOL_VERSIONS;
        println!("Using protocol version: {}", LATEST_PROTOCOL_VERSION);
        println!("Supported protocol versions: {:?}", SUPPORTED_PROTOCOL_VERSIONS);
        
        // Create a client builder with debugging
        let builder = McpClientBuilder::new(transport)
            .client_info("test-repl", "1.0.0")
            .timeout(Duration::from_secs(15)) // Shorter timeout for easier debugging
            .numeric_ids(); // Use numeric IDs instead of UUIDs to match server expectations
        
        info!("Builder created with parameters");    
        info!("Client info: test-repl v1.0.0");
        
        // Let's see what's in the builder
        let mut client_builder = builder.build();
        info!("Client builder initialized, protocol version: {}", client_builder.protocol_version());
        
        // Initialize separately instead of using connect()
        info!("Preparing to initialize client...");
        let capabilities = shared_protocol_objects::ClientCapabilities {
            experimental: None,
            sampling: None,
            roots: None,
        };
        
        println!("Initializing client manually...");
        let server_capabilities = client_builder.initialize(capabilities).await?;
        info!("Client initialized, server capabilities: {:?}", server_capabilities);
            
        println!("Client connected successfully!");
        
        // Sending the initialized notification
        info!("Sending initialized notification...");
        client_builder.notify("notifications/initialized", None::<()>).await?;
        info!("Initialized notification sent");
        
        // Let's use the client directly instead of through the adapter
        println!("Skipping adapter creation and using client directly");
        
        println!("Let's try manual tools/list call directly to the process");
        
        // Create a direct manual call since the transport is having issues
        // We'll reuse the existing client's stdin/stdout
        let transport = client_builder.get_transport();
        
        // Let's try a different format with empty object params
        let tools_list_request = r#"{"jsonrpc":"2.0","method":"tools/list","params":{},"id":1}"#;
        println!("Sending manual tools/list request: {}", tools_list_request);
        
        // Create a direct manual client (fresh process)
        println!("Creating fresh process for tools/list request");
        let mut cmd3 = Command::new("../target/debug/mcp_tools");
        cmd3.kill_on_drop(true);
        cmd3.env("RUST_LOG", "trace,tokio=trace,runtime=trace");
        cmd3.env("MCP_TOOLS_ENABLED", "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task");
        cmd3.stdin(std::process::Stdio::piped());
        cmd3.stdout(std::process::Stdio::piped());
        cmd3.stderr(std::process::Stdio::piped());
        
        println!("Spawning fresh process");
        let mut process3 = cmd3.spawn()?;
        
        // First initialize
        let mut stdin3 = process3.stdin.take().unwrap();
        let init_req = r#"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"experimental":null,"roots":null,"sampling":null},"client_info":{"name":"test-repl","version":"1.0.0"},"protocol_version":"2025-03-26"},"id":1}"#;
        println!("Initializing fresh process");
        println!("JSONRPC >>> {}", init_req); // Log exactly what we're sending
        stdin3.write_all(format!("{}\n", init_req).as_bytes()).await?;
        stdin3.flush().await?;
        
        let mut stdout3 = process3.stdout.take().unwrap();
        let mut reader3 = BufReader::new(&mut stdout3);
        let mut init_response = String::new();
        
        match reader3.read_line(&mut init_response).await {
            Ok(n) => {
                println!("Fresh process init response ({} bytes): {}", n, init_response.trim());
                println!("JSONRPC <<< {}", init_response.trim()); // Log exactly what we're receiving
            },
            Err(e) => println!("Error reading init response for fresh process: {}", e),
        }
        
        // Now send tools/list
        println!("Sending tools/list to fresh process");
        println!("JSONRPC >>> {}", tools_list_request); // Log exactly what we're sending
        stdin3.write_all(format!("{}\n", tools_list_request).as_bytes()).await?;
        stdin3.flush().await?;
        
        let mut tools_response = String::new();
        match reader3.read_line(&mut tools_response).await {
            Ok(n) => {
                println!("Fresh process tools response ({} bytes): {}", n, tools_response.trim());
                
                // Try to parse the response as JSON
                match serde_json::from_str::<serde_json::Value>(&tools_response) {
                    Ok(json) => {
                        println!("Successfully parsed tools/list response as JSON");
                        
                        // Check for expected structure
                        if let Some(result) = json.get("result") {
                            if let Some(tools_obj) = result.as_object() {
                                if let Some(tools_array) = tools_obj.get("tools") {
                                    if let Some(tools) = tools_array.as_array() {
                                        println!("Found {} tools in direct response", tools.len());
                                        
                                        // Examine first tool structure to understand the format
                                        if let Some(first_tool) = tools.get(0) {
                                            println!("First tool structure: {}", first_tool);
                                            
                                            // Check fields we care about
                                            if let Some(name) = first_tool.get("name") {
                                                println!("Tool name: {}", name);
                                            }
                                            
                                            if let Some(description) = first_tool.get("description") {
                                                println!("Tool description: {}", description);
                                            }
                                        }
                                    } else {
                                        println!("Tools is not an array: {:?}", tools_array);
                                    }
                                } else {
                                    println!("Result doesn't contain 'tools' field: {:?}", tools_obj);
                                }
                            } else {
                                println!("Result is not an object: {:?}", result);
                            }
                        } else {
                            println!("Response doesn't contain 'result' field: {:?}", json);
                        }
                    },
                    Err(e) => println!("Failed to parse tools response as JSON: {}", e),
                }
            },
            Err(e) => println!("Error reading tools response for fresh process: {}", e),
        }
        
        // Continue with normal list_tools for comparison
        println!("Now trying with the regular list_tools method:");
        
        // Let's try both the client call method and the direct list_tools method
        println!("Approach 1: Using client_builder.call method:");
        // Define a proper struct to deserialize the tools/list response
        #[derive(serde::Deserialize, Debug)]
        struct ToolsListResponse {
            tools: Vec<shared_protocol_objects::ToolInfo>,
        }
        
        // Try with this struct for proper deserialization
        let call_result: Result<ToolsListResponse, _> = client_builder.call("tools/list", None::<()>).await;
        match call_result {
            Ok(result) => {
                println!("Call method succeeded with result containing {} tools", result.tools.len());
                
                // Print some tool info
                for (i, tool) in result.tools.iter().enumerate().take(2) {
                    println!("Tool {}: {} - {}", i+1, tool.name, tool.description.as_ref().map_or("", |d| d.split('\n').next().unwrap_or("")));
                }
                if result.tools.len() > 2 {
                    println!("... and {} more tools", result.tools.len() - 2);
                }
            },
            Err(e) => println!("Call method failed: {}", e),
        }
        
        println!("Approach 2: Using client_builder.list_tools method:");
        let list_tools_future = client_builder.list_tools();
        info!("List tools future created");
        
        // Use a shorter timeout for the list_tools call
        let tools = match tokio::time::timeout(Duration::from_secs(5), list_tools_future).await {
            Ok(result) => {
                match result {
                    Ok(tools) => {
                        println!("Tools listed successfully: {} tools", tools.len());
                        for (i, tool) in tools.iter().enumerate() {
                            println!("Tool {}: {}", i+1, tool.name);
                        }
                        tools
                    },
                    Err(e) => {
                        println!("Failed to list tools directly: {}", e);
                        info!("Error details: {:?}", e);
                        
                        // Let's trace through the request/response
                        info!("Making a simple empty notification call...");
                        let notification_result = client_builder.notify("notifications/progress", None::<()>).await;
                        info!("Notification result: {:?}", notification_result);
                        
                        return Err(e.into());
                    }
                }
            },
            Err(_) => {
                println!("Timed out waiting for tools list");
                
                // Let's check if the client is actually initialized
                info!("Checking client initialization status: {}", client_builder.is_initialized());
                
                // Let's try to make a simpler call
                info!("Trying to make a simple notification...");
                let notification_result = client_builder.notify("notifications/progress", None::<()>).await;
                info!("Notification result: {:?}", notification_result);
                
                return Err(anyhow::anyhow!("Timed out waiting for tools list"));
            }
        };
        
        // We need to use a different approach to test the bash tool
        println!("Running bash tool with a fresh process to avoid response mix-ups...");
        
        // Create a new client specifically for the bash tool test
        let mut bash_cmd = Command::new("../target/debug/mcp_tools");
        bash_cmd.kill_on_drop(true);
        bash_cmd.env("RUST_LOG", "debug");
        bash_cmd.env("MCP_TOOLS_ENABLED", "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task");
        bash_cmd.stdin(std::process::Stdio::piped());
        bash_cmd.stdout(std::process::Stdio::piped());
        bash_cmd.stderr(std::process::Stdio::piped());
        
        println!("Spawning process for bash tool test");
        let mut bash_process = bash_cmd.spawn()?;
        
        // Get stdin/stdout 
        let mut bash_stdin = bash_process.stdin.take().unwrap();
        let mut bash_stdout = bash_process.stdout.take().unwrap();
        let mut bash_reader = BufReader::new(&mut bash_stdout);
        
        // First initialize
        let init_req = r#"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"experimental":null,"roots":null,"sampling":null},"client_info":{"name":"test-repl","version":"1.0.0"},"protocol_version":"2025-03-26"},"id":1}"#;
        println!("Initializing bash test process");
        bash_stdin.write_all(format!("{}\n", init_req).as_bytes()).await?;
        bash_stdin.flush().await?;
        
        // Read init response
        let mut init_response = String::new();
        match bash_reader.read_line(&mut init_response).await {
            Ok(n) => println!("Bash test process init response ({} bytes): {}", n, init_response.trim()),
            Err(e) => println!("Error reading init response for bash test: {}", e),
        }
        
        // Now send tools/call for bash
        let bash_req = r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"bash","arguments":{"command":"echo Hello from the bash tool!"}},"id":2}"#;
        println!("Sending bash tool request: {}", bash_req);
        bash_stdin.write_all(format!("{}\n", bash_req).as_bytes()).await?;
        bash_stdin.flush().await?;
        
        // Read the response
        let mut bash_response = String::new();
        match bash_reader.read_line(&mut bash_response).await {
            Ok(n) => {
                println!("Bash tool response ({} bytes): {}", n, bash_response.trim());
                
                // Try to parse the response as JSON to see the format
                match serde_json::from_str::<serde_json::Value>(&bash_response) {
                    Ok(json) => {
                        if let Some(result) = json.get("result") {
                            if let Some(content) = result.get("content") {
                                println!("Content field found in bash tool response!");
                                
                                // Extract the text
                                if let Some(content_array) = content.as_array() {
                                    if !content_array.is_empty() {
                                        if let Some(text) = content_array[0].get("text") {
                                            println!("Bash tool output: {}", text);
                                        }
                                    }
                                }
                            } else {
                                println!("No content field in bash response: {:?}", result);
                            }
                        }
                    },
                    Err(e) => println!("Failed to parse bash tool response as JSON: {}", e),
                }
            },
            Err(e) => println!("Error reading bash tool response: {}", e),
        }
        
        // Let's also try the original approach for comparison
        println!("\nComparing with direct client_builder.call_tool approach:");
        let args = serde_json::json!({
            "command": "echo Hello from the client_builder approach!"
        });
        
        let call_tool_future = client_builder.call_tool("bash", args);
        info!("Call tool future created");
        
        match tokio::time::timeout(Duration::from_secs(5), call_tool_future).await {
            Ok(result) => {
                match result {
                    Ok(result) => {
                        println!("Tool executed successfully");
                        println!("Tool result: ");
                        for content in result.content {
                            println!("{}", content.text);
                        }
                    },
                    Err(e) => {
                        println!("Failed to call tool: {}", e);
                        info!("Error details: {:?}", e);
                        return Err(e.into());
                    }
                }
            },
            Err(_) => {
                println!("Timed out waiting for tool call");
                return Err(anyhow::anyhow!("Timed out waiting for tool call"));
            }
        }
        
        // Close the client directly
        println!("Closing client...");
        let close_future = client_builder.close();
        
        match tokio::time::timeout(Duration::from_secs(2), close_future).await {
            Ok(result) => {
                match result {
                    Ok(_) => println!("Client closed successfully"),
                    Err(e) => println!("Error closing client: {}", e),
                }
            },
            Err(_) => println!("Timed out closing client"),
        }
        
        Ok::<_, anyhow::Error>(())
    };
    
    // Apply timeout - give more time for the full test
    match tokio::time::timeout(Duration::from_secs(30), test_future).await {
        Ok(result) => {
            match result {
                Ok(_) => println!("Test completed successfully!"),
                Err(e) => println!("Test failed with error: {}", e),
            }
        },
        Err(_) => println!("Test timed out after 30 seconds!"),
    }
    
    // Kill the tools server process
    println!("Killing tools server process...");
    let _ = tools_process.start_kill();
    
    // Wait for the stdout/stderr tasks to complete
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    
    println!("Exiting...");
    Ok(())
}