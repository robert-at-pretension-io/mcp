// Keep only the necessary imports
use tracing::{error, info, Level};
use tracing_appender;
use tracing_subscriber::{self, EnvFilter};

// Import necessary rmcp components
use rmcp::{
    transport::stdio, // For standard I/O transport
    ServiceExt,       // For the .serve() method
    handler::server::tool::ToolBox, // For creating the ToolBox directly
};

// Import local modules needed
use mcp_tools::bash::BashTool; // Ensure BashTool is imported
use mcp_tools::scraping_bee::ScrapingBeeTool;
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

    // Create tools with the ToolBox struct directly
    info!("Setting up tools with rmcp SDK...");

    // Create a ToolBox to hold multiple tools
    let mut toolbox = ToolBox::new();
    info!("ToolBox created.");

    // Instantiate and add ScrapingBeeTool
    info!("Creating ScrapingBeeTool instance...");
    let scraping_tool = ScrapingBeeTool::new();
    toolbox.add_tool(scraping_tool);
    info!("ScrapingBeeTool added to ToolBox.");

    // Instantiate and add BashTool
    info!("Creating BashTool instance...");
    let bash_tool = BashTool::new();
    toolbox.add_tool(bash_tool);
    info!("BashTool added to ToolBox.");

    // Serve the ToolBox containing all tools
    info!("Initializing RMCP server with ToolBox...");
    let server = match toolbox.serve(stdio()).await {
        Ok(s) => {
            info!("RMCP server started successfully with ToolBox.");
            s
        }
        Err(e) => {
            error!("Failed to start RMCP server: {}", e);
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

