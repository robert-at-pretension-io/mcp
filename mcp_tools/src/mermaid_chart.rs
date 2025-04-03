use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shared_protocol_objects::{CallToolParams, CallToolResult, JsonRpcResponse};
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{debug, error, info};
use reqwest::Client;

use crate::tool_trait::{ensure_id, standard_error_response, standard_success_response, standard_tool_result};

/// Call the Gemini API to generate content
async fn call_gemini_api(prompt: &str) -> Result<String> {
    // Get the API key from environment
    let api_key = env::var("GEMINI_API_KEY")
        .map_err(|_| anyhow!("GEMINI_API_KEY environment variable must be set"))?;
    
    // Set up the model and API endpoint - using the exact model ID requested
    let model_id = "gemini-2.5-pro-exp-03-25"; // Using specified model ID
    let api = "generateContent"; // Use non-streaming endpoint
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:{}?key={}",
        model_id, api, api_key
    );

    error!("Using Gemini model: {}", model_id); // Log which model we're using

    // Prepare the request JSON - exactly matching the Gemini API docs
    let request_body = json!({
        "contents": [
            {
                "parts": [
                    {
                        "text": prompt
                    }
                ]
            }
        ],
        "generationConfig": {
            "responseMimeType": "text/plain"
        }
    });

    // Make the API call
    let client = Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to call Gemini API: {}", e))?;
    
    // Check if the request was successful
    if !response.status().is_success() {
        let error_text = response.text().await
            .unwrap_or_else(|_| "Failed to get error details".to_string());
        error!("Gemini API returned error status: {}", error_text);
        return Err(anyhow!("Gemini API error: {}", error_text));
    }
    
    // Process the standard non-streaming response
    let response_text = response.text().await
        .map_err(|e| anyhow!("Failed to read response: {}", e))?;
    
    // Log response for debugging
    error!("Gemini API response: {}", response_text);
    
    // Parse the JSON response
    let response_json: Value = serde_json::from_str(&response_text)
        .map_err(|e| anyhow!("Failed to parse JSON response: {}", e))?;
    
    // Check for error response
    if let Some(error) = response_json.get("error") {
        error!("Gemini API error: {:?}", error);
        return Err(anyhow!("Gemini API error: {:?}", error));
    }
    
    // Extract text from the response using a simple approach
    match response_json
        .get("candidates")
        .and_then(|candidates| candidates.get(0))
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.get(0))
        .and_then(|part| part.get("text"))
        .and_then(|text| text.as_str()) {
            Some(text) => Ok(text.to_string()),
            None => {
                error!("Could not extract text from response: {}", response_text);
                Err(anyhow!("No content found in Gemini response. Check API key and try again."))
            }
        }
}


#[derive(Debug, Serialize, Deserialize)]
pub struct MermaidChartParams {
    pub files: Vec<String>,
    pub chart_type: Option<String>,
    pub prompt: Option<String>,
}

/// Info for the mermaid chart generation tool
pub fn mermaid_chart_tool_info() -> shared_protocol_objects::ToolInfo {
    shared_protocol_objects::ToolInfo {
        name: "mermaid_chart".to_string(),
        description: Some("Generate a Mermaid chart from a collection of files. Provide a list of file paths, and this tool will create a string with their contents and generate a Mermaid diagram visualization.".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "description": "List of file paths to include in the chart generation",
                    "items": {
                        "type": "string"
                    }
                },
                "chart_type": {
                    "type": "string",
                    "description": "Optional. The type of chart to generate (e.g., 'flowchart', 'class', 'sequence', etc.). Defaults to 'flowchart' if not specified.",
                    "enum": ["flowchart", "class", "sequence", "er", "gantt", "pie"]
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional. Additional instructions for the chart generation"
                }
            },
            "required": ["files"]
        }),
    }
}

/// Handle the mermaid chart generation
pub async fn handle_mermaid_chart_tool_call(
    params: MermaidChartParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    // Check for API key first to fail fast
    if env::var("GEMINI_API_KEY").is_err() {
        error!("GEMINI_API_KEY environment variable is not set");
        let error_message = "GEMINI_API_KEY environment variable must be set to use the mermaid_chart tool.";
        let tool_res = standard_tool_result(error_message.to_string(), Some(true));
        return Ok(standard_success_response(id, json!(tool_res)));
    }
    
    // Validate file paths
    for file_path in &params.files {
        if !Path::new(file_path).exists() {
            return Ok(standard_error_response(
                id,
                -32602,
                &format!("File not found: {}", file_path),
            ));
        }
    }

    // Read file contents
    let mut file_contents = String::new();
    for file_path in &params.files {
        let path = Path::new(file_path);
        
        // Skip very large files, binary files, etc.
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > 1_000_000 {  // Skip files larger than 1MB
            file_contents.push_str(&format!("# File: {} (skipped - too large)\n\n", file_path));
            continue;
        }
        
        let filename = path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        
        let extension = path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
            
        match File::open(path) {
            Ok(mut file) => {
                let mut content = String::new();
                if file.read_to_string(&mut content).is_ok() {
                    // Add file header and content to the collection
                    file_contents.push_str(&format!("# File: {}\n```{}\n{}\n```\n\n", 
                        file_path, extension, content));
                } else {
                    // If we can't read as string, it might be binary
                    file_contents.push_str(&format!("# File: {} (skipped - binary or encoding issue)\n\n", file_path));
                }
            }
            Err(e) => {
                return Ok(standard_error_response(
                    id,
                    -32603,
                    &format!("Failed to open file {}: {}", file_path, e),
                ));
            }
        }
    }

    // Build the prompt for chart generation
    let chart_type = params.chart_type.unwrap_or_else(|| "flowchart".to_string());
    let additional_instructions = params.prompt.unwrap_or_else(|| "".to_string());
    
    let prompt = format!(
        "Based on the code files below, generate a clean, well-structured Mermaid {} diagram that visualizes the relationships between components.\n\n\
         {}\n\n\
         INSTRUCTIONS:\n\
         - Create a Mermaid diagram using proper {} syntax\n\
         - Focus only on the important components and their relationships\n\
         - Include {} specific classes, methods, and relationships\n\
         - Keep the diagram easy to read and understand\n\
         - {}\n\n\
         IMPORTANT: Return only the diagram code with no code block formatting, explanations, or other text.",
        chart_type, file_contents, chart_type, chart_type, additional_instructions
    );

    // Call Gemini API to generate the diagram
    match call_gemini_api(&prompt).await {
        Ok(diagram) => {
            // Success case - return the diagram
            let tool_res = standard_tool_result(diagram, None);
            Ok(standard_success_response(id, json!(tool_res)))
        },
        Err(e) => {
            // Error case - return a properly formatted error message
            error!("Gemini API error: {}", e);
            let error_message = format!("Error generating Mermaid chart: {}", e);
            let tool_res = standard_tool_result(error_message, Some(true));
            Ok(standard_success_response(id, json!(tool_res)))
        }
    }
}