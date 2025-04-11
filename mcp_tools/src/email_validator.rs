use reqwest::Client;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use rmcp::tool;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NeverBounceParams {
    #[schemars(description = "Email address to validate")]
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NeverBounceApiResponse {
    status: Option<String>,
    result: Option<String>,
    // Depending on the full NeverBounce JSON structure, you may want more fields here
}

#[derive(Debug, Clone)]
pub struct EmailValidatorTool;

impl EmailValidatorTool {
    pub fn new() -> Self {
        Self
    }
}

#[tool(tool_box)]
impl EmailValidatorTool {
    #[tool(description = "Validates email addresses using the NeverBounce API.")]
    pub async fn never_bounce(&self, #[tool(aggr)] params: NeverBounceParams) -> String {
        // Validate the email parameter
        let email = params.email.trim();
        if email.is_empty() {
            return "Email cannot be empty".to_string();
        }

        // Get the API key from environment
        let neverbounce_api_key = match std::env::var("NEVERBOUNCE_API_KEY") {
            Ok(key) => key,
            Err(_) => return "NEVERBOUNCE_API_KEY environment variable is not set".to_string(),
        };

        // Construct the NeverBounce URL
        let url = format!(
            "https://api.neverbounce.com/v4/single/check?key={}&email={}",
            neverbounce_api_key, email
        );

        // Call the NeverBounce API
        let client = Client::new();
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return format!("HTTP request failed: {}", e),
        };

        let status_code = resp.status();
        if !status_code.is_success() {
            let body = resp.text().await.unwrap_or_else(|_| "".to_string());
            return format!("NeverBounce returned HTTP {}. Body: {}", status_code, body);
        }

        let api_response: NeverBounceApiResponse = match resp.json().await {
            Ok(r) => r,
            Err(e) => return format!("Failed to parse response JSON: {}", e),
        };

        // Interpret the result and format a response
        // Typical `result` values can be: "valid", "invalid", "disposable", "catchall", or "unknown"
        match api_response.result {
            Some(r) => format!("NeverBounce result for '{}': {}", email, r),
            None => format!("No result returned for '{}'", email),
        }
    }
}