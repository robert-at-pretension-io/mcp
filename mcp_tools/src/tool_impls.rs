use crate::aider::{aider_tool_info, handle_aider_tool_call, AiderParams};
use crate::brave_search::{search_tool_info, BraveSearchClient};
use crate::long_running_task::{handle_long_running_tool_call, long_running_tool_info, LongRunningTaskManager};
use crate::mermaid_chart::{handle_mermaid_chart_tool_call, mermaid_chart_tool_info, MermaidChartParams};
// Removed: use crate::bash::BashTool; // Handled by McpToolServer
// Removed: use crate::scraping_bee::ScrapingBeeTool; // Handled by McpToolServer
use crate::tool_trait::{ExecuteFuture, Tool, standard_success_response, standard_tool_result};
// Import DynService from rmcp::service and RoleServer for the correct trait object type
use rmcp::{service::DynService, RoleServer}; // Removed unused ServiceExt

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
// Removed unused CallToolResult, ToolResponseContent, INTERNAL_ERROR, INVALID_PARAMS
use shared_protocol_objects::{CallToolParams, JsonRpcResponse}; // Keep CallToolParams for now
use std::env;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
// Removed unused debug, error
use tracing::warn; // Removed unused info

// Old ScrapingBee Tool Implementation - Removed in favor of SDK-based implementation
// This was replaced by the SDK-based ScrapingBeeTool in scraping_bee.rs

// Google Search Tool Implementation
#[derive(Debug)]
pub struct GoogleSearchTool {
    api_key: String,
    cx: String,
}

// Old ScrapingBeeTool implementation removed - replaced by SDK version

// ScrapingBeeTool is being converted to use the rmcp SDK
// Old implementation commented out for reference
/*
impl Tool for ScrapingBeeTool {
    fn name(&self) -> &str {
        "scrape_url"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        scraping_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        // Implementation placeholder for future conversion
        Box::pin(async move {
            let tool_res = standard_tool_result("ScrapingBee tool is being migrated to the rmcp SDK.".to_string(), None);
            Ok(standard_success_response(id, json!(tool_res)))
        })
    }
}
*/

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
    let tools: Vec<Box<dyn DynService<RoleServer>>> = Vec::new(); // Use DynService vector, remove mut

    // ScrapingBeeTool is now implemented using SDK and added directly in main.rs
    
    // Add BraveSearch tool if environment variable is set
    // TODO: Convert BraveSearchTool to SDK and add using into_dyn()
    if let Ok(_brave_search_tool) = BraveSearchTool::new() {
        // tools.push(Box::new(brave_search_tool).into_dyn()); // Placeholder for converted tool
        // tools.push(Box::new(brave_search_tool)); // Cannot add non-ServerHandler tool yet
    } else {
        warn!("BraveSearch tool not available: missing API key");
    }
    
    // Removed: BashTool is now handled by McpToolServer in main.rs
    // tools.push(Box::new(BashTool).into_dyn());

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
