use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::env;
use std::process::Command;
use tracing::{debug, error, warn};

// Import the tool macro
use rmcp::tool;

// --- Parameter Structs ---

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SupabaseParams {
    #[schemars(description = "The Supabase CLI command arguments (e.g., 'projects list', 'functions deploy my-func')")]
    pub command_args: String,
    #[serde(default = "default_cwd")]
    #[schemars(description = "The working directory for the command (defaults to current dir)")]
    pub cwd: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SupabaseHelpParams {
    #[schemars(description = "Optional Supabase command to get specific help for (e.g., 'functions', 'db push')")]
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

// --- Result Struct (similar to BashResult/NetlifyExecutionResult) ---

#[derive(Debug)]
struct SupabaseExecutionResult {
    success: bool,
    status: i32,
    stdout: String,
    stderr: String,
}

// --- Tool Struct and Implementation ---

#[derive(Debug, Clone)]
pub struct SupabaseTool;

impl SupabaseTool {
    pub fn new() -> Self {
        Self
    }

    // Helper function to execute supabase commands
    async fn execute_supabase_command(
        &self,
        command_str: &str, // The full command string including subcommands and flags
        cwd: &str,
        append_token: bool, // Flag to control appending --token
    ) -> Result<SupabaseExecutionResult> {
        let token = if append_token {
            // Use SUPABASE_ACCESS_TOKEN, the standard env var for the CLI
            env::var("SUPABASE_ACCESS_TOKEN").map_err(|_| {
                anyhow!("SUPABASE_ACCESS_TOKEN environment variable not set. Cannot authenticate.")
            })?
        } else {
            String::new() // No token needed if not appending auth
        };

        // Construct the full command with token if needed
        // Note: Supabase CLI uses --token <token>
        let full_command = if append_token && !token.is_empty() {
            format!("{} --token {}", command_str, token)
        } else {
            command_str.to_string()
        };

        debug!("Executing Supabase command: supabase {}", full_command);
        debug!("Working directory: {}", cwd);

        let cwd_path = std::path::PathBuf::from(cwd);
        if !cwd_path.exists() {
            // Attempt to create, but warn if it fails
            if let Err(e) = std::fs::create_dir_all(&cwd_path) {
                 warn!("Failed to create working directory '{}': {}. Proceeding anyway.", cwd, e);
            }
        }

        // Use sh -c for robustness with complex args/quotes
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("supabase {}", full_command)) // Prepend supabase here
            .current_dir(&cwd_path)
            .output()?;

        let result = SupabaseExecutionResult {
            success: output.status.success(),
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        };

        if !result.success {
            error!(
                "Supabase command failed with status {}. Stderr: {}",
                result.status, result.stderr
            );
        }

        Ok(result)
    }

    // --- Tool Methods ---

    #[tool(description = "Executes authenticated Supabase CLI commands. Provide the command arguments *after* 'supabase' (e.g., 'projects list', 'functions deploy my-func'). Authentication is handled automatically via SUPABASE_ACCESS_TOKEN.")]
    pub async fn supabase(
        &self,
        #[tool(aggr)] params: SupabaseParams,
    ) -> String {
        debug!("Executing supabase tool with params: {:?}", params);

        match self.execute_supabase_command(&params.command_args, &params.cwd, true).await {
            Ok(result) => {
                format!(
                    "Supabase command completed with status {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                    result.status, result.stdout, result.stderr
                )
            }
            Err(e) => {
                let error_message = format!("Failed to execute supabase command: {}", e);
                error!("{}", error_message);
                format!("TOOL EXECUTION ERROR: {}", error_message)
            }
        }
    }

    #[tool(description = "Gets help for the Supabase CLI or a specific command.")]
    pub async fn supabase_help(
        &self,
        #[tool(aggr)] params: SupabaseHelpParams,
    ) -> String {
        debug!("Executing supabase_help tool with params: {:?}", params);

        let command_to_run = match params.command {
            Some(cmd) => format!("{} --help", cmd),
            None => "--help".to_string(),
        };

        // Execute without appending token
        match self.execute_supabase_command(&command_to_run, &params.cwd, false).await {
             Ok(result) => {
                // Help usually goes to stdout
                format!(
                    "Supabase help command completed with status {}\n\nHELP OUTPUT (STDOUT):\n{}\n\nSTDERR:\n{}",
                    result.status, result.stdout, result.stderr
                )
            }
            Err(e) => {
                let error_message = format!("Failed to execute supabase help command: {}", e);
                error!("{}", error_message);
                format!("TOOL EXECUTION ERROR: {}", error_message)
            }
        }
    }
}
