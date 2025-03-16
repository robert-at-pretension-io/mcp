use crate::aider::{aider_tool_info, handle_aider_tool_call, AiderParams};
use crate::bash::{
    bash_tool_info, handle_quick_bash, quick_bash_tool_info, BashExecutor, BashParams,
    QuickBashParams,
};
use crate::brave_search::{search_tool_info, BraveSearchClient};
use crate::email_validator::{handle_neverbounce_tool_call, neverbounce_tool_info};
use crate::git_integration::{git_tool_info, handle_git_tool_call};
use crate::gmail_integration::{gmail_tool_info, handle_gmail_tool_call};
use crate::google_search::{google_search_tool_info, GoogleSearchClient, GoogleSearchResult};
use crate::long_running_task::{handle_long_running_tool_call, long_running_tool_info, LongRunningTaskManager};
use crate::oracle_tool::{handle_oracle_select_tool_call, oracle_select_tool_info};
use crate::process_html::extract_text_from_html;
use crate::regex_replace::{handle_regex_replace_tool_call, regex_replace_tool_info};
use crate::scraping_bee::{scraping_tool_info, ScrapingBeeClient, ScrapingBeeResponse};
use crate::tool_trait::{ExecuteFuture, Tool, ensure_id, standard_error_response, standard_success_response, standard_tool_result};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use shared_protocol_objects::{
    CallToolParams, CallToolResult, JsonRpcResponse, ToolResponseContent,
    INTERNAL_ERROR, INVALID_PARAMS,
};
use std::env;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

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
                
            let mut client = ScrapingBeeClient::new(api_key);
            client.url(&url).render_js(true);
            
            match client.execute().await {
                Ok(ScrapingBeeResponse::Text(body)) => {
                    let markdown = extract_text_from_html(&body, Some(&url));
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

// Bash Tool Implementation
#[derive(Debug)]
pub struct BashTool;

impl Tool for BashTool {
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

impl Tool for GoogleSearchTool {
    fn name(&self) -> &str {
        "google_search"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        google_search_tool_info()
    }
    
    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        let api_key = self.api_key.clone();
        let cx = self.cx.clone();
        
        Box::pin(async move {
            let query = params
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Missing required argument: query"))?
                .to_string();
                
            let num_results = params
                .arguments
                .get("num_results")
                .and_then(Value::as_u64)
                .map(|n| n as u32);
                
            let client = GoogleSearchClient::new(api_key, cx);
            
            match client.search(&query, num_results).await {
                Ok(results) => {
                    let formatted_results = if results.is_empty() {
                        "No results found.".to_string()
                    } else {
                        let mut output = String::new();
                        output.push_str("# Search Results\n\n");
                        
                        for (i, result) in results.iter().enumerate() {
                            output.push_str(&format!("## {}. {}\n", i + 1, result.title));
                            output.push_str(&format!("URL: {}\n\n", result.link));
                            output.push_str(&format!("{}\n\n", result.snippet));
                        }
                        
                        output
                    };
                    
                    let tool_res = standard_tool_result(formatted_results, None);
                    Ok(standard_success_response(id, json!(tool_res)))
                }
                Err(e) => {
                    let tool_res = standard_tool_result(format!("Google search error: {}", e), Some(true));
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

// Factory function to create all available tools
pub async fn create_tools() -> Result<Vec<Box<dyn Tool>>> {
    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    
    // Add ScrapingBee tool if environment variable is set
    if let Ok(scraping_bee_tool) = ScrapingBeeTool::new() {
        tools.push(Box::new(scraping_bee_tool));
    } else {
        warn!("ScrapingBee tool not available: missing API key");
    }
    
    // Add BraveSearch tool if environment variable is set
    if let Ok(brave_search_tool) = BraveSearchTool::new() {
        tools.push(Box::new(brave_search_tool));
    } else {
        warn!("BraveSearch tool not available: missing API key");
    }
    
    // // Add GoogleSearch tool if environment variables are set
    // if let Ok(google_search_tool) = GoogleSearchTool::new() {
    //     tools.push(Box::new(google_search_tool));
    // } else {
    //     warn!("GoogleSearch tool not available: missing API key or search engine ID");
    // }
    
    // Add other tools that don't require special initialization
    tools.push(Box::new(BashTool));
    tools.push(Box::new(AiderTool));
    
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
