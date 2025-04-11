use crate::aider::{aider_tool_info, handle_aider_tool_call, AiderParams};
// Removed bash_tool_info, BashExecutor, BashParams imports
use crate::brave_search::{search_tool_info, BraveSearchClient};
// Removed unused gmail_integration imports
use crate::long_running_task::{handle_long_running_tool_call, long_running_tool_info, LongRunningTaskManager};
use crate::mermaid_chart::{handle_mermaid_chart_tool_call, mermaid_chart_tool_info, MermaidChartParams};
// Removed unused PlannerToolImpl import
use crate::bash::BashTool; // Import the new BashTool location
use crate::process_html::extract_text_from_html;
// Removed unused regex_replace imports
use crate::scraping_bee::{scraping_tool_info, ScrapingBeeClient, ScrapingBeeResponse};
// Removed unused ensure_id, standard_error_response
use crate::tool_trait::{ExecuteFuture, Tool, standard_success_response, standard_tool_result}; // Keep Tool trait for now
// Import DynService from rmcp::service and RoleServer for the correct trait object type
use rmcp::{service::DynService, RoleServer, ServiceExt}; // Removed unused ServerHandler import

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
// Removed unused CallToolResult, ToolResponseContent, INTERNAL_ERROR, INVALID_PARAMS
use shared_protocol_objects::{CallToolParams, JsonRpcResponse}; // Keep CallToolParams for now
use std::env;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
// Removed unused debug, error
use tracing::{info, warn};

// ScrapingBee Tool Implementation
#[derive(Debug)]
pub struct ScrapingBeeTool {
    api_key: String,
}

// Google Search Tool Implementation
#[derive(Debug)]
pub struct GoogleSearchTool {
    api_key: String,
    cx: String,
}

impl ScrapingBeeTool {
    pub fn new() -> Result<Self> {
        let api_key = env::var("SCRAPINGBEE_API_KEY")
            .map_err(|_| anyhow!("SCRAPINGBEE_API_KEY environment variable must be set"))?;
        
        Ok(Self { api_key })
    }
}

impl Tool for ScrapingBeeTool {
    fn name(&self) -> &str {
        "scrape_url"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        scraping_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        let api_key = self.api_key.clone();
        
        Box::pin(async move {
            let url = params
                .arguments
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Missing required argument: url"))?
                .to_string();
                
            // Get optional render_js parameter (default: true for compatibility)
            let render_js = params
                .arguments
                .get("render_js")
                .and_then(Value::as_bool)
                .unwrap_or(true);
                
            let mut client = ScrapingBeeClient::new(api_key);
            client
                .url(&url)
                .render_js(render_js)
                .block_resources(true)
                .block_ads(true);
                
            // Set a shorter timeout for faster responses if not using JS rendering
            if !render_js {
                client.timeout(8000); // 8 seconds is enough for static content
            }
            
            info!("Scraping URL: {} (render_js: {})", url, render_js);
            
            match client.execute().await {
                Ok(ScrapingBeeResponse::Text(body)) => {
                    let mut markdown = extract_text_from_html(&body, Some(&url));
                    const MAX_CHARS: usize = 25000; // Increased limit
                    if markdown.chars().count() > MAX_CHARS {
                        markdown = markdown.chars().take(MAX_CHARS).collect::<String>();
                        markdown.push_str("\n\n... (content truncated)");
                        info!("Scraped content truncated to {} characters", MAX_CHARS);
                    }
                    let tool_res = standard_tool_result(markdown, None);
                    Ok(standard_success_response(id, json!(tool_res)))
                }
                Ok(ScrapingBeeResponse::Binary(_)) => {
                    Err(anyhow!("Can't read binary scrapes"))
                }
                Err(e) => {
                    let tool_res = standard_tool_result(format!("Error: {}", e), Some(true));
                    Ok(standard_success_response(id, json!(tool_res)))
                }
            }
        })
    }
}

// BraveSearch Tool Implementation
#[derive(Debug)]
pub struct BraveSearchTool {
    api_key: String,
}

impl BraveSearchTool {
    pub fn new() -> Result<Self> {
        let api_key = env::var("BRAVE_API_KEY")
            .map_err(|_| anyhow!("BRAVE_API_KEY environment variable must be set"))?;
            
        Ok(Self { api_key })
    }
}

impl GoogleSearchTool {
    pub fn new() -> Result<Self> {
        let api_key = env::var("GOOGLE_API_KEY")
            .map_err(|_| anyhow!("GOOGLE_API_KEY environment variable must be set"))?;
            
        let cx = env::var("GOOGLE_SEARCH_CX")
            .map_err(|_| anyhow!("GOOGLE_SEARCH_CX environment variable must be set"))?;
            
        Ok(Self { api_key, cx })
    }
}

