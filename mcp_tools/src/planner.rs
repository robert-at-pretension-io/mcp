// Removed unused anyhow import
use anyhow::{Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use tracing::{debug, error, info, warn}; // Import warn macro

use rllm::builder::{LLMBackend, LLMBuilder};
// Removed unused ChatRole import
use rllm::chat::{ChatMessageBuilder, ChatResponse, ChatRole}; // Import ChatRole for builder
use rllm::error::LLMError; // Import rllm's error type

use shared_protocol_objects::{
    CallToolParams, JsonRpcResponse, ToolInfo, INVALID_PARAMS, // Import error codes from here, removed unused INTERNAL_ERROR
};
use crate::tool_trait::{
    ExecuteFuture, Tool, standard_error_response, standard_success_response, standard_tool_result,
    // Removed INTERNAL_ERROR, INVALID_PARAMS from here
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PlannerParams {
    pub user_request: String,
    pub ai_interpretation: String,
    pub available_tools: String, // Changed from Vec<ToolInfo> to String
}

/// Generates the tool information for the planning tool.
pub fn planner_tool_info() -> ToolInfo {
    ToolInfo {
        name: "planning_tool".to_string(),
        description: Some(
            "Generates a multi-step plan using available tools to fulfill a user request.
            Provide the original user request, the AI's interpretation of that request,
            and a list of all available tools (including their descriptions and parameters).
            The tool will call a powerful LLM (Gemini) to devise a plan,
            including potential contingencies and points for reflection or waiting for results."
                .to_string(),
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "user_request": {
                    "type": "string",
                    "description": "The original request from the user."
                },
                "ai_interpretation": {
                    "type": "string",
                    "description": "The AI's interpretation or summary of the user's request and goal."
                },
                "available_tools": {
                    "type": "string",
                    "description": "A formatted string listing all tools available to the AI, including only their name and description (excluding input schema)."
                }
            },
            "required": ["user_request", "ai_interpretation", "available_tools"]
        }),
        annotations: None, // Added missing field
    }
}

// Removed format_tool_list function as available_tools is now provided as a string.

/// Calls the Gemini API via RLLM to generate a plan.
// Return type changed to Result<Box<dyn ChatResponse>, rllm::error::LLMError>
async fn generate_plan_with_gemini(prompt: &str) -> Result<Box<dyn ChatResponse>, LLMError> {
    let api_key = env::var("GEMINI_API_KEY")
        // Use rllm::error::LLMError variant - Assuming AuthError is appropriate for missing key
        .map_err(|e| LLMError::AuthError(format!("GEMINI_API_KEY not set: {}", e)))?;

    info!("Building Gemini LLM client using RLLM");
    // Define the system prompt text
    let system_prompt = "You are an expert planning assistant. Your goal is to create a robust, step-by-step plan \
                         to achieve a user's objective using a predefined set of tools. The plan should be clear, \
                         actionable, and account for potential issues. Specify when the AI should pause to reflect, \
                         wait for tool results, or handle errors. Output only the plan itself, without preamble or explanation.";

    let llm = LLMBuilder::new()
        .backend(LLMBackend::Google)
        .api_key(api_key)
        .model("gemini-1.5-pro-latest") // Using a capable model for planning
        .temperature(0.5) // Lower temperature for more deterministic planning
        .system(system_prompt) // Set system prompt via builder
        .build()?;

    // Construct messages for the chat API - only the user message now
    let messages = vec![
        // System message is handled by the builder's .system() method
        ChatMessageBuilder::new(ChatRole::User).content(prompt).build(), // Use the builder pattern
    ];

    info!("Sending planning request to Gemini via RLLM");
    debug!("Gemini prompt content: {}", prompt);

    // Use the chat method from RLLM
    llm.chat(&messages).await
}

/// Handles the execution of the planning tool call.
async fn handle_planning_tool_call(
    params: PlannerParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    info!("Handling planning_tool call");

    // Construct the detailed prompt for Gemini using the provided string
    let prompt = format!(
        "Generate a plan based on the following information:\n\n\
         User Request:\n\"{}\"\n\n\
         AI Interpretation of Goal:\n\"{}\"\n\n\
         Available Tools:\n{}\n\
         ------------------------------------\n\
         PLAN:",
        params.user_request, params.ai_interpretation, params.available_tools // Use the string directly
    );

    match generate_plan_with_gemini(&prompt).await {
        // Handle the Box<dyn ChatResponse>
        Ok(response_box) => {
            let plan_option = response_box.text(); // Use text() method instead of content(), returns Option<String>
            info!("Successfully generated plan from Gemini");
            // Use debug formatting for Option<String>
            debug!("Generated Plan:\n{:?}", plan_option);
            // Handle the Option before passing to standard_tool_result
            let plan_text = plan_option.unwrap_or_else(|| {
                warn!("Gemini response text was None, returning empty plan.");
                String::new()
            });
            let tool_res = standard_tool_result(plan_text, None);
            Ok(standard_success_response(id, json!(tool_res)))
        }
        // Use rllm::error::LLMError variants based on provided documentation
        Err(e) => {
            error!("Error generating plan using Gemini via RLLM: {}", e);
            // Map rllm::error::LLMError to a user-friendly message
            let error_message = match e {
                LLMError::HttpError(msg) => format!("Network error contacting Gemini: {}", msg),
                LLMError::AuthError(msg) => format!("Gemini authentication/authorization error: {}", msg),
                LLMError::InvalidRequest(msg) => format!("Invalid request sent to Gemini: {}", msg),
                LLMError::ProviderError(msg) => format!("Gemini provider error: {}", msg), // Includes rate limits, API errors etc.
                LLMError::JsonError(msg) => format!("Error processing Gemini response: {}", msg),
                // Add a catch-all for safety, though the enum definition seems exhaustive
                // _ => format!("An unexpected error occurred: {}", e),
            };
            // Return an error response through the standard tool result mechanism
             let tool_res = standard_tool_result(error_message.clone(), Some(true));
             // Even though it's an error *from* the tool, we return it as a *successful* RPC call
             // containing the error details within the CallToolResult.
             Ok(standard_success_response(id, json!(tool_res)))
        }
    }
}

// --- Tool Trait Implementation ---

#[derive(Debug)]
pub struct PlannerTool;

impl Tool for PlannerTool {
    fn name(&self) -> &str {
        "planning_tool"
    }

    fn info(&self) -> ToolInfo {
        planner_tool_info()
    }

    fn execute(&self, params: CallToolParams, id: Option<Value>) -> ExecuteFuture {
        Box::pin(async move {
            match serde_json::from_value::<PlannerParams>(params.arguments.clone()) {
                Ok(planner_params) => {
                    // Call the specific handler function
                    handle_planning_tool_call(planner_params, id).await
                }
                Err(e) => {
                    error!("Failed to parse PlannerParams: {}", e);
                    Ok(standard_error_response(
                        id,
                        INVALID_PARAMS,
                        &format!("Invalid parameters for planning_tool: {}", e),
                    ))
                }
            }
        })
    }
}
