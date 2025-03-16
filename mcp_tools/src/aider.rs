use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use tokio::process::Command;
use tracing::{debug, error, info};

use shared_protocol_objects::ToolInfo;

#[derive(Debug, Serialize, Deserialize)]
pub struct AiderParams {
    /// The directory to run aider in (must exist)
    pub directory: String,
    /// The message to send to aider
    pub message: String,
    /// Additional options to pass to aider (optional)
    #[serde(default)]
    pub options: Vec<String>,
    /// The provider to use (e.g., "anthropic", "openai")
    #[serde(default)]
    pub provider: Option<String>,
    /// The model to use (e.g., "claude-3-opus-20240229")
    #[serde(default)]
    pub model: Option<String>,
    /// Number of thinking tokens for Anthropic models
    #[serde(default)]
    pub thinking_tokens: Option<u32>,
    /// Reasoning effort level for OpenAI models
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiderResult {
    /// Whether the aider command completed successfully
    pub success: bool,
    /// The exit status code
    pub status: i32,
    /// Standard output from aider
    pub stdout: String,
    /// Standard error from aider
    pub stderr: String,
    /// The directory the command was run in
    pub directory: String,
    /// The message that was sent to aider
    pub message: String,
}

pub struct AiderExecutor;

impl AiderExecutor {
    pub fn new() -> Self {
        AiderExecutor
    }

