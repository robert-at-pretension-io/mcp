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
    /// The provider that was used (e.g., "anthropic", "openai")
    pub provider: String,
    /// The model that was used (e.g., "claude-3-opus-20240229")
    pub model: Option<String>,
}

pub struct AiderExecutor;

impl AiderExecutor {
    pub fn new() -> Self {
        AiderExecutor
    }

    /// Helper method to build command arguments for testing
    pub fn build_command_args(&self, params: &AiderParams) -> Vec<String> {
        // Determine provider: first use explicit parameter, otherwise detect available API keys
        let provider = if let Some(p) = params.provider.clone() {
            let p_l = p.to_lowercase();
            if p_l != "anthropic" && p_l != "openai" {
                error!("Unsupported provider '{}'. Defaulting to 'anthropic'", p);
                "anthropic".to_string()
            } else {
                p_l
            }
        } else {
            let has_anthropic = std::env::var("ANTHROPIC_API_KEY").is_ok();
            let has_openai = std::env::var("OPENAI_API_KEY").is_ok();
            if has_anthropic && !has_openai {
                "anthropic".to_string()
            } else if has_openai && !has_anthropic {
                "openai".to_string()
            } else if has_anthropic && has_openai {
                // If both providers have keys, maintain current default preference
                "anthropic".to_string()
            } else {
                // Default to anthropic if no API keys are found
                "anthropic".to_string()
            }
        };
        
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
        
        // Get model from params, environment variables, or set default based on provider
        let model = params.model
            .clone()
            .or_else(|| std::env::var("AIDER_MODEL").ok())
            .or_else(|| {
                // Set default models based on provider
                match provider.to_lowercase().as_str() {
                    "anthropic" => {
                        debug!("Using default Anthropic model: anthropic/claude-3-7-sonnet-20250219");
                        Some("anthropic/claude-3-7-sonnet-20250219".to_string())
                    },
                    "openai" => {
                        debug!("Using default OpenAI model: openai/o1");
                        Some("openai/o3-mini".to_string())
                    },
                    _ => None
                }
            });

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
        if let Some(m) = &model {
            cmd_args.push("--model".to_string());
            cmd_args.push(m.clone());
            info!("Using provider '{}' with model '{}'", provider, m);
        } else {
            info!("Using provider '{}' with no specific model", provider);
        }

        // Add thinking tokens for Anthropic models
        if provider.to_lowercase() == "anthropic" {
            let tokens = params.thinking_tokens.unwrap_or(32000);
            cmd_args.push("--thinking-tokens".to_string());
            cmd_args.push(tokens.to_string());
            debug!("Using thinking_tokens: {}", tokens);
        }

        // Add reasoning effort for OpenAI models
        if provider.to_lowercase() == "openai" {
            let effort = params.reasoning_effort.as_deref().unwrap_or("high");
            // Validate reasoning_effort - only allow "low", "medium", "high"
            let valid_efforts = ["low", "medium", "high"];
            let validated_effort = if valid_efforts.contains(&effort.to_lowercase().as_str()) {
                effort.to_string()
            } else {
                error!("Invalid reasoning_effort '{}'. Defaulting to 'high'", effort);
                "high".to_string()
            };
            
            cmd_args.push("--reasoning-effort".to_string());
            cmd_args.push(validated_effort.clone());
            debug!("Using reasoning_effort: {}", validated_effort);
        }

        // Add any additional options
        cmd_args.extend(params.options.iter().cloned());
        
        cmd_args
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

        // Build command arguments
        let cmd_args = self.build_command_args(&params);
        let provider = params.provider.clone().unwrap_or_else(|| "anthropic".to_string());
        let model = params.model.clone();

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
            provider: provider.clone(),
            model: model.clone(),
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
            - Specify the file paths that include relevent context for the problem
            
            
            Note: This tool runs aider with the --yes-always flag which automatically accepts all proposed changes.
            
            MODEL AND PROVIDER OPTIONS:
            This tool supports both Anthropic (Claude) and OpenAI models. You can specify which provider and model to use:
            
            - Default provider: 'anthropic' with model 'anthropic/claude-3-7-sonnet-20250219'
            - Alternative provider: 'openai' with default model 'openai/o3-mini'
            
            Examples of provider/model usage:
            - Basic usage (uses default Anthropic model): {\"directory\": \"/path/to/code\", \"message\": \"Fix the bug\"}
            - Specify provider: {\"directory\": \"/path/to/code\", \"message\": \"Fix the bug\", \"provider\": \"openai\"}
            - Specify provider and model: {\"directory\": \"/path/to/code\", \"message\": \"Fix the bug\", \"provider\": \"anthropic\", \"model\": \"claude-3-opus-20240229\"}
            
            ADVANCED FEATURES:
            - For Anthropic models (Claude), the default 'thinking_tokens' is set to 32000 for optimal performance, but you can override it:
              Example: {\"directory\": \"/path/to/code\", \"message\": \"Fix the bug\", \"provider\": \"anthropic\", \"thinking_tokens\": 16000}
            
            - For OpenAI models, the default 'reasoning_effort' is set to 'high' for optimal performance, but you can override it:
              Example: {\"directory\": \"/path/to/code\", \"message\": \"Fix the bug\", \"provider\": \"openai\", \"reasoning_effort\": \"medium\"}
              Valid values: 'auto', 'low', 'medium', 'high'
            
            Note: The tool will look for API keys in environment variables. It first checks for provider-specific keys 
            (ANTHROPIC_API_KEY or OPENAI_API_KEY) and then falls back to AIDER_API_KEY if needed."
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
        annotations: None, // Added missing field
    }
}

/// Handler function for aider tool calls
pub async fn handle_aider_tool_call(params: AiderParams) -> Result<AiderResult> {
    let executor = AiderExecutor::new();
    executor.execute(params).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::Path;
    use tokio::fs;
    use tokio::runtime::Runtime;

    // Helper function to create a temporary directory for testing
    async fn create_temp_dir() -> Result<String> {
        let temp_dir = format!("/tmp/aider_test_{}", std::process::id());
        if !Path::new(&temp_dir).exists() {
            fs::create_dir_all(&temp_dir).await?;
        }
        Ok(temp_dir)
    }

    // Test provider validation logic
    #[test]
    fn test_provider_validation() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let temp_dir = create_temp_dir().await.unwrap();
            let executor = AiderExecutor::new();
            
            // Test with valid provider: anthropic
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("anthropic".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: None,
            };
            
            // We don't actually execute the command, just check the validation logic
            // by inspecting the command that would be built
            let cmd_args = executor.build_command_args(&params);
            assert!(cmd_args.contains(&"--api-key".to_string()));
            
            // Test with valid provider: openai
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("openai".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: None,
            };
            
            let cmd_args = executor.build_command_args(&params);
            assert!(cmd_args.contains(&"--api-key".to_string()));
            
            // Test with invalid provider - should default to anthropic
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("invalid_provider".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: None,
            };
            
            let cmd_args = executor.build_command_args(&params);
            // The provider should be defaulted to anthropic
            assert!(cmd_args.iter().any(|arg| arg.contains("anthropic=")));
            
            // Handle cleanup gracefully
            let _ = fs::remove_dir_all(temp_dir).await;
        });
    }
    
    // Test default model selection logic
    #[test]
    fn test_default_model_selection() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let temp_dir = create_temp_dir().await.unwrap();
            let executor = AiderExecutor::new();
            
            // Test default model for anthropic
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("anthropic".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: None,
            };
            
            let cmd_args = executor.build_command_args(&params);
            assert!(cmd_args.contains(&"--model".to_string()));
            let model_index = cmd_args.iter().position(|arg| arg == "--model").unwrap();
            assert_eq!(cmd_args[model_index + 1], "anthropic/claude-3-7-sonnet-20250219");
            
            // Test default model for openai
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("openai".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: None,
            };
            
            let cmd_args = executor.build_command_args(&params);
            assert!(cmd_args.contains(&"--model".to_string()));
            let model_index = cmd_args.iter().position(|arg| arg == "--model").unwrap();
            assert_eq!(cmd_args[model_index + 1], "openai/o3-mini");
            
            // Test custom model overrides default
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("anthropic".to_string()),
                model: Some("claude-3-opus-20240229".to_string()),
                thinking_tokens: None,
                reasoning_effort: None,
            };
            
            let cmd_args = executor.build_command_args(&params);
            assert!(cmd_args.contains(&"--model".to_string()));
            let model_index = cmd_args.iter().position(|arg| arg == "--model").unwrap();
            assert_eq!(cmd_args[model_index + 1], "claude-3-opus-20240229");
            
            // Handle cleanup gracefully
            let _ = fs::remove_dir_all(temp_dir).await;
        });
    }
    
    // Test reasoning_effort validation
    #[test]
    fn test_reasoning_effort_validation() {
        let rt = Runtime::new().unwrap();
        
        rt.block_on(async {
            let temp_dir = create_temp_dir().await.unwrap();
            let executor = AiderExecutor::new();
            
            // Test valid reasoning_effort with OpenAI
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("openai".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: Some("high".to_string()),
            };
            
            let cmd_args = executor.build_command_args(&params);
            assert!(cmd_args.contains(&"--reasoning-effort".to_string()));
            let effort_index = cmd_args.iter().position(|arg| arg == "--reasoning-effort").unwrap();
            assert_eq!(cmd_args[effort_index + 1], "high");
            
            // Test invalid reasoning_effort with OpenAI - should default to high
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("openai".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: Some("invalid_effort".to_string()),
            };
            
            let cmd_args = executor.build_command_args(&params);
            assert!(cmd_args.contains(&"--reasoning-effort".to_string()));
            let effort_index = cmd_args.iter().position(|arg| arg == "--reasoning-effort").unwrap();
            assert_eq!(cmd_args[effort_index + 1], "high");
            
            // Test reasoning_effort with Anthropic - should be ignored
            let params = AiderParams {
                directory: temp_dir.clone(),
                message: "Test message".to_string(),
                options: vec![],
                provider: Some("anthropic".to_string()),
                model: None,
                thinking_tokens: None,
                reasoning_effort: Some("high".to_string()),
            };
            
            let cmd_args = executor.build_command_args(&params);
            assert!(!cmd_args.contains(&"--reasoning-effort".to_string()));
            
            // Handle cleanup gracefully
            let _ = fs::remove_dir_all(temp_dir).await;
        });
    }
}
