use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use shared_protocol_objects::{error_response, success_response, CallToolParams, CallToolResult, JsonRpcResponse, ToolResponseContent, INTERNAL_ERROR};
use std::process::{Command, Stdio};

#[derive(Debug, Deserialize)]
struct GitParams {
    action: String,
    #[serde(default)]
    repo_path: Option<String>,
    #[serde(default)]
    files: Option<Vec<String>>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    remote: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    max_count: Option<usize>,
}

/// Execute a command in `repo_path` directory, capturing stdout and stderr.
fn run_git_command(repo_path: &str, args: &[&str]) -> Result<(String, String)> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() {
        return Err(anyhow!(format!("Git command failed: {}\nSTDERR: {}", args.join(" "), stderr)));
    }

    Ok((stdout.trim().to_string(), stderr.trim().to_string()))
}

/// Ensure the repo at `repo_path` is initialized.
fn ensure_repo_initialized(repo_path: &str) -> Result<()> {
    if !std::path::Path::new(&format!("{}/.git", repo_path)).exists() {
        run_git_command(repo_path, &["init"])?;
    }
    Ok(())
}

/// Handle calls to the `git` tool.
pub async fn handle_git_tool_call(params: CallToolParams, id: Option<Value>) -> Result<JsonRpcResponse> {
    let git_params: GitParams = serde_json::from_value(params.arguments).map_err(|e| anyhow!(e))?;
    let action = git_params.action.as_str();

    // Default repo path if none provided
    let repo_path = git_params.repo_path.as_deref().unwrap_or("./repo");

    // Ensure repo is initialized for any action except init_repo
    if action != "init_repo" {
        ensure_repo_initialized(repo_path)?;
    }

    let result = match action {
        "init_repo" => {
            ensure_repo_initialized(repo_path)?;
            "Repository initialized.".to_string()
        }
        "add_files" => {
            let files = git_params.files.ok_or_else(|| anyhow!("Missing 'files' parameter"))?;
            let mut args = vec!["add"];
            for f in &files {
                args.push(f);
            }
            run_git_command(repo_path, &args)?;
            "Files added.".to_string()
        }
        "commit_changes" => {
            let msg = git_params.message.as_deref().ok_or_else(|| anyhow!("Missing 'message' parameter"))?;
            run_git_command(repo_path, &["commit", "-m", msg])?;
            "Changes committed.".to_string()
        }
        "undo_last_commit" => {
            run_git_command(repo_path, &["reset", "HEAD~1"])?;
            "Last commit undone (changes are still in working directory).".to_string()
        }
        "get_status" => {
            let (stdout, _) = run_git_command(repo_path, &["status", "--short"])?;
            if stdout.is_empty() {
                "No changes.".to_string()
            } else {
                stdout
            }
        }
        "get_log" => {
            let count = git_params.max_count.unwrap_or(5);
            let (stdout, _) = run_git_command(repo_path, &["log", &format!("--max-count={}", count), "--pretty=format:%H %s"])?;
            if stdout.is_empty() {
                "No commits yet.".to_string()
            } else {
                stdout
            }
        }
        "push_changes" => {
            let remote = git_params.remote.as_deref().unwrap_or("origin");
            let branch = git_params.branch.as_deref().unwrap_or("main");
            run_git_command(repo_path, &["push", remote, branch])?;
            "Changes pushed successfully.".to_string()
        }
        _ => {
            return Ok(error_response(id, INTERNAL_ERROR, &format!("Unknown action '{}'", action)));
        }
    };

    let tool_res = CallToolResult {
        content: vec![ToolResponseContent {
            type_: "text".into(),
            text: result,
            annotations: None,
        }],
        is_error: None,
        _meta: None,
        progress: None,
        total: None,
    };

    Ok(success_response(id, serde_json::to_value(tool_res)?))
}

pub fn git_tool_info() -> shared_protocol_objects::ToolInfo {
    shared_protocol_objects::ToolInfo {
        name: "git".to_string(),
        description: Some(
            "A comprehensive Git version control interface for managing source code and file changes systematically.

            **When to Use:**
            - Managing source code versions and history
            - Tracking changes to configuration files
            - Collaborating on shared codebases
            - Documenting code evolution over time
            - Creating and managing feature branches
            - Preparing code for deployment
            
            **Core Operations:**
            1. Repository Management:
               - init_repo: Create new Git repository
               - get_status: Check current repo state
               - get_log: View commit history
            
            2. Change Management:
               - add_files: Stage specific files
               - commit_changes: Record staged changes
               - undo_last_commit: Revert recent changes
            
            3. Remote Operations:
               - push_changes: Share commits with remote
            
            **Best Practices:**
            1. Repository Setup:
               - Initialize in appropriate directory
               - Verify repository state before operations
               - Maintain clean working directory
            
            2. Change Tracking:
               - Stage related changes together
               - Write clear, descriptive commit messages
               - Review changes before committing
               - Keep commits focused and atomic
            
            3. Collaboration:
               - Pull before pushing changes
               - Resolve conflicts promptly
               - Follow branch naming conventions
               - Keep remote refs updated
            
            **Safety Guidelines:**
            - ALWAYS check status before operations
            - Verify target repository path
            - Review changes before committing
            - Test code before pushing
            - Back up important changes
            - Use appropriate branch strategies
            
            **Integration Workflow:**
            1. Check repository status
            2. Stage relevant changes
            3. Create descriptive commit
            4. Verify commit history
            5. Push to appropriate remote
            
            **Parameters Guide:**
            - repo_path: Target repository location
            - files: Array of paths to stage
            - message: Descriptive commit text
            - remote: Target remote (default: origin)
            - branch: Target branch (default: main)
            - max_count: History limit for logs
           ".to_string()
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["init_repo","add_files","commit_changes","undo_last_commit","get_status","get_log","push_changes"],
                    "description": "The git action to perform."
                },
                "repo_path": {
                    "type": "string",
                    "description": "Path to the git repository directory (default './repo')."
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Files to add (for 'add_files' action)."
                },
                "message": {
                    "type": "string",
                    "description": "Commit message (for 'commit_changes')."
                },
                "remote": {
                    "type": "string",
                    "description": "Remote name for push (default 'origin')."
                },
                "branch": {
                    "type": "string",
                    "description": "Branch name for push (default 'main')."
                },
                "max_count": {
                    "type": "integer",
                    "description": "Number of commits to retrieve for 'get_log' (default: 5)."
                }
            },
            "required": ["action"],
            "additionalProperties": false
        })
    }
}
