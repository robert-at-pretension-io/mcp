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

        // Get API key and model from environment variables
        let api_key = std::env::var("AIDER_API_KEY").ok();
        let model = std::env::var("AIDER_MODEL").ok();

        // Build the command
        let mut cmd_args = vec![
            "--message".to_string(),
            params.message.clone(),
            "--yes-always".to_string(),
            "--no-detect-urls".to_string(),
        ];

        // Add API key if available in environment
        if let Some(key) = api_key {
            // Pass the API key directly without requiring provider= format
            cmd_args.push("--api-key".to_string());
            cmd_args.push(format!("anthropic={}", key));
        }

        // Add model if available in environment
        if let Some(m) = model {
            cmd_args.push("--model".to_string());
            cmd_args.push(m);
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
            
            The tool requires:
            - A directory path where the code exists
            - A detailed message describing what changes to make
            
            Environment variables:
            - AIDER_API_KEY: Your Anthropic API key (without the 'anthropic=' prefix)
            - AIDER_MODEL: The model to use (e.g., 'claude-3-opus-20240229', 'claude-3-sonnet-20240229')
            
            Best practices for messages:
            - Be specific about what files or components to modify
            - Describe the desired behavior or functionality clearly
            - Provide context about the existing codebase structure
            - Include any constraints or requirements to follow
            
            Examples of good messages:
            - \"Add unit tests for the Customer class in src/models/customer.rb testing the validation logic\"
            - \"Implement pagination for the user listing API in the controllers/users_controller.js file\"
            - \"Fix the bug in utils/date_formatter.py where dates before 1970 aren't handled correctly\"
            - \"Refactor the authentication middleware in middleware/auth.js to use async/await instead of callbacks\"
            
            Note: This tool runs aider with the --yes-always flag which automatically accepts all proposed changes."
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
