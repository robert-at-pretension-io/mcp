use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};
use schemars::JsonSchema;
use std::env;

// Import SDK components
use rmcp::{tool, ServerHandler, model::ServerInfo};

#[derive(Debug)]
pub enum ScrapingBeeResponse {
    Text(String),
    Binary(Vec<u8>),
}

// Parameters for the SDK-based tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScrapingBeeParams {
    #[schemars(description = "The complete URL of the webpage to read and analyze")]
    pub url: String,
    
    #[serde(default = "default_render_js")]
    #[schemars(description = "Whether to render JavaScript (default: true; set to false for faster scraping of static sites)")]
    pub render_js: bool,
}

fn default_render_js() -> bool {
    true
}

// Define the ScrapingBee tool
#[derive(Debug, Clone)]
pub struct ScrapingBeeTool {
    // The tool won't store the client directly
    // Instead, it will create a client when needed
}

impl ScrapingBeeTool {
    pub fn new() -> Self {
        Self {}
    }
    
    // Helper method to create a properly configured client
    async fn execute_scraping(&self, url: &str, render_js: bool) -> Result<String> {
        info!("Starting ScrapingBee request for URL: {} (render_js: {})", url, render_js);
        
        // Get API key from environment
        let api_key = env::var("SCRAPINGBEE_API_KEY")
            .map_err(|_| anyhow!("SCRAPINGBEE_API_KEY environment variable must be set"))?;
        
        // Create a client with a 20-second timeout
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| {
                error!("Failed to build HTTP client with timeout, using default client");
                reqwest::Client::new()
            });
            
        // Prepare headers and query parameters
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        
        // Calculate timeout based on render_js
        let timeout = if render_js { 15000 } else { 8000 };
        
        // Build the request
        let request = client
            .get("https://app.scrapingbee.com/api/v1/")
            .headers(headers)
            .query(&[
                ("api_key", api_key.as_str()),
                ("url", url),
                ("render_js", &render_js.to_string()),
                ("premium_proxy", "true"),
                ("block_ads", "true"),
                ("block_resources", "true"),
                ("timeout", &timeout.to_string()),
            ]);
            
        debug!("Sending request to ScrapingBee API");
        
        // Execute the request
        let response = request.send().await
            .map_err(|e| {
                error!("Failed to send request to ScrapingBee: {}", e);
                
                if e.is_timeout() {
                    error!("Request to ScrapingBee timed out");
                    anyhow!("Request to ScrapingBee timed out after 20 seconds")
                } else if e.is_connect() {
                    error!("Connection error to ScrapingBee API");
                    anyhow!("Failed to connect to ScrapingBee API: {}", e)
                } else {
                    anyhow!("ScrapingBee request failed: {}", e)
                }
            })?;
            
        let status = response.status();
        debug!("Received response with status: {}", status);
        
        // Check if successful
        if !status.is_success() {
            let error_text = response.text().await?;
            error!("ScrapingBee API request failed with status: {}", status);
            error!("Error response: {}", error_text);
            return Err(anyhow!("ScrapingBee API failed: {} - {}", status, error_text));
        }
        
        // Process the response based on content type
        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
            
        if content_type.starts_with("text") || content_type.contains("json") {
            // Process text content
            let text = response.text().await?;
            debug!("Received text response ({} characters)", text.len());
            
            // Convert HTML to readable text
            let markdown = crate::process_html::extract_text_from_html(&text, Some(url));
            
            // Truncate if too long
            const MAX_CHARS: usize = 25000;
            if markdown.chars().count() > MAX_CHARS {
                let truncated = markdown.chars().take(MAX_CHARS).collect::<String>();
                Ok(format!("{}\n\n... (content truncated)", truncated))
            } else {
                Ok(markdown)
            }
        } else {
            // Can't process binary responses
            error!("Received binary response, cannot process");
            Err(anyhow!("Cannot process binary response from URL"))
        }
    }
}

// Remove the tool_box macro here, as McpToolServer handles registration
impl ScrapingBeeTool {
    // Make the method public so McpToolServer can call it
    #[tool(description = "Web scraping tool that extracts and processes content from websites. Use for extracting text from webpages, documentation, and articles.")]
    pub async fn scrape_url( // Method should already be public
        &self,
        #[tool(aggr)] params: ScrapingBeeParams // Keep tool(aggr) for potential future direct use? Or remove if only called by McpToolServer? Let's keep it for now.
    ) -> String {
        // Log the operation start
        info!("ScrapingBee tool called for URL: {}", params.url);
        
        // Execute scraping and handle errors
        match self.execute_scraping(&params.url, params.render_js).await {
            Ok(content) => content,
            Err(e) => {
                error!("Scraping error: {}", e);
                format!("Error: {}", e)
            }
        }
    }
}

// Remove ServerHandler implementation for the individual tool
// This is now handled by McpToolServer
/* REMOVED
impl ServerHandler for ScrapingBeeTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("A tool for scraping and processing web content.".into()),
            ..Default::default()
        }
    }
}
*/

// Old ScrapingBeeClient struct and related functions have been refactored
// into ScrapingBeeTool and its implementation above.
