Conversion Plan: Migrating to the Official Rust MCP SDK
1. Project Dependencies
Current Dependencies:
toml[dependencies]
shared_protocol_objects = { path = "../shared_protocol_objects" }
# ...other dependencies
Updated Dependencies:
toml[dependencies]
rmcp = { version = "0.1", features = ["server"] }
# Keep other dependencies as needed
2. Core Type Mappings
Current TypeRMCP TypeToolInformcp::model::ToolCallToolParamsrmcp::model::CallToolRequestParamCallToolResultrmcp::model::CallToolResultToolResponseContentrmcp::model::ContentJsonRpcResponseReturn type from the handlerErrorDatarmcp::error::Error
3. Tool Implementation Changes
A. Update the Tool Trait
Current Tool Trait:
rustpub trait Tool: Send + Sync + Debug {
    fn name(&self) -> &str;
    fn info(&self) -> shared_protocol_objects::ToolInfo;
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture;
}
New Implementation with RMCP:
rustuse rmcp::{model::*, ServerHandler};
use std::borrow::Cow;

#[tool(tool_box)]
trait MCPTool: ServerHandler {
    fn name(&self) -> Cow<'static, str>;
    
    #[tool(description = "Tool description here")]
    async fn execute(&self, #[tool(aggr)] params: JsonObject) -> String;
}
B. Sample Tool Conversion (Bash Tool)
Current Bash Tool:
rustimpl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        bash_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        Box::pin(async move {
            let bash_params: BashParams = serde_json::from_value(params.arguments)?;
            let executor = BashExecutor::new();
            
            match executor.execute(bash_params).await {
                Ok(result) => {
                    let text = format!(
                        "Command completed with status {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        result.status,
                        result.stdout,
                        result.stderr
                    );
                    
                    let tool_res = standard_tool_result(text, Some(!result.success));
                    Ok(standard_success_response(id, json!(tool_res)))
                }
                Err(e) => Err(anyhow!(e))
            }
        })
    }
}
New Bash Tool with RMCP:
rust#[derive(Debug, Clone)]
pub struct BashTool;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BashParams {
    #[schemars(description = "The bash command to execute")]
    pub command: String,
    #[schemars(description = "The working directory for the command")]
    pub cwd: Option<String>,
}

#[tool(tool_box)]
impl BashTool {
    #[tool(description = "Executes bash shell commands on the host system.")]
    async fn bash(&self, #[tool(aggr)] params: BashParams) -> Result<String, rmcp::Error> {
        let cwd = params.cwd.unwrap_or_else(|| std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
            .to_string_lossy().to_string());
            
        let executor = BashExecutor::new();
        let bash_params = BashParams {
            command: params.command,
            cwd,
        };
        
        let result = executor.execute(bash_params).await?;
        
        Ok(format!(
            "Command completed with status {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
            result.status,
            result.stdout,
            result.stderr
        ))
    }
}

#[tool(tool_box)]
impl ServerHandler for BashTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("A bash command execution tool".into()),
            ..Default::default()
        }
    }
}
4. Main Server Implementation Changes
A. Current Implementation:
rustasync fn main() {
    // ...setup code...
    
    let mut tool_impls = create_tools().await.unwrap_or_default();
    
    // ...more setup code...
    
    while let Some(Ok(line)) = lines.next().await {
        if line.trim().is_empty() {
            continue;
        }

        // Parsing and request handling logic
        // ...
    }
}
B. New Implementation with RMCP:
rustuse rmcp::{
    ServiceExt, ServerHandler, model::ServerInfo,
    transport::stdio,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    // ...

    // Create tool implementations 
    let tools = create_tools().await?;
    
    // Create a server instance
    let server = tools.serve(stdio()).await?;
    
    // Wait for server to complete processing
    server.waiting().await?;
    
    Ok(())
}

async fn create_tools() -> Result<impl ServerHandler> {
    // Create individual tools
    let bash_tool = BashTool;
    let scraping_tool = ScrapingBeeTool::new()?;
    let brave_search_tool = BraveSearchTool::new()?;
    let aider_tool = AiderTool;
    // ...etc
    
    // Combine using tool collection
    let tools = vec![
        bash_tool.into_dyn(),
        scraping_tool.into_dyn(),
        brave_search_tool.into_dyn(),
        aider_tool.into_dyn(),
        // ...add other tools
    ];
    
    Ok(tools)
}
5. Step-by-Step Migration Plan

Preparation Phase

Add rmcp as a dependency alongside existing shared_protocol_objects
Implement a small test tool with the new SDK to verify functionality


