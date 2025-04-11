use anyhow::Result;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema; // Added
use std::process::Command;
use serde_json::json;
use tracing::{debug, error}; // Added tracing
// Import specific items from rmcp instead of prelude
use rmcp::{tool, Error as RmcpError, ServerHandler, model::ServerInfo};

use shared_protocol_objects::ToolInfo; // Keep for quick_bash_tool_info for now

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

#[tool(tool_box)] // Apply the SDK macro
impl BashTool {
    #[tool(description = "Executes bash shell commands on the host system. Use this tool to run system commands, check files, process text, manage files/dirs. Runs in a non-interactive `sh` shell.")] // Use description from old info (fixed quotes around sh)
    async fn bash(
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

// Optional: Implement ServerHandler if needed for server-level info (like instructions)
#[tool(tool_box)]
impl ServerHandler for BashTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            // Add instructions if desired, otherwise use default
            instructions: Some("A tool for executing bash commands.".into()),
            ..Default::default()
        }
    }
}

// --- End New SDK Implementation ---


#[derive(Debug, Serialize, Deserialize)]
pub struct QuickBashParams {
    pub cmd: String,
}

pub fn quick_bash_tool_info() -> ToolInfo {
    ToolInfo {
        name: "quick_bash".to_string(),
        description: Some(
            "Fast shell command tool for simple one-liners. Use this to:
            
            1. Run quick system checks (ls, ps, grep, find, etc.)
            2. View file contents (cat, head, tail, less)
            3. Create, move, or delete files and directories
            4. Process text with utilities like grep, sed, awk
            
            Advantages over regular bash tool:
            - Streamlined interface for common commands
            - Optimized for one-line operations
            - Focuses on readable command output
            - Perfect for file system operations and text processing
            
            Example commands:
            - `ls -la /path/to/dir`
            - `grep -r \"pattern\" /search/path`
            - `find . -name \"*.txt\" -mtime -7`
            - `cat file.txt | grep pattern | wc -l`
            - `du -sh /path/to/dir`
            
            Note: Commands run with your current user permissions.".to_string() // Fixed backticks in examples
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "cmd": {
                    "type": "string",
                    "description": "The shell command to execute"
                }
            },
            "required": ["cmd"],
            "additionalProperties": false
        }),
        annotations: None, // Added missing field
    }
}

pub async fn handle_quick_bash(params: QuickBashParams) -> Result<BashResult> {
    let executor = BashExecutor::new();
    
    // Convert the quick bash params to regular bash params
    let bash_params = BashParams {
        command: params.cmd,
        cwd: default_cwd(),  // Always use the current working directory
    };
    
    // Execute the command using the existing executor
    executor.execute(bash_params).await
}
