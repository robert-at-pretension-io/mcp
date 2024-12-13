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
                "Executes commands in a bash shell environment. USE THIS TOOL TO RUN SYSTEM-LEVEL OPERATIONS, MANAGE FILES, AND PERFORM UTILITIES.
                
                **When to Use:**
                - When an operation requires direct interaction with the operating system.
                - To perform file system operations like creating directories, reading files, or moving content.
                - To run system utilities like `ls`, `grep`, `cat`, `date`, `ping`, or `wget`.
                - To execute scripts or other CLI tools.
    
                **Input:**
                - Expects a single string representing the bash command.
                    - Avoid any commands that could have unintended side effects or cause harm to the system.
                    - Be specific and avoid using wildcard characters for clarity.
    
                **Output:**
                - Returns the command's standard output (STDOUT), and standard error (STDERR) as strings.
                - Indicates if the command was successful via `is_error` flag. If the command fails, `is_error` will be `true` and the STDERR will contain the details of the failure.
                
                **Usage Constraints:**
                - Do not use this tool for sensitive operations or password management.
                -  Do not run commands that might require user interaction.
                - Only use if necessary. Favor existing or specialized tools.
                - Be specific, precise and make sure to include required arguments and options for the command.
                - ALWAYS examine the output (both stdout and stderr) to confirm a successful execution.
            ".to_string()
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
