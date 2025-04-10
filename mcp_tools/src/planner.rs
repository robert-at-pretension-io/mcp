use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use tracing::{debug, error, info};

use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chat::{ChatMessageBuilder, ChatRole, ChatResponse}; // Added ChatRole, ChatResponse
// Removed rllm::error::LLMError import, will use llm::error::LLMError

use shared_protocol_objects::{
    CallToolParams, JsonRpcResponse, ToolInfo, INTERNAL_ERROR, INVALID_PARAMS, // Import error codes from here
};
use crate::tool_trait::{
    ExecuteFuture, Tool, standard_error_response, standard_success_response, standard_tool_result,
    // Removed INTERNAL_ERROR, INVALID_PARAMS from here
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PlannerParams {
    pub user_request: String,
    pub ai_interpretation: String,
    pub available_tools: Vec<ToolInfo>,
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
                    "type": "array",
                    "description": "A list of all tools available to the AI, including name, description, and input schema.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "description": {"type": ["string", "null"]},
                            "input_schema": {"type": "object"}
                        },
                        "required": ["name", "input_schema"]
                    }
                }
            },
            "required": ["user_request", "ai_interpretation", "available_tools"]
        }),
    }
}

/// Formats the tool list into a string suitable for the LLM prompt.
fn format_tool_list(tools: &[ToolInfo]) -> String {
    tools
        .iter()
        .map(|tool| {
            format!(
                "- Name: {}\n  Description: {}\n  Input Schema: {}\n",
                tool.name,
                tool.description.as_deref().unwrap_or("No description"),
                serde_json::to_string_pretty(&tool.input_schema).unwrap_or_else(|_| "{}".to_string())
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Calls the Gemini API via RLLM to generate a plan.
// Return type changed to Result<Box<dyn ChatResponse>, llm::error::LLMError>
async fn generate_plan_with_gemini(prompt: &str) -> Result<Box<dyn ChatResponse>, llm::error::LLMError> {
    let api_key = env::var("GEMINI_API_KEY")
        // Use llm::error::LLMError variant
        .map_err(|e| llm::error::LLMError::ConfigurationError(format!("GEMINI_API_KEY not set: {}", e)))?;

    info!("Building Gemini LLM client using RLLM");
    let llm = LLMBuilder::new()
        .backend(LLMBackend::Google)
        .api_key(api_key)
        .model("gemini-1.5-pro-latest") // Using a capable model for planning
        .temperature(0.5) // Lower temperature for more deterministic planning
        .build()?;

    // Construct messages for the chat API using the new builder pattern
    let messages = vec![
        ChatMessageBuilder::new(ChatRole::System)
            .content(
                "You are an expert planning assistant. Your goal is to create a robust, step-by-step plan \
                 to achieve a user's objective using a predefined set of tools. The plan should be clear, \
                 actionable, and account for potential issues. Specify when the AI should pause to reflect, \
                 wait for tool results, or handle errors. Output only the plan itself, without preamble or explanation."
            )
            .build(),
        ChatMessageBuilder::new(ChatRole::User).content(prompt).build(),
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

    // Format the available tools for the prompt
    let formatted_tools = format_tool_list(&params.available_tools);

    // Construct the detailed prompt for Gemini
    let prompt = format!(
        "Generate a plan based on the following information:\n\n\
         User Request:\n\"{}\"\n\n\
         AI Interpretation of Goal:\n\"{}\"\n\n\
         Available Tools:\n{}\n\
         ------------------------------------\n\
         PLAN:",
        params.user_request, params.ai_interpretation, formatted_tools
    );

    match generate_plan_with_gemini(&prompt).await {
        // Handle the Box<dyn ChatResponse>
        Ok(response_box) => {
            let plan = response_box.content(); // Extract content string
            info!("Successfully generated plan from Gemini");
            debug!("Generated Plan:\n{}", plan);
            let tool_res = standard_tool_result(plan, None);
            Ok(standard_success_response(id, json!(tool_res)))
        }
        // Use llm::error::LLMError variants
        Err(e) => {
            error!("Error generating plan using Gemini via RLLM: {}", e);
            // Map llm::error::LLMError to a user-friendly message
            let error_message = match e {
                llm::error::LLMError::ApiError(msg) => format!("Gemini API error: {}", msg),
                llm::error::LLMError::AuthenticationError(_) => "Gemini authentication failed. Check GEMINI_API_KEY.".to_string(),
                llm::error::LLMError::ConfigurationError(msg) => format!("Configuration error: {}", msg),
                llm::error::LLMError::NetworkError(msg) => format!("Network error contacting Gemini: {}", msg),
                llm::error::LLMError::RateLimitError(_) => "Gemini rate limit exceeded.".to_string(),
                llm::error::LLMError::InvalidResponseError(msg) => format!("Invalid response from Gemini: {}", msg),
                _ => format!("An unexpected error occurred: {}", e),
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
