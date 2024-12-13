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
            "Web scraping tool.
            
            Use this to extract content from web pages."
           ".into()
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
        ScrapingBeeClient {
            client: reqwest::Client::new(),
            api_key,
            base_url: "https://app.scrapingbee.com/api/v1/".to_string(),
            url: None,
            render_js: false,
        }
    }

    pub fn url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self
    }

    pub fn render_js(mut self, enabled: bool) -> Self {
        self.render_js = enabled;
        self
    }

    pub async fn execute(self) -> Result<ScrapingBeeResponse> {
        info!("Starting ScrapingBee request execution");
        let url = self.url.ok_or_else(|| {
            error!("URL not set for ScrapingBee request");
            anyhow!("URL not set")
        })?;

        info!("Preparing ScrapingBee request for URL: {}", url);
        debug!("Request parameters: render_js={}", self.render_js);

        let request_body = ScrapingBeeRequest {
            url: url.clone(),
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
                ("premium_proxy", &"true".to_string()),  // Use premium proxy for better success rate
                ("stealth_proxy", &"true".to_string()),  // Enable stealth mode to avoid detection
                ("country_code", &"us".to_string()),     // Route through US proxies
                ("block_resources", &"false".to_string()) // Load all page resources
            ]);

        // Clone and build request for logging
        debug!("Full request URL: {}", request.try_clone().unwrap().build()?.url());

        info!("Sending request to ScrapingBee API");
        let response = request.send().await?;

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
