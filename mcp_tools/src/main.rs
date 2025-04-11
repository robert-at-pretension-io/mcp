// Keep only the necessary imports
use tracing::{error, info, Level};
use tracing_appender;
use tracing_subscriber::{self, EnvFilter};

// Import necessary rmcp components
use rmcp::{
    model::ServerInfo, // Needed for ServerHandler implementation
    tool,              // The tool attribute macro
    transport::stdio,  // For standard I/O transport
    ServerHandler,     // Trait for server handlers
    ServiceExt,        // For the .serve() method
};

// Import local modules needed
use mcp_tools::bash::{BashParams, BashTool}; // Import BashParams too
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

    // --- New SDK Server Structure ---
    #[derive(Debug, Clone)]
    struct McpToolServer {
        bash_tool: BashTool,
        scraping_tool: ScrapingBeeTool,
        // Add other tools here as they are converted
    }

    impl McpToolServer {
        fn new() -> Self {
            Self {
                bash_tool: BashTool::new(),
                scraping_tool: ScrapingBeeTool::new(),
            }
        }
    }

    // Implement the actual tool logic within the server struct
    #[tool(tool_box)] // Apply the SDK macro to generate list_tools/call_tool
    impl McpToolServer {
        // Re-implement the bash tool logic here, calling the original executor if needed
        // Or directly use the BashTool instance's method if it makes sense
        #[tool(description = "Executes bash shell commands on the host system. Use this tool to run system commands, check files, process text, manage files/dirs. Runs in a non-interactive `sh` shell.")]
        async fn bash(
            &self,
            #[tool(aggr)] params: BashParams, // Aggregate parameters
        ) -> String {
            // Delegate to the BashTool's implementation logic
            // Note: We might need to adjust BashTool's bash method slightly if it wasn't public
            // or refactor the core logic into a reusable function.
            // Assuming BashTool::bash is callable (might need adjustment in bash.rs)
            self.bash_tool.bash(params).await // Call the method on the instance
        }

        // Rename this method to match the tool's purpose better within the server context
        #[tool(description = "Web scraping tool that extracts and processes content from websites. Use for extracting text from webpages, documentation, and articles.")]
        async fn scrape_url( // Renamed from scrape to scrape_url
            &self,
            #[tool(aggr)] params: mcp_tools::scraping_bee::ScrapingBeeParams,
        ) -> String {
            // Delegate to ScrapingBeeTool's implementation
            self.scraping_tool.scrape_url(params).await // Call the correct method name
        }
    }

    // Implement ServerHandler for the server struct
    // The #[tool(tool_box)] macro can automatically implement this based on the tools defined above
    #[tool(tool_box)]
    impl ServerHandler for McpToolServer {
        // Optionally override get_info for custom server details
        fn get_info(&self) -> ServerInfo {
            ServerInfo {
                name: Some("MCP Tools Server (SDK)".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                description: Some("Provides various tools like bash execution and web scraping.".into()),
                instructions: Some("Use 'call' with tool name and parameters.".into()),
                ..Default::default() // Use defaults for other fields
            }
        }
    }
    // --- End New SDK Server Structure ---

    info!("Setting up tools with rmcp SDK...");
    let mcp_server = McpToolServer::new();
    info!("McpToolServer created with tools.");

    // Serve the McpToolServer instance
    info!("Initializing RMCP server...");
    let server = match mcp_server.serve(stdio()).await {
        Ok(s) => {
            info!("RMCP server started successfully.");
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

