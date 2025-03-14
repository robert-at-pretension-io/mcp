use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use serde::Serialize;
use tracing::{info, warn, error, debug};
use serde_json::json;

use ::shared_protocol_objects::ToolInfo;

#[derive(Debug)]
pub enum ScrapingBeeResponse {
    Text(String),
    Binary(Vec<u8>),
}

#[derive(Debug, Serialize)]
struct ScrapingBeeRequest {
    url: String,
    render_js: bool,
}

#[derive(Clone)]
pub struct ScrapingBeeClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    url: Option<String>,
    render_js: bool,
}

pub fn scraping_tool_info() -> ToolInfo {
    ToolInfo {
        name: "scrape_url".into(),
        description: Some(
            "Web scraping tool that extracts and processes content from websites. Use this to:
            
            1. Extract text from webpages (news, articles, documentation)
            2. Gather product information from e-commerce sites
            3. Retrieve data from sites with JavaScript-rendered content
            4. Access content behind cookie notifications or simple overlays
            
            Important notes:
            - Always provide complete URLs including protocol (e.g., 'https://example.com')
            - JavaScript rendering is enabled by default
            - Content is automatically processed to extract readable text
            - Safe mode filters out potentially harmful content
            - May take up to 30 seconds for complex pages
            
            Example queries:
            - News article: 'https://news.site.com/article/12345'
            - Product page: 'https://shop.example.com/products/item-name'
            - Documentation: 'https://docs.domain.org/tutorial'".into()
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { 
                    "type": "string",
                    "description": "The complete URL of the webpage to read and analyze",
                    "format": "uri"
                }
            },
            "required": ["url"],
            "additionalProperties": false
        }),
    }
}


impl ScrapingBeeClient {
    pub fn new(api_key: String) -> Self {
        // Create a client with a 30-second timeout
        // According to MCP spec, we should give tools reasonable time but prevent indefinite hangs
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| {
                error!("Failed to build HTTP client with timeout, using default client");
                reqwest::Client::new()
            });
            
        ScrapingBeeClient {
            client,
            api_key,
            base_url: "https://app.scrapingbee.com/api/v1/".to_string(),
            url: None,
            render_js: true,
        }
    }

    pub fn url(&mut self, url: &str) -> &mut Self {
        self.url = Some(url.to_string());
        self
    }

    pub fn render_js(&mut self, enabled: bool) -> &mut Self {
        self.render_js = enabled;
        self
    }

    pub async fn execute(&self) -> Result<ScrapingBeeResponse> {
        info!("Starting ScrapingBee request execution");
        let url = self.url.as_ref().ok_or_else(|| {
            error!("URL not set for ScrapingBee request");
            anyhow!("URL not set")
        })?;

        info!("Preparing ScrapingBee request for URL: {}", url);
        debug!("Request parameters: render_js={}", self.render_js);

        let request_body = ScrapingBeeRequest {
            url: url.to_string(),
            render_js: self.render_js,
        };

        info!("Setting up request headers");
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        debug!("Using API key: {}", self.api_key.chars().take(5).collect::<String>() + "...");

        info!("Building ScrapingBee API request");
        let request = self.client
            .get(&self.base_url)
            .headers(headers)
            .query(&[
                ("api_key", &self.api_key),
                ("url", &request_body.url),
                ("render_js", &request_body.render_js.to_string()),
                ("premium_proxy", &"true".to_string()),
                ("block_ads", &"true".to_string()),      // Block ads to reduce page load time
                ("block_resources", &"true".to_string()), // Block unnecessary resources like images
                ("timeout", &"15000".to_string())        // 15 seconds timeout on ScrapingBee side
            ]);

        // Clone and build request for logging
        debug!("Full request URL: {}", request.try_clone().unwrap().build()?.url());

        info!("Sending request to ScrapingBee API");
        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Failed to send request to ScrapingBee: {}", e);
                
                if e.is_timeout() {
                    error!("Request to ScrapingBee timed out");
                    return Err(anyhow!("Request to ScrapingBee timed out after 20 seconds"));
                } else if e.is_connect() {
                    error!("Connection error to ScrapingBee API");
                    return Err(anyhow!("Failed to connect to ScrapingBee API: {}", e));
                }
                
                return Err(anyhow!("ScrapingBee request failed: {}", e));
            }
        };

        let status = response.status();
        info!("Received response with status: {}", status);

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("ScrapingBee API request failed");
            error!("Status code: {}", status);
            error!("Error response: {}", error_text);
            error!("Target URL: {}", url);
            error!("API endpoint: {}", self.base_url);
            warn!("Request parameters:");
            warn!("  - render_js: {}", self.render_js);
            warn!("  - api_key length: {}", self.api_key.len());
            return Err(anyhow!(
                "ScrapingBee API request failed with status: {} - Response: {}", 
                status,
                error_text
            ));
        }

        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        
        info!("Response content type: {}", content_type);

        if content_type.starts_with("text") || content_type.contains("json") {
            info!("Processing text/JSON response");
            let text = response.text().await?;
            debug!("Response length: {} characters", text.len());
            info!("Successfully retrieved text content from ScrapingBee");
            Ok(ScrapingBeeResponse::Text(text))
        } else {
            info!("Processing binary response");
            let bytes = response.bytes().await?.to_vec();
            debug!("Response size: {} bytes", bytes.len());
            info!("Successfully retrieved binary content from ScrapingBee");
            Ok(ScrapingBeeResponse::Binary(bytes))
        }
    }
}
