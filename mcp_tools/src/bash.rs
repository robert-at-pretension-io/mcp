use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;
use serde_json::json;

use shared_protocol_objects::ToolInfo;

#[derive(Debug, Serialize, Deserialize)]
pub struct BashParams {
    pub command: String,
    #[serde(default = "default_cwd")]
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

    pub fn tool_info(&self) -> ToolInfo {
        ToolInfo {
            name: "bash".to_string(),
            description: Some(
                "System command execution tool for OS-level operations.

                **COMMANDMENTS:**
                - VERIFY command safety before execution
                - CHECK outputs for success/failure
                - AVOID interactive commands
                - USE specific paths, not wildcards
                
                **Use this tool to:**
                - Run system utilities (ls, grep, etc)
                - Manage files and directories
                - Execute scripts and programs
                - Process command outputs
                
                **Returns:** Command's stdout/stderr and success status.
                
                **Safety:** No sensitive ops, passwords, or user interaction."
                .to_string()
            ),
            input_schema: json!({})
        }
    }

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

pub fn bash_tool_info() -> ToolInfo {
    ToolInfo {
        name: BashExecutor::new().tool_info().name,
        description: BashExecutor::new().tool_info().description,
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "The working directory for the command"
                }
            },
            "required": ["command"],
            "additionalProperties": false
        }),
    }
}