Tool Trait Conversion

Create a new version of the Tool trait using the rmcp macros
Implement a compatibility layer if needed for gradual migration


Tool-by-Tool Migration

Start with simpler tools (e.g., BashTool, MermaidChartTool)
Convert each tool's implementation to use the #[tool] macro patterns
Update the schema definitions to use schemars::JsonSchema
Refactor response handling to return Result<String> or Result<Content>


Server Implementation

Replace the custom message handling loop with rmcp::serve
Adapt the tool registration process to use the new SDK pattern
Migrate notification handling


Testing and Validation

Test each converted tool with standard inputs
Verify error handling functions correctly
Check that all features are maintained during conversion



6. Detailed Conversion Examples
A. ScrapingBee Tool Conversion
Current:
rustimpl Tool for ScrapingBeeTool {
    // implementation details...
}
New:
rust#[derive(Debug, Clone)]
pub struct ScrapingBeeTool {
    api_key: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ScrapingBeeParams {
    #[schemars(description = "The complete URL of the webpage to read and analyze")]
    pub url: String,
    
    #[schemars(description = "Whether to render JavaScript (default: true)")]
    #[serde(default = "default_render_js")]
    pub render_js: bool,
}

fn default_render_js() -> bool {
    true
}

impl ScrapingBeeTool {
    pub fn new() -> Result<Self, anyhow::Error> {
        let api_key = std::env::var("SCRAPINGBEE_API_KEY")
            .map_err(|_| anyhow::anyhow!("SCRAPINGBEE_API_KEY not set"))?;
        Ok(Self { api_key })
    }
}

#[tool(tool_box)]
impl ScrapingBeeTool {
    #[tool(description = "Web scraping tool that extracts and processes content from websites")]
    async fn scrape_url(
        &self, 
        #[tool(aggr)] params: ScrapingBeeParams
    ) -> Result<String, rmcp::Error> {
        let mut client = ScrapingBeeClient::new(self.api_key.clone());
        client
            .url(&params.url)
            .render_js(params.render_js)
            .block_resources(true)
            .block_ads(true);
            
        // Set a shorter timeout for faster responses if not using JS rendering
        if !params.render_js {
            client.timeout(8000); // 8 seconds is enough for static content
        }
        
        match client.execute().await {
            Ok(ScrapingBeeResponse::Text(body)) => {
                let markdown = extract_text_from_html(&body, Some(&params.url));
                
                // Truncate if needed
                const MAX_CHARS: usize = 25000;
                if markdown.chars().count() > MAX_CHARS {
                    let mut truncated = markdown.chars().take(MAX_CHARS).collect::<String>();
                    truncated.push_str("\n\n... (content truncated)");
                    Ok(truncated)
                } else {
                    Ok(markdown)
                }
            }
            Ok(ScrapingBeeResponse::Binary(_)) => {
                Err(rmcp::Error::invalid_params("Binary content not supported", None))
            }
            Err(e) => {
                Err(rmcp::Error::internal_error(
                    &format!("Scraping failed: {}", e),
                    None
                ))
            }
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for ScrapingBeeTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
    }
}
B. Progress Reporting Conversion
Current:
rustpub async fn send_progress_notification(
    tx_out: &mpsc::UnboundedSender<JsonRpcResponse>,
    params: &CallToolParams,
    progress: u32,
    total: u32
) -> Result<()> {
    // implementation details...
}
New:
rust// In the context of a tool implementation
async fn long_running_operation(
    &self, 
    params: LongRunningParams,
    context: RequestContext<RoleServer>
) -> Result<String, rmcp::Error> {
    let total_steps = 10;
    
    // Get progress token from context
    let progress_token = context.meta.progress_token.clone();
    
    for i in 1..=total_steps {
        // Do work for step i
        
        // Report progress using context's peer
        if let Some(peer) = context.peer.as_ref() {
            peer.notify_progress(
                ProgressNotificationParam {
                    progress_token: progress_token.clone(),
                    progress: i,
                    total: Some(total_steps),
                    message: Some(format!("Processing step {}/{}", i, total_steps)),
                }
            ).await?;
        }
        
        // Sleep to simulate work
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    
    Ok("Operation completed successfully".to_string())
}
7. Benefits of Migration

Standardization: Aligns with the official MCP specification and ecosystem
Simplified Code: Less boilerplate due to macro support
Type Safety: Better type definitions and schema generation
Maintainability: Future updates to the protocol will be handled by the SDK
Interoperability: Easier integration with other MCP-compatible systems