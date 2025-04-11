use serde::{Deserialize, Serialize};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use tracing::{info, error, debug};
use std::env;

// Import SDK components
use rmcp::tool;

// Response Models
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    #[serde(rename = "type")]
    pub type_: String,
    pub web: Option<Search>,
    pub query: Option<Query>,
}

#[derive(Debug, Deserialize)]
pub struct Search {
    #[serde(rename = "type")]
    pub type_: String,
    pub results: Vec<SearchResult>,
    pub family_friendly: bool,
}

#[derive(Debug, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub description: Option<String>,
    pub page_age: Option<String>,
    pub page_fetched: Option<String>,
    pub language: Option<String>,
    pub family_friendly: bool,
    pub is_source_local: bool,
    pub is_source_both: bool,
    pub extra_snippets: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct Query {
    pub original: String,
    pub show_strict_warning: Option<bool>,
    pub altered: Option<String>,
    pub safesearch: Option<bool>,
    pub is_navigational: Option<bool>,
    pub is_geolocal: Option<bool>,
    pub local_decision: Option<String>,
    pub local_locations_idx: Option<i32>,
    pub is_trending: Option<bool>,
    pub is_news_breaking: Option<bool>,
    pub ask_for_location: Option<bool>,
    pub spellcheck_off: Option<bool>,
    pub country: Option<String>,
    pub bad_results: Option<bool>,
    pub should_fallback: Option<bool>,
    pub lat: Option<String>,
    pub long: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub header_country: Option<String>,
    pub more_results_available: Option<bool>,
    pub custom_location_label: Option<String>,
    pub reddit_cluster: Option<String>,
}

// Tool Parameters
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BraveSearchParams {
    #[schemars(description = "The search query - be specific and include relevant keywords")]
    pub query: String,
    
    #[serde(default = "default_count")]
    #[schemars(description = "Number of results to return (max 20). Use more results for broad research, fewer for specific queries.")]
    pub count: u8,
}

fn default_count() -> u8 {
    10
}

// Request Parameters
#[derive(Debug, Serialize)]
struct SearchParams {
    pub q: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safesearch: Option<String>,
}

// Define the BraveSearch tool
#[derive(Debug, Clone)]
pub struct BraveSearchTool {
    // We'll get the API key from env in execute_search
}

impl BraveSearchTool {
    pub fn new() -> Self {
        Self {}
    }
    
    // Helper method to create a properly configured client and execute search
    async fn execute_search(&self, query: &str, count: u8) -> Result<String> {
        info!("Starting Brave Search request for query: {}", query);
        
        // Get API key from environment
        let api_key = env::var("BRAVE_API_KEY")
            .map_err(|_| anyhow!("BRAVE_API_KEY environment variable must be set"))?;
        
        // Create client
        let client = reqwest::Client::new();
        let base_url = "https://api.search.brave.com/res/v1/web/search";
        
        let params = SearchParams {
            q: query.to_string(),
            count: Some(count.min(20)),  // maximum 20 results
            offset: None,
            safesearch: Some("moderate".to_string()),
        };

        // Set up headers
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            "X-Subscription-Token",
            HeaderValue::from_str(&api_key)?,
        );

        debug!("Sending request to Brave Search API");
        
        // Make the request
        let response = client
            .get(base_url)
            .headers(headers)
            .query(&params)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to send request to Brave Search: {}", e);
                anyhow!("Brave Search request failed: {}", e)
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("API request failed with status: {}", error_text);
            return Err(anyhow!("API request failed: {}", error_text));
        }

        // Parse the JSON response
        let search_response = response.json::<SearchResponse>().await
            .map_err(|e| {
                error!("Failed to parse Brave Search response: {}", e);
                anyhow!("Failed to parse response: {}", e)
            })?;

        debug!("Successfully parsed Brave Search response");
        
        // Format the results
        let results = match search_response.web {
            Some(web) => {
                if web.results.is_empty() {
                    "No search results found.".to_string()
                } else {
                    web.results
                        .iter()
                        .take(count as usize)
                        .map(|result| {
                            format!(
                                "Title: {}\nURL: {}\nDescription: {}\n{}",
                                result.title,
                                result.url,
                                result
                                    .description
                                    .as_deref()
                                    .unwrap_or("No description available"),
                                if result.page_age.is_some() {
                                    format!("Age: {}\n", result.page_age.as_deref().unwrap())
                                } else {
                                    "".to_string()
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n---\n\n")
                }
            },
            None => "No web results found".to_string(),
        };
        
        Ok(results)
    }
}

#[tool(tool_box)]
impl BraveSearchTool {
    #[tool(description = "Web search tool powered by Brave Search that retrieves relevant results from across the internet. Use this to find current information and facts from the web, research topics with multiple sources, verify claims, discover recent news and trends, or find specific websites and resources.")]
    pub async fn brave_search(
        &self,
        #[tool(aggr)] params: BraveSearchParams
    ) -> String {
        // Log the operation start
        info!("Brave Search tool called for query: {}", params.query);
        
        // Execute search and handle errors
        match self.execute_search(&params.query, params.count).await {
            Ok(content) => content,
            Err(e) => {
                error!("Search error: {}", e);
                format!("Error: {}", e)
            }
        }
    }
}