impl Tool for BraveSearchTool {
    fn name(&self) -> &str {
        "brave_search"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        search_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        let api_key = self.api_key.clone();
        
        Box::pin(async move {
            let query = params
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Missing required argument: query"))?
                .to_string();
                
            let count = params
                .arguments
                .get("count")
                .and_then(Value::as_u64)
                .unwrap_or(10)
                .min(20) as u8;
                
            let client = BraveSearchClient::new(api_key);
            
            match client.search(&query).await {
                Ok(response) => {
                    let results = match response.web {
                        Some(web) => web
                            .results
                            .iter()
                            .take(count as usize)
                            .map(|result| {
                                format!(
                                    "Title: {}\nURL: {}\nDescription: {}\n\n",
                                    result.title,
                                    result.url,
                                    result
                                        .description
                                        .as_deref()
                                        .unwrap_or("No description available")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("---\n"),
                        None => "No web results found".to_string(),
                    };
                    
                    let tool_res = standard_tool_result(results, None);
                    Ok(standard_success_response(id, json!(tool_res)))
                }
                Err(e) => {
                    let tool_res = standard_tool_result(format!("Search error: {}", e), Some(true));
                    Ok(standard_success_response(id, json!(tool_res)))
                }
            }
        })
    }
}


// Aider Tool Implementation
#[derive(Debug)]
pub struct AiderTool;

impl Tool for AiderTool {
    fn name(&self) -> &str {
        "aider"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        aider_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        Box::pin(async move {
            let aider_params: AiderParams = serde_json::from_value(params.arguments)?;
            
            match handle_aider_tool_call(aider_params).await {
                Ok(result) => {
                    let model_info = match &result.model {
                        Some(model) => format!("Provider: {} | Model: {}", result.provider, model),
                        None => format!("Provider: {}", result.provider),
                    };
                    
                    let text = format!(
                        "Aider execution {} [{}]\n\nDirectory: {}\nExit status: {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        if result.success { "succeeded" } else { "failed" },
                        model_info,
                        result.directory,
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

// LongRunningTask Tool Implementation
#[derive(Debug)]
pub struct LongRunningTaskTool {
    manager: Arc<Mutex<LongRunningTaskManager>>,
}

impl LongRunningTaskTool {
    pub fn new(manager: Arc<Mutex<LongRunningTaskManager>>) -> Self {
        Self { manager }
    }
}

impl Tool for LongRunningTaskTool {
    fn name(&self) -> &str {
        "long_running_tool"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        long_running_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        let manager = Arc::clone(&self.manager);
        
        Box::pin(async move {
            let manager_clone = {
                let guard = manager.lock().await;
                guard.clone()
            };
            
            handle_long_running_tool_call(params, &manager_clone, id).await
        })
    }
}

// MermaidChart Tool Implementation
#[derive(Debug)]
pub struct MermaidChartTool;

impl Tool for MermaidChartTool {
    fn name(&self) -> &str {
        "mermaid_chart"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        mermaid_chart_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        Box::pin(async move {
            let mermaid_params: MermaidChartParams = serde_json::from_value(params.arguments)?;
            
            // Pass errors directly from the handler which now has proper error handling
            handle_mermaid_chart_tool_call(mermaid_params, id.clone()).await
        })
    }
}

// Factory function to create all available tools
// Change return type to use the dyn-safe DynService trait object
pub async fn create_tools() -> Result<Vec<Box<dyn DynService<RoleServer>>>> {
    let mut tools: Vec<Box<dyn DynService<RoleServer>>> = Vec::new(); // Use DynService vector

    // Add ScrapingBee tool if environment variable is set
    // TODO: Convert ScrapingBeeTool to SDK and add using into_dyn()
    if let Ok(_scraping_bee_tool) = ScrapingBeeTool::new() {
        // tools.push(Box::new(scraping_bee_tool).into_dyn()); // Placeholder for converted tool
        // tools.push(Box::new(scraping_bee_tool)); // Cannot add non-ServerHandler tool yet
    } else {
        warn!("ScrapingBee tool not available: missing API key");
    }
    
    // Add BraveSearch tool if environment variable is set
    // TODO: Convert BraveSearchTool to SDK and add using into_dyn()
    if let Ok(_brave_search_tool) = BraveSearchTool::new() {
        // tools.push(Box::new(brave_search_tool).into_dyn()); // Placeholder for converted tool
        // tools.push(Box::new(brave_search_tool)); // Cannot add non-ServerHandler tool yet
    } else {
        warn!("BraveSearch tool not available: missing API key");
    }
    
    // Add BashTool using into_dyn() - Requires ServiceExt trait in scope
    tools.push(Box::new(BashTool).into_dyn());

    // Add other tools that don't require special initialization
    // TODO: Convert AiderTool, MermaidChartTool, PlannerTool to SDK and use into_dyn()
    // tools.push(Box::new(AiderTool).into_dyn());
    // tools.push(Box::new(MermaidChartTool).into_dyn());
    // tools.push(Box::new(PlannerTool).into_dyn()); // Use PlannerTool struct name

    // Note: LongRunningTaskTool is added separately in main.rs since it needs the manager
    
    Ok(tools)
}

// Helper function to send progress notification
pub async fn send_progress_notification(
    tx_out: &mpsc::UnboundedSender<JsonRpcResponse>,
    params: &CallToolParams,
    progress: u32,
    total: u32
) -> Result<()> {
    if let Some(meta) = params.arguments.get("_meta") {
        if let Some(token) = meta.get("progressToken") {
            let notification = shared_protocol_objects::create_notification(
                "notifications/progress",
                json!({
                    "progressToken": token,
                    "progress": progress,
                    "total": total
                }),
            );

            let progress_notification = JsonRpcResponse {
                jsonrpc: notification.jsonrpc,
                id: Value::Null,
                result: Some(json!({
                    "method": notification.method,
                    "params": notification.params
                })),
                error: None,
            };
            
            tx_out.send(progress_notification)
                .map_err(|e| anyhow!("Failed to send progress notification: {}", e))?;
        }
    }
    
    Ok(())
}
