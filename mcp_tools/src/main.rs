// Remove imports related to the old manual implementation and shared_protocol_objects
// use futures::StreamExt;
// use serde_json::Value;
// use shared_protocol_objects::{...};
// use std::collections::HashMap;
// use std::sync::Arc;
// use tokio::io::{stdout, AsyncBufReadExt, AsyncWriteExt, BufReader};
// use tokio::sync::{mpsc, Mutex};
// use tokio::{io, task};
// use tokio_stream::wrappers::LinesStream;

// Keep tracing imports
use tracing::{error, info, Level}; // Removed debug, warn as they might not be needed directly
use tracing_appender;
use tracing_subscriber::{self, EnvFilter};

// Import necessary rmcp components
use rmcp::{
    transport::stdio, // For standard I/O transport
    ServiceExt,       // For the .serve() method
    // ServerHandler,    // Might be needed if implementing directly on tools Vec
    // model::ServerInfo // Might be needed for server info
};

// Import local modules needed
use mcp_tools::tool_impls::create_tools;
// use mcp_tools::long_running_task::LongRunningTaskManager; // Comment out for now

#[tokio::main]
async fn main() {
    // Set up file appender
    let log_dir = std::env::var("LOG_DIR")
        .unwrap_or_else(|_| format!("{}/Developer/mcp/logs", dirs::home_dir().unwrap().display()));
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::NEVER)
        .filename_prefix("mcp-server")
        .filename_suffix("log")
        .build(log_dir)
        .expect("Failed to create log directory");

    // Initialize the tracing subscriber with both stdout and file output
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(Level::DEBUG.into())
                .add_directive("mcp_tools=debug".parse().unwrap()),
        )
        .with_writer(non_blocking)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .init();

    info!("Starting MCP server (SDK)...");
    info!("RUST_LOG environment: {:?}", std::env::var("RUST_LOG"));
    info!("MCP_TOOLS_ENABLED: {:?}", std::env::var("MCP_TOOLS_ENABLED"));
    info!("Current directory: {:?}", std::env::current_dir().unwrap_or_default());
    info!("Process ID: {}", std::process::id());

    // TODO: Re-integrate LongRunningTaskManager when LongRunningTaskTool is converted to SDK
    // let my_manager = LongRunningTaskManager::new("tasks.json".to_string());
    // if let Err(err) = my_manager.load_persistent_tasks().await {
    //     error!("Failed to load tasks: {}", err);
    // }

    // Create tool implementations using the SDK-compatible factory
    let tools = match create_tools().await {
        Ok(tools) => {
            if tools.is_empty() {
                error!("No tools were created. Ensure at least one tool is converted and uncommented in tool_impls.rs");
                // Return an empty Vec to avoid panic, but log the error.
                // Alternatively, could implement a default handler or error out.
                Vec::new()
            } else {
                info!("Successfully created {} tool(s)", tools.len());
                tools
            }
        }
        Err(e) => {
            error!("Failed to create tools: {}", e);
            // Exit or return an empty Vec? For now, return empty.
            Vec::new()
        }
    };

    // TODO: Add LongRunningTaskTool when converted
    // let manager_arc = Arc::new(Mutex::new(my_manager.clone()));
    // tools.push(Box::new(LongRunningTaskTool::new(manager_arc)).into_dyn()); // Assuming conversion

    // Start the server using the rmcp SDK
    info!("Initializing RMCP server with stdio transport...");
    let server = match tools.serve(stdio()).await {
        Ok(s) => {
            info!("RMCP server started successfully.");
            s
        }
        Err(e) => {
            error!("Failed to start RMCP server: {}", e);
            // Consider exiting or handling the error appropriately
            return; // Exit if server fails to start
        }
    };

    // Keep the server running
    info!("Server is running, waiting for requests...");
    if let Err(e) = server.waiting().await {
        error!("Server encountered an error while running: {}", e);
    }

    info!("MCP server shutdown complete.");
}

