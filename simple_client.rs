use std::env;
use std::process::Command;

fn main() {
    // Set environment variables to avoid logging conflicts
    println!("Starting MCP client...");
    
    // First start the tools server
    let mut tools_cmd = Command::new("cargo");
    tools_cmd.args(["run", "--package", "mcp_tools", "--bin", "mcp_tools"]);
    tools_cmd.env("RUST_LOG", "error");
    
    println!("Starting MCP Tools server...");
    println!("Command: cargo run --package mcp_tools --bin mcp_tools");
    println!("Run this in a separate terminal and keep it running.");
    println!();
    
    // Now build the mcp_repl command
    let mut run_cmd = Command::new("cargo");
    run_cmd.args(["run", "--package", "mcp_host", "--bin", "mcp_repl"]);
    
    // Pass through the API key if set
    if let Ok(api_key) = env::var("ANTHROPIC_API_KEY") {
        run_cmd.env("ANTHROPIC_API_KEY", api_key);
    } else {
        println!("Warning: No ANTHROPIC_API_KEY found in environment.");
        println!("You may need to set this to use certain AI models.");
    }
    
    // Disable conflicting logging
    run_cmd.env("RUST_LOG", "error");
    run_cmd.env("DISABLE_TRACING", "1");
    
    println!("Starting MCP Host REPL...");
    println!("Command: cargo run --package mcp_host --bin mcp_repl");
    println!("Run this in a separate terminal after the tools server is running.");
    println!();
    
    println!("When the REPL starts, connect to the tools server:");
    println!("  connect default localhost:3000");
    println!();
    println!("Then list available tools:");
    println!("  tools default");
    println!();
    println!("Start a chat session:");
    println!("  chat default");
    println!();
    println!("The AI will use smiley-delimited tool calling format (😊😊😊😊😊😊😊😊😊😊😊😊😊😊) when it needs to call tools.");
}