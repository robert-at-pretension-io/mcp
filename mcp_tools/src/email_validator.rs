use serde_json::json;
use shared_protocol_objects::{ToolInfo};

pub fn neverbounce_tool_info() -> ToolInfo {
    ToolInfo {
        name: "never_bounce_tool".to_string(),
        description: Some(
            "Validates email addresses using the NeverBounce API.".to_string()
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "email": {"type": "string"}
            },
            "required": ["email"]
        }),
        annotations: None, // Added missing field
    }
}

// Removed unused HashMap import
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value};
use shared_protocol_objects::{
    CallToolParams, JsonRpcResponse,
    error_response, success_response,
    CallToolResult, ToolResponseContent
};

#[derive(Debug, Serialize, Deserialize)]
struct NeverBounceApiResponse {
    status: Option<String>,
    result: Option<String>,
    // Depending on the full NeverBounce JSON structure, you may want more fields here
}

pub async fn handle_neverbounce_tool_call(
    params: CallToolParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    // 1) Extract email from the call arguments
    let email = match params.arguments.get("email").and_then(Value::as_str) {
        Some(e) => e.trim().to_string(),
        None => {
            return Ok(error_response(
                id,
                -32602,
                "Missing required argument: 'email'",
            ));
        }
    };

    if email.is_empty() {
        return Ok(error_response(
            id,
            -32602,
            "Email cannot be empty",
        ));
    }

    // 2) Construct the NeverBounce URL
    let neverbounce_api_key = std::env::var("NEVERBOUNCE_API_KEY")?;
    let url = format!(
        "https://api.neverbounce.com/v4/single/check?key={}&email={}",
        neverbounce_api_key, email
    );

    // 3) Call the NeverBounce API
    let client = Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

    let status_code = resp.status();
    if !status_code.is_success() {
        let body = resp.text().await.unwrap_or_else(|_| "".to_string());
        return Ok(error_response(
            id,
            -32603,
            &format!("NeverBounce returned HTTP {}. Body: {}", status_code, body),
        ));
    }

    let api_response: NeverBounceApiResponse = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse response JSON: {}", e))?;

    // 4) Interpret the result and format a response
    // Typical `result` values can be: "valid", "invalid", "disposable", "catchall", or "unknown"
    let tool_result_str = match api_response.result {
        Some(r) => format!("NeverBounce result for '{}': {}", email, r),
        None => format!("No result returned for '{}'", email),
    };

    // Return success with the tool’s text content
    let tool_res = CallToolResult {
        content: vec![ToolResponseContent {
            type_: "text".into(),
            text: tool_result_str,
            annotations: None,
        }],
        is_error: None,
        // Removed _meta, progress, total fields
    };
    Ok(success_response(id, serde_json::to_value(tool_res)?))
}

