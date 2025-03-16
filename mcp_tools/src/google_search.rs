use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleSearchResult {
    pub title: String,
    pub link: String,
    pub snippet: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleSearchResponse {
    pub items: Option<Vec<GoogleSearchResult>>,
    pub error: Option<GoogleSearchError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleSearchError {
    pub code: Option<i32>,
    pub message: Option<String>,
    pub status: Option<String>,
}

pub struct GoogleSearchClient {
    client: Client,
    api_key: String,
    cx: String,
    base_url: String,
}

impl GoogleSearchClient {
    pub fn new(api_key: String, cx: String) -> Self {
        // Create a client with a 30-second timeout
        // According to MCP spec, we should give tools reasonable time but prevent indefinite hangs
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| {
                error!("Failed to build HTTP client with timeout, using default client");
                Client::new()
            });

        Self {
            client,
            api_key,
            cx,
            base_url: "https://www.googleapis.com/customsearch/v1".to_string(),
        }
    }

    pub async fn search(&self, query: &str, num_results: Option<u32>) -> Result<Vec<GoogleSearchResult>> {
        let num = num_results.unwrap_or(10).min(10); // Google API limits to 10 results per request
        
        debug!("Performing Google search for query: {}", query);
        
        let response = self.client
            .get(&self.base_url)
            .query(&[
                ("key", &self.api_key),
                ("cx", &self.cx),
                ("q", &query.to_string()),
                ("num", &num.to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            error!("Google search API error: {} - {}", status, text);
            return Err(anyhow!("Google search API error: {} - {}", status, text));
        }

        let search_response: GoogleSearchResponse = response.json().await?;
        
        if let Some(error) = search_response.error {
            let message = error.message.unwrap_or_else(|| "Unknown error".to_string());
            error!("Google search API error: {}", message);
            return Err(anyhow!("Google search API error: {}", message));
        }
        
        let results = search_response.items.unwrap_or_default();
        debug!("Google search returned {} results", results.len());
        
        Ok(results)
    }
}

pub fn google_search_tool_info() -> shared_protocol_objects::ToolInfo {
    shared_protocol_objects::ToolInfo {
        name: "google_search".to_string(),
        description: Some("Search the web using Google Custom Search API".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Number of results to return (max 10)",
                    "default": 5
                }
            },
            "required": ["query"]
        }),
    }
}
