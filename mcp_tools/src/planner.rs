use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{debug, error, info, warn};
use schemars::JsonSchema;

use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chat::{ChatMessageBuilder, ChatResponse, ChatRole};
use rllm::error::LLMError;

// Import rmcp SDK components
use rmcp::tool;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PlannerParams {
    #[schemars(description = "The original request from the user.")]
    pub user_request: String,
    
    #[schemars(description = "The AI's interpretation or summary of the user's request and goal.")]
    pub ai_interpretation: String,
    
    #[schemars(description = "A formatted string listing all tools available to the AI, including only their name and description (excluding input schema).")]
    pub available_tools: String,
}

#[derive(Debug, Clone)]
pub struct PlannerTool;

impl PlannerTool {
    pub fn new() -> Self {
        Self
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

// Replaced by PlannerTool.generate_plan method

impl PlannerTool {
    // Helper method to generate a plan using the provided parameters
    async fn generate_plan(&self, params: PlannerParams) -> Result<String> {
        // Construct the detailed prompt for Gemini using the provided string
        let prompt = format!(
            "Generate a plan based on the following information:\n\n\
             User Request:\n\"{}\"\n\n\
             AI Interpretation of Goal:\n\"{}\"\n\n\
             Available Tools:\n{}\n\
             ------------------------------------\n\
             PLAN:",
            params.user_request, params.ai_interpretation, params.available_tools
        );

        match generate_plan_with_gemini(&prompt).await {
            Ok(response_box) => {
                let plan_option = response_box.text();
                info!("Successfully generated plan from Gemini");
                debug!("Generated Plan:\n{:?}", plan_option);
                
                let plan_text = plan_option.unwrap_or_else(|| {
                    warn!("Gemini response text was None, returning empty plan.");
                    String::new()
                });
                
                Ok(plan_text)
            }
            Err(e) => {
                error!("Error generating plan using Gemini via RLLM: {}", e);
                
                // Map rllm::error::LLMError to a user-friendly message
                let error_message = match e {
                    LLMError::HttpError(msg) => format!("Network error contacting Gemini: {}", msg),
                    LLMError::AuthError(msg) => format!("Gemini authentication/authorization error: {}", msg),
                    LLMError::InvalidRequest(msg) => format!("Invalid request sent to Gemini: {}", msg),
                    LLMError::ProviderError(msg) => format!("Gemini provider error: {}", msg),
                    LLMError::JsonError(msg) => format!("Error processing Gemini response: {}", msg),
                };
                
                Err(anyhow!(error_message))
            }
        }
    }
}

#[tool(tool_box)]
impl PlannerTool {
    #[tool(description = "Generates a multi-step plan using available tools to fulfill a user request. Provide the original user request, the AI's interpretation of that request, and a list of all available tools (including their descriptions and parameters). The tool will call a powerful LLM (Gemini) to devise a plan, including potential contingencies and points for reflection or waiting for results.")]
    pub async fn planning_tool(
        &self,
        #[tool(aggr)] params: PlannerParams
    ) -> String {
        info!("Generating plan for user request: {}", params.user_request);
        
        match self.generate_plan(params).await {
            Ok(plan) => plan,
            Err(e) => {
                error!("Failed to generate plan: {}", e);
                format!("Error generating plan: {}", e)
            }
        }
    }
}
