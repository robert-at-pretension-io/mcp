use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::env;
use std::process::Command;
use tracing::{debug, error, warn};

// --- Parameter Structs ---

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NetlifyParams {
    #[schemars(description = "The Netlify CLI command arguments (e.g., 'sites:list', 'deploy --prod')")]
    pub command_args: String,
    #[serde(default = "default_cwd")]
    #[schemars(description = "The working directory for the command (defaults to current dir)")]
    pub cwd: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NetlifyHelpParams {
    #[schemars(description = "Optional Netlify command to get specific help for (e.g., 'deploy', 'sites:create')")]
    pub command: Option<String>,
    #[serde(default = "default_cwd")]
    #[schemars(description = "The working directory for the command (defaults to current dir)")]
    pub cwd: String,
}


fn default_cwd() -> String {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
        .to_string_lossy()
        .to_string()
}

// --- Result Struct (similar to BashResult) ---

#[derive(Debug)]
struct NetlifyExecutionResult {
    success: bool,
    status: i32,
    stdout: String,
    stderr: String,
}

// --- Tool Struct and Implementation ---

#[derive(Debug, Clone)]
pub struct NetlifyTool;

impl NetlifyTool {
    pub fn new() -> Self {
        Self
    }

    // Helper function to execute netlify commands
    async fn execute_netlify_command(
        &self,
        command_str: &str, // The full command string including subcommands and flags
        cwd: &str,
        append_auth: bool, // Flag to control appending --auth
    ) -> Result<NetlifyExecutionResult> {
        let token = if append_auth {
            env::var("NETLIFY_AUTH_TOKEN").map_err(|_| {
                anyhow!("NETLIFY_AUTH_TOKEN environment variable not set. Cannot authenticate.")
            })?
        } else {
            String::new() // No token needed if not appending auth
        };

        // Construct the full command with auth token if needed
        let full_command = if append_auth && !token.is_empty() {
            format!("{} --auth {}", command_str, token)
        } else {
            command_str.to_string()
        };

        debug!("Executing Netlify command: netlify {}", full_command);
        debug!("Working directory: {}", cwd);

        let cwd_path = std::path::PathBuf::from(cwd);
        if !cwd_path.exists() {
            // Attempt to create, but warn if it fails, as it might not be necessary for all commands
            if let Err(e) = std::fs::create_dir_all(&cwd_path) {
                 warn!("Failed to create working directory '{}': {}. Proceeding anyway.", cwd, e);
            }
        }

        // Use Command::new("netlify") and pass the rest as arguments
        // Splitting the command_str naively by space might break commands with quoted args.
        // A more robust approach would involve shell parsing, but for typical CLI usage,
        // passing the whole string to `sh -c` (like bash tool) or splitting might suffice.
        // Let's try splitting for now, assuming simple command structures.
        // Alternatively, we could require the user to pass args correctly separated.
        // Let's stick to the bash approach for simplicity and robustness with complex args.
        let output = Command::new("sh") // Use sh -c to handle complex args/quotes
            .arg("-c")
            .arg(format!("netlify {}", full_command)) // Prepend netlify here
            .current_dir(&cwd_path)
            .output()?;

        let result = NetlifyExecutionResult {
            success: output.status.success(),
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        };

        if !result.success {
            error!(
                "Netlify command failed with status {}. Stderr: {}",
                result.status, result.stderr
            );
        }

        Ok(result)
    }

    // --- Tool Methods ---

    #[rmcp::tool(description = "Executes Netlify CLI commands. Requires NETLIFY_AUTH_TOKEN env var. Provide the command arguments *after* 'netlify' (e.g., 'sites:list', 'deploy --prod').")]
    pub async fn netlify(
        &self,
        #[rmcp::tool(aggr)] params: NetlifyParams,
    ) -> String {
        debug!("Executing netlify tool with params: {:?}", params);

        match self.execute_netlify_command(&params.command_args, &params.cwd, true).await {
            Ok(result) => {
                format!(
                    "Netlify command completed with status {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                    result.status, result.stdout, result.stderr
                )
            }
            Err(e) => {
                let error_message = format!("Failed to execute netlify command: {}", e);
                error!("{}", error_message);
                format!("TOOL EXECUTION ERROR: {}", error_message)
            }
        }
    }

    #[rmcp::tool(description = "Gets help for the Netlify CLI or a specific command. Does not require auth token.")]
    pub async fn netlify_help(
        &self,
        #[rmcp::tool(aggr)] params: NetlifyHelpParams,
    ) -> String {
        debug!("Executing netlify_help tool with params: {:?}", params);

        let command_to_run = match params.command {
            Some(cmd) => format!("{} --help", cmd),
            None => "--help".to_string(),
        };

        // Execute without appending auth token
        match self.execute_netlify_command(&command_to_run, &params.cwd, false).await {
             Ok(result) => {
                // Help usually goes to stdout
                format!(
                    "Netlify help command completed with status {}\n\nHELP OUTPUT (STDOUT):\n{}\n\nSTDERR:\n{}",
                    result.status, result.stdout, result.stderr
                )
            }
            Err(e) => {
                let error_message = format!("Failed to execute netlify help command: {}", e);
                error!("{}", error_message);
                format!("TOOL EXECUTION ERROR: {}", error_message)
            }
        }
    }
}
