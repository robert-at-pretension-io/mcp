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
use mcp_tools::scraping_bee::{ScrapingBeeTool, ScrapingBeeParams};
use mcp_tools::brave_search::{BraveSearchTool, BraveSearchParams};
use mcp_tools::long_running_task::{
    LongRunningTaskTool, StartTaskParams, GetStatusParams, ListTasksParams, StopTaskParams, ClearTasksParams // Added ClearTasksParams
};
use mcp_tools::aider::{AiderTool, AiderParams};
use mcp_tools::mermaid_chart::{MermaidChartTool, MermaidChartParams};
use mcp_tools::netlify::{NetlifyTool, NetlifyParams, NetlifyHelpParams}; // Added Netlify imports
// use mcp_tools::planner::{PlannerTool, PlannerParams};
// use mcp_tools::gmail_integration::{
//     GmailTool, AuthInitParams, AuthExchangeParams, SendMessageParams,
//     ListMessagesParams, ReadMessageParams, SearchMessagesParams, ModifyMessageParams
// };
// use mcp_tools::email_validator::{EmailValidatorTool, NeverBounceParams};

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
        brave_search_tool: BraveSearchTool,
        long_running_task_tool: LongRunningTaskTool,
        aider_tool: AiderTool,
        mermaid_chart_tool: MermaidChartTool,
        netlify_tool: NetlifyTool, // Added NetlifyTool field
        // planner_tool: PlannerTool,
        // gmail_tool: GmailTool,
        // email_validator_tool: EmailValidatorTool,
    }

    impl McpToolServer {
        fn new() -> Self {
            // Create the long-running task manager
            let task_tool = LongRunningTaskTool::new("tasks.json");
            let task_tool_clone = task_tool.clone();
            
            // Try to load any existing tasks
            tokio::spawn(async move {
                if let Err(e) = task_tool_clone.load_persistent_tasks().await {
                    error!("Failed to load persistent tasks: {}", e);
                }
            });
            
            Self {
                bash_tool: BashTool::new(),
                scraping_tool: ScrapingBeeTool::new(),
                brave_search_tool: BraveSearchTool::new(),
                long_running_task_tool: task_tool,
                aider_tool: AiderTool::new(),
                mermaid_chart_tool: MermaidChartTool::new(),
                netlify_tool: NetlifyTool::new(), // Instantiate NetlifyTool
                // planner_tool: PlannerTool::new(),
                // gmail_tool: GmailTool::new(),
                // email_validator_tool: EmailValidatorTool::new(),
            }
        }
    }

    // Implement the actual tool logic within the server struct
    #[tool(tool_box)] // Apply the SDK macro to generate list_tools/call_tool
    impl McpToolServer {
        // Re-implement the bash tool logic here, calling the original executor if needed
        #[tool(description = "Executes bash shell commands on the host system. Use this tool to run system commands, check files, process text, manage files/dirs. Runs in a non-interactive `sh` shell.")]
        async fn bash(
            &self,
            #[tool(aggr)] params: BashParams, // Aggregate parameters
        ) -> String {
            // Delegate to the BashTool's implementation logic
            self.bash_tool.bash(params).await // Call the method on the instance
        }

        // Web scraping tool implementation
        #[tool(description = "Web scraping tool that extracts and processes content from websites. Use for extracting text from webpages, documentation, and articles.")]
        async fn scrape_url(
            &self,
            #[tool(aggr)] params: ScrapingBeeParams,
        ) -> String {
            // Delegate to ScrapingBeeTool's implementation
            self.scraping_tool.scrape_url(params).await
        }
        
        // Brave search tool implementation
        #[tool(description = "Web search tool powered by Brave Search that retrieves relevant results from across the internet. Use this to find current information and facts from the web, research topics with multiple sources, verify claims, discover recent news and trends, or find specific websites and resources.")]
        async fn brave_search(
            &self,
            #[tool(aggr)] params: BraveSearchParams,
        ) -> String {
            // Delegate to BraveSearchTool's implementation
            self.brave_search_tool.brave_search(params).await
        }
        
        // Long-running task tools
        #[tool(description = "Start a new long-running shell task. Use this for any shell command that might take longer than 1 minute to complete, or for tasks that need to run in the background while other tools are used. The task runs asynchronously, continues after this conversation ends, and its status/output can be checked later using 'get_status' or 'list_tasks'.")]
        async fn start_task(
            &self,
            #[tool(aggr)] params: StartTaskParams,
        ) -> String {
            // Delegate to LongRunningTaskTool's implementation
            self.long_running_task_tool.start_task(params).await
        }
        
        #[tool(description = "Get the status and output of a long-running task. This will show if the task is still running and display its stdout/stderr.")]
        async fn get_status(
            &self,
            #[tool(aggr)] params: GetStatusParams,
        ) -> String {
            // Delegate to LongRunningTaskTool's implementation
            self.long_running_task_tool.get_status(params).await
        }
        
        #[tool(description = "List all tasks or filter by status (created, running, ended, error). Shows a summary of each task without the full output.")]
        async fn list_tasks(
            &self,
            #[tool(aggr)] params: ListTasksParams,
        ) -> String {
            // Delegate to LongRunningTaskTool's implementation
            self.long_running_task_tool.list_tasks(params).await
        }

        #[tool(description = "Stop a currently running background task. This attempts to gracefully terminate the process using SIGTERM, falling back to SIGKILL if necessary. Use this to cancel tasks that are no longer needed or are running indefinitely.")]
        async fn stop_task(
            &self,
            #[tool(aggr)] params: StopTaskParams,
        ) -> String {
            // Delegate to LongRunningTaskTool's implementation
            self.long_running_task_tool.stop_task(params).await
        }

        #[tool(description = "Stops all currently running tasks and removes ALL tasks (running, completed, errored, etc.) from the manager's memory and persistence file. Use with caution, as this permanently deletes task history.")]
        async fn clear_tasks(
            &self,
            #[tool(aggr)] params: ClearTasksParams,
        ) -> String {
            // Delegate to LongRunningTaskTool's implementation
            self.long_running_task_tool.clear_tasks(params).await
        }

        // Aider tool implementation
        #[tool(description = "AI pair programming tool for making targeted code changes. Requires VERY SPECIFIC instructions to perform well. It has NO CONTEXT from the conversation; all necessary details must be in the 'message'. Use for implementing new features, adding tests, fixing bugs, refactoring code, or making structural changes across multiple files.")]
        async fn aider(
            &self,
            #[tool(aggr)] params: AiderParams,
        ) -> String {
            // Delegate to AiderTool's implementation
            self.aider_tool.aider(params).await
        }
        
        // Mermaid chart tool implementation
        #[tool(description = "Generate a Mermaid chart from a collection of files. Provide a list of file paths, and this tool will create a string with their contents and generate a Mermaid diagram visualization.")]
        async fn mermaid_chart(
            &self,
            #[tool(aggr)] params: MermaidChartParams,
        ) -> String {
            // Delegate to MermaidChartTool's implementation
            self.mermaid_chart_tool.mermaid_chart(params).await
        }

        // Netlify tool implementations
        #[tool(description = "Executes Netlify CLI commands. Requires NETLIFY_AUTH_TOKEN env var. Provide the command arguments *after* 'netlify' (e.g., 'sites:list', 'deploy --prod').")]
        pub async fn netlify( // Added pub
            &self,
            #[tool(aggr)] params: NetlifyParams,
        ) -> String {
            // Delegate to NetlifyTool's implementation
            self.netlify_tool.netlify(params).await
        }

        #[tool(description = "Gets help for the Netlify CLI or a specific command. Does not require auth token.")]
        pub async fn netlify_help( // Added pub
            &self,
            #[tool(aggr)] params: NetlifyHelpParams,
        ) -> String {
            // Delegate to NetlifyTool's implementation
            self.netlify_tool.netlify_help(params).await
        }

        // // Planner tool implementation
        // #[tool(description = "Generates a multi-step plan using available tools to fulfill a user request. Provide the original user request, the AI's interpretation of that request, and a list of all available tools (including their descriptions and parameters). The tool will call a powerful LLM (Gemini) to devise a plan, including potential contingencies and points for reflection or waiting for results.")]
        // async fn planning_tool(
        //     &self,
        //     #[tool(aggr)] params: PlannerParams,
        // ) -> String {
        //     // Delegate to PlannerTool's implementation
        //     self.planner_tool.planning_tool(params).await
        // }
        
        // // Gmail integration tools
        // #[tool(description = "Initiates OAuth authentication flow for Gmail. Provides a URL for user to authorize access.")]
        // async fn auth_init(
        //     &self,
        //     #[tool(aggr)] params: AuthInitParams,
        // ) -> String {
        //     // Delegate to GmailTool's implementation
        //     self.gmail_tool.auth_init(params).await
        // }
        
        // #[tool(description = "Exchanges OAuth authorization code for access token. Use after completing the auth_init step.")]
        // async fn auth_exchange(
        //     &self,
        //     #[tool(aggr)] params: AuthExchangeParams,
        // ) -> String {
        //     // Delegate to GmailTool's implementation
        //     self.gmail_tool.auth_exchange(params).await
        // }
        
        // #[tool(description = "Sends an email message from your Gmail account. Requires prior authorization.")]
        // async fn send_message(
        //     &self,
        //     #[tool(aggr)] params: SendMessageParams,
        // ) -> String {
        //     // Delegate to GmailTool's implementation
        //     self.gmail_tool.send_message(params).await
        // }
        
        // #[tool(description = "Lists recent messages from your Gmail inbox. Requires prior authorization.")]
        // async fn list_messages(
        //     &self,
        //     #[tool(aggr)] params: ListMessagesParams,
        // ) -> String {
        //     // Delegate to GmailTool's implementation
        //     self.gmail_tool.list_messages(params).await
        // }
        
        // #[tool(description = "Reads the content of a specific Gmail message. Requires message ID and prior authorization.")]
        // async fn read_message(
        //     &self,
        //     #[tool(aggr)] params: ReadMessageParams,
        // ) -> String {
        //     // Delegate to GmailTool's implementation
        //     self.gmail_tool.read_message(params).await
        // }
        
        // #[tool(description = "Searches Gmail messages using Gmail search syntax. Requires prior authorization.")]
        // async fn search_messages(
        //     &self,
        //     #[tool(aggr)] params: SearchMessagesParams,
        // ) -> String {
        //     // Delegate to GmailTool's implementation
        //     self.gmail_tool.search_messages(params).await
        // }
        
        // #[tool(description = "Modifies Gmail message labels (archive, mark read/unread, star). Requires prior authorization.")]
        // async fn modify_message(
        //     &self,
        //     #[tool(aggr)] params: ModifyMessageParams,
        // ) -> String {
        //     // Delegate to GmailTool's implementation
        //     self.gmail_tool.modify_message(params).await
        // }
        
        // // Email validator tool
        // #[tool(description = "Validates email addresses using the NeverBounce API.")]
        // async fn never_bounce(
        //     &self,
        //     #[tool(aggr)] params: NeverBounceParams,
        // ) -> String {
        //     // Delegate to EmailValidatorTool's implementation
        //     self.email_validator_tool.never_bounce(params).await
        // }
    }

    // Implement ServerHandler for the server struct
    // The #[tool(tool_box)] macro can automatically implement this based on the tools defined above
    #[tool(tool_box)]
    impl ServerHandler for McpToolServer {
        // Override get_info for custom server details
        fn get_info(&self) -> ServerInfo {
            // Create the ServerInfo struct with the correct fields
            // Note: ServerInfo in rmcp 0.1.5 doesn't have protocol_version or capabilities fields directly.
            // These are part of the InitializeResult. ServerInfo focuses on implementation details.
            ServerInfo {
                 instructions: Some("Use 'call' with tool name and parameters.".into()),
                 ..Default::default() // Use defaults for other fields like icon, homepage_url
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
