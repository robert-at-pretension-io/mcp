use anyhow::Result;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema; // Added
use std::process::Command;

use tracing::{debug, error}; // Added tracing
// Import specific items from rmcp instead of prelude
use rmcp::tool;

// Removed unused shared_protocol_objects::ToolInfo import


#[derive(Debug, Serialize, Deserialize, JsonSchema)] // Added JsonSchema
pub struct BashParams {
    #[schemars(description = "The bash command to execute")] // Added
    pub command: String,
    #[serde(default = "default_cwd")]
    #[schemars(description = "The working directory for the command (defaults to current dir)")] // Added
    pub cwd: String,
}

fn default_cwd() -> String {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
        .to_string_lossy()
        .to_string()
}

#[derive(Debug)]
pub struct BashResult {
    pub success: bool,
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct BashExecutor;

impl BashExecutor {
    pub fn new() -> Self {
        BashExecutor
    }

    // Removed tool_info method as it's handled by the SDK macro now

    pub async fn execute(&self, params: BashParams) -> Result<BashResult> {
        // Create working directory if it doesn't exist
        let cwd = std::path::PathBuf::from(&params.cwd);
        if !cwd.exists() {
            std::fs::create_dir_all(&cwd)?;
        }

        let output = Command::new("sh")
            .arg("-c")
            .arg(&params.command)
            .current_dir(&cwd)
            .output()?;

        // Check if there were permission issues
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("permission denied") {
                return Err(anyhow::anyhow!("Permission denied. Try running with appropriate permissions or in a different directory."));
            }
        }

        Ok(BashResult {
            success: output.status.success(),
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

// --- New SDK Implementation ---

#[derive(Debug, Clone)] // Added Clone
pub struct BashTool;

impl BashTool {
    // Add a constructor
    pub fn new() -> Self {
        Self
    }
}

// Remove the tool_box macro here, as McpToolServer handles registration
impl BashTool {
    // Make the method public so McpToolServer can call it
    #[tool(description = "Executes bash shell commands on the host system. Use this tool to run system commands, check files, process text, manage files/dirs. Runs in a non-interactive `sh` shell.")] // Use description from old info (fixed quotes around sh)
    pub async fn bash( // Changed to pub async fn
        &self,
        #[tool(aggr)] params: BashParams // Automatically aggregates JSON args into BashParams
    ) -> String { // Return String directly
        debug!("Executing bash tool with params: {:?}", params);
        let executor = BashExecutor::new();

        // Execute the command and handle the Result explicitly
        match executor.execute(params).await {
            Ok(result) => {
                // Format the success/failure message as before
                format!(
                    "Command completed with status {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                    result.status,
                    result.stdout,
                    result.stderr
                )
            }
            Err(e) => {
                // If the executor itself fails, format the error into the returned string
                let error_message = format!("Failed to execute bash command: {}", e);
                error!("BashExecutor failed: {}", error_message); // Log the error
                // Return the error message as the tool's output string
                format!("TOOL EXECUTION ERROR: {}", error_message)
            }
        }
    }
}