    pub async fn execute(&self, params: AiderParams) -> Result<AiderResult> {
        // Validate directory exists
        let dir_path = PathBuf::from(&params.directory);
        if !dir_path.exists() {
            return Err(anyhow!("Directory '{}' does not exist", params.directory));
        }
        if !dir_path.is_dir() {
            return Err(anyhow!("Path '{}' is not a directory", params.directory));
        }

        // Basic validation of the message
        if params.message.trim().is_empty() {
            return Err(anyhow!("Message cannot be empty"));
        }

        // Get provider from params or default to "anthropic"
        let provider = params.provider.clone().unwrap_or_else(|| "anthropic".to_string());
        
        // Check for provider-specific API key first, then fall back to AIDER_API_KEY
        let provider_env_key = format!("{}_API_KEY", provider.to_uppercase());
        let api_key = std::env::var(&provider_env_key)
            .or_else(|_| {
                debug!("Provider-specific API key {} not found, falling back to AIDER_API_KEY", provider_env_key);
                std::env::var("AIDER_API_KEY")
            })
            .ok();
            
        // Log warning if no API key is found
        if api_key.is_none() {
            error!("No API key found for provider '{}'. Checked {} and AIDER_API_KEY", 
                provider, provider_env_key);
        }
        
        // Get model from params or environment variables
        let model = params.model.or_else(|| std::env::var("AIDER_MODEL").ok());

        // Build the command
        let mut cmd_args = vec![
            "--message".to_string(),
            params.message.clone(),
            "--yes-always".to_string(),
            "--no-detect-urls".to_string(),
        ];

        // Add API key if available in environment
        if let Some(key) = api_key {
            // Pass the API key with the specified provider
            cmd_args.push("--api-key".to_string());
            cmd_args.push(format!("{}={}", provider, key));
        }

        // Add model if available
        if let Some(m) = model {
            cmd_args.push("--model".to_string());
            cmd_args.push(m);
        }

        // Add thinking tokens for Anthropic models
        if let Some(tokens) = params.thinking_tokens {
            if provider.to_lowercase() == "anthropic" {
                cmd_args.push("--thinking-tokens".to_string());
                cmd_args.push(tokens.to_string());
            } else {
                debug!("Ignoring thinking_tokens as provider is not Anthropic");
            }
        }

        // Add reasoning effort for OpenAI models
        if let Some(effort) = &params.reasoning_effort {
            if provider.to_lowercase() == "openai" {
                cmd_args.push("--reasoning-effort".to_string());
                cmd_args.push(effort.clone());
            } else {
                debug!("Ignoring reasoning_effort as provider is not OpenAI");
            }
        }

        // Add any additional options
        cmd_args.extend(params.options.iter().cloned());

        debug!("Running aider with args: {:?}", cmd_args);
        info!("Executing aider in directory: {}", params.directory);

        // Execute aider command
        let output = Command::new("aider")
            .args(&cmd_args)
            .current_dir(&params.directory)
            .output()
            .await
            .map_err(|e| anyhow!("Failed to execute aider: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Log results
        if !output.status.success() {
            error!("Aider command failed with status: {:?}", output.status);
            if !stderr.is_empty() {
                error!("Stderr: {}", stderr);
            }
        } else {
            info!("Aider command completed successfully");
            debug!("Stdout length: {}", stdout.len());
        }

        Ok(AiderResult {
            success: output.status.success(),
            status: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
            directory: params.directory,
            message: params.message,
        })
    }
}

/// Returns the tool info for the aider tool
pub fn aider_tool_info() -> ToolInfo {
    ToolInfo {
        name: "aider".to_string(),
        description: Some(
            "AI pair programming tool for making targeted code changes. Use this tool to:
            
            1. Implement new features or functionality in existing code
            2. Add tests to an existing codebase
            3. Fix bugs in code
            4. Refactor or improve existing code
            5. Make structural changes across multiple files

            When using aider, make sure to pass ALL of the context into the message needed for a particular issue. don't just provide the solution.
            
            The tool requires:
            - A directory path where the code exists
            - A detailed message describing what changes to make. Please only describe one change per message. If you need to make multiple changes, please submit multiple requests. You must include all context required because this tool doesn't have any memory of previous requests.
            
            Best practices for messages:
            - Clearly describe the problem we're seeing in the tests
            - Show the relevant code that's failing
            - Explain why it's failing
            - Provide the specific error messages
            - Outline the approach to fix it
            - Include any related code that might be affected by the changes
            
            Examples of good messages:
            - \"Add unit tests for the Customer class in src/models/customer.rb testing the validation logic\"
            - \"Implement pagination for the user listing API in the controllers/users_controller.js file\"
            - \"Fix the bug in utils/date_formatter.py where dates before 1970 aren't handled correctly\"
            - \"Refactor the authentication middleware in middleware/auth.js to use async/await instead of callbacks\"
            
            Note: This tool runs aider with the --yes-always flag which automatically accepts all proposed changes.
            
            Advanced features:
            - For Anthropic models (Claude), you can set 'thinking_tokens' to control how much thinking the model does before responding
            - For OpenAI models, you can set 'reasoning_effort' to control the level of reasoning (e.g., 'auto', 'low', 'medium', 'high')"
                .to_string(),
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "directory": {
                    "type": "string",
                    "description": "The directory path where aider should run (must exist and contain code files)"
                },
                "message": {
                    "type": "string",
                    "description": "Detailed instructions for what changes aider should make to the code"
                },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Additional command-line options to pass to aider (optional)"
                },
                "provider": {
                    "type": "string",
                    "description": "The provider to use (e.g., 'anthropic', 'openai'). Defaults to 'anthropic' if not specified."
                },
                "model": {
                    "type": "string",
                    "description": "The model to use (e.g., 'claude-3-opus-20240229'). Falls back to AIDER_MODEL environment variable if not specified."
                },
                "thinking_tokens": {
                    "type": "integer",
                    "description": "Number of thinking tokens to use for Anthropic models (Claude). Higher values allow more thorough reasoning."
                },
                "reasoning_effort": {
                    "type": "string",
                    "description": "Reasoning effort level for OpenAI models. Values: 'auto', 'low', 'medium', 'high'."
                }
            },
            "required": ["directory", "message"],
            "additionalProperties": false
        }),
    }
}

/// Handler function for aider tool calls
pub async fn handle_aider_tool_call(params: AiderParams) -> Result<AiderResult> {
    let executor = AiderExecutor::new();
    executor.execute(params).await
}
