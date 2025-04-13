use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::{error, info};
use reqwest::Client;
use schemars::JsonSchema;

// Import rmcp SDK components
use rmcp::tool;

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


#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MermaidChartParams {
    #[schemars(description = "A space-separated string of file paths to include in the chart generation")]
    pub files: String,

    #[serde(default)]
    #[schemars(description = "Optional: The type of chart to generate (e.g., 'flowchart', 'class', 'sequence'). Leave empty to default to 'flowchart'.")]
    pub chart_type: String, // Changed from Option<String>

    #[serde(default)]
    #[schemars(description = "Optional: Additional instructions for the chart generation. Leave empty for none.")]
    pub prompt: String, // Changed from Option<String>
}

#[derive(Debug, Clone)]
pub struct MermaidChartTool;

impl MermaidChartTool {
    pub fn new() -> Self {
        Self
    }
    
    // Helper method to generate the mermaid chart
    async fn generate_chart(&self, params: MermaidChartParams) -> Result<String> {
        // Check for API key first to fail fast
        if env::var("GEMINI_API_KEY").is_err() {
            error!("GEMINI_API_KEY environment variable is not set");
            return Err(anyhow!("GEMINI_API_KEY environment variable must be set to use the mermaid_chart tool."));
        }

        // Split the files string into individual paths
        let file_paths: Vec<&str> = params.files.split_whitespace().collect();
        if file_paths.is_empty() {
            return Err(anyhow!("No file paths provided in the 'files' string."));
        }

        // Validate file paths
        for file_path in &file_paths {
            if !Path::new(file_path).exists() {
                return Err(anyhow!("File not found: {}", file_path));
            }
        }

        // Read file contents
        let mut file_contents = String::new();
        for file_path in &file_paths {
            let path = Path::new(*file_path); // Dereference file_path since it's &str

            // Skip very large files, binary files, etc.
            let metadata = std::fs::metadata(path)?;
            if metadata.len() > 1_000_000 {  // Skip files larger than 1MB
                file_contents.push_str(&format!("# File: {} (skipped - too large)\n\n", file_path));
                continue;
            }
            
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
                    return Err(anyhow!("Failed to open file {}: {}", file_path, e));
                }
            }
        }

        // Build the prompt for chart generation
        let chart_type = if params.chart_type.trim().is_empty() {
            "flowchart".to_string() // Default if empty
        } else {
            params.chart_type.trim().to_string()
        };
        let additional_instructions_raw = params.prompt.trim(); // Use directly, empty if no instructions

        // Create the formatted instruction string beforehand if needed
        let formatted_instructions = if additional_instructions_raw.is_empty() {
            "".to_string() // Use an empty owned String if no instructions
        } else {
            format!("- {}", additional_instructions_raw) // Create the formatted owned String
        };

        let prompt = format!(
            "Based on the code files below, generate a clean, well-structured Mermaid {} diagram that visualizes the relationships between components.\n\n\
             {}\n\n\
             INSTRUCTIONS:\n\
             - Create a Mermaid diagram using proper {} syntax\n\
             - Focus only on the important components and their relationships\n\
             - Include {} specific classes, methods, and relationships\n\
             - Keep the diagram easy to read and understand\n\
             {}\n\n\
             IMPORTANT: Return only the diagram code with no code block formatting, explanations, or other text.",
            chart_type, file_contents, chart_type, chart_type,
            formatted_instructions // Use the pre-formatted string here
        );

        // Call Gemini API to generate the diagram
        call_gemini_api(&prompt).await
    }
}

#[tool(tool_box)]
impl MermaidChartTool {
    #[tool(description = "Generate a Mermaid chart from a collection of files. Provide a space-separated string of file paths, and this tool will create a string with their contents and generate a Mermaid diagram visualization.")]
    pub async fn mermaid_chart(
        &self,
        #[tool(aggr)] params: MermaidChartParams
    ) -> String {
        // Log the number of files based on splitting the input string
        let file_count = params.files.split_whitespace().count();
        // Log the chart type string directly
        info!("Generating Mermaid chart for {} files (from input string '{}') with chart type: '{}'",
              file_count, params.files, if params.chart_type.is_empty() { "flowchart (default)" } else { &params.chart_type });

        match self.generate_chart(params).await {
            Ok(diagram) => {
                // Format the diagram with a Mermaid code block
                format!("```mermaid\n{}\n```", diagram)
            },
            Err(e) => {
                error!("Error generating Mermaid chart: {}", e);
                format!("Error generating Mermaid chart: {}", e)
            }
        }
    }
}
