use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::process::Stdio;

use tokio::{fs, sync::Mutex};
use tokio::process::Command;
use futures::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};
use tracing::{debug, info, error};
use schemars::JsonSchema;

// Import SDK components
use rmcp::tool;

#[derive(Clone, Debug)]
pub struct LongRunningTaskManager {
    pub tasks_in_memory: Arc<Mutex<HashMap<String, TaskState>>>,
    pub persistence_path: std::path::PathBuf,
}

/// Each task includes the original command, partial logs, final status, and a reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task_id: String,
    pub command: String,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    /// A new field to store *why* we created this task.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Created,
    Running,
    Ended,
    Error,
}
impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Created
    }
}

impl LongRunningTaskManager {
    pub fn new(filename: String) -> Self {
        let path = dirs::home_dir()
            .expect("Could not find home directory")
            .join(filename);

        debug!("LongRunningTaskManager storing tasks at: {}", path.display());

        Self {
            tasks_in_memory: Arc::new(Mutex::new(HashMap::new())),
            persistence_path: path,
        }
    }

    pub async fn load_persistent_tasks(&self) -> Result<()> {
        if !self.persistence_path.exists() {
            return Ok(());
        }
        let data = fs::read_to_string(&self.persistence_path).await?;
        let tasks: HashMap<String, TaskState> = serde_json::from_str(&data)?;
        let mut guard = self.tasks_in_memory.lock().await;
        guard.extend(tasks);
        Ok(())
    }

    async fn save(&self) -> Result<()> {
        let guard = self.tasks_in_memory.lock().await;
        let json = serde_json::to_string_pretty(&*guard)?;
        fs::write(&self.persistence_path, json).await?;
        Ok(())
    }

    /// Spawns a background task that reads partial stdout/stderr
    pub async fn spawn_task(&self, command: &str, reason: &str) -> Result<String> {
        let task_id = format!("task-{}", uuid::Uuid::new_v4());
        let task_id_clone = task_id.clone();
        let mut state = TaskState {
            task_id: task_id.clone(),
            command: command.to_string(),
            status: TaskStatus::Created,
            stdout: String::new(),
            stderr: String::new(),
            reason: reason.to_string(),
        };

        // Insert initial record in the tasks map
        {
            let mut guard = self.tasks_in_memory.lock().await;
            guard.insert(task_id.clone(), state.clone());
        }

        let manager_clone = self.clone();
        tokio::spawn(async move {
            // Mark as Running
            state.status = TaskStatus::Running;
            {
                let mut guard = manager_clone.tasks_in_memory.lock().await;
                guard.insert(task_id.clone(), state.clone());
            }
            let _ = manager_clone.save().await;

            // Launch the process (removed mut)
            let child = Command::new("bash")
                .arg("-c")
                .arg(&state.command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();

            match child {
                Ok(mut child) => {
                    // read stdout lines
                    if let Some(stdout) = child.stdout.take() {
                        let manager_for_stdout = manager_clone.clone();
                        let task_id_for_stdout = task_id.clone();
                        tokio::spawn(async move {
                            let mut lines = FramedRead::new(stdout, LinesCodec::new());
                            while let Some(item) = lines.next().await {
                                match item {
                                    Ok(line) => {
                                        // Append partial stdout
                                        let mut guard = manager_for_stdout.tasks_in_memory.lock().await;
                                        if let Some(ts) = guard.get_mut(&task_id_for_stdout) {
                                            ts.stdout.push_str(&line);
                                            ts.stdout.push('\n');
                                        }
                                    }
                                    Err(e) => {
                                        let mut guard = manager_for_stdout.tasks_in_memory.lock().await;
                                        if let Some(ts) = guard.get_mut(&task_id_for_stdout) {
                                            ts.stderr.push_str(&format!(
                                                "[reading stdout error]: {}\n",
                                                e
                                            ));
                                        }
                                    }
                                }
                            }
                        });
                    }

                    // read stderr lines
                    if let Some(stderr) = child.stderr.take() {
                        let manager_for_stderr = manager_clone.clone();
                        let task_id_for_stderr = task_id.clone();
                        tokio::spawn(async move {
                            let mut lines = FramedRead::new(stderr, LinesCodec::new());
                            while let Some(item) = lines.next().await {
                                match item {
                                    Ok(line) => {
                                        let mut guard = manager_for_stderr.tasks_in_memory.lock().await;
                                        if let Some(ts) = guard.get_mut(&task_id_for_stderr) {
                                            ts.stderr.push_str(&line);
                                            ts.stderr.push('\n');
                                        }
                                    }
                                    Err(e) => {
                                        let mut guard = manager_for_stderr.tasks_in_memory.lock().await;
                                        if let Some(ts) = guard.get_mut(&task_id_for_stderr) {
                                            ts.stderr.push_str(&format!(
                                                "[reading stderr error]: {}\n",
                                                e
                                            ));
                                        }
                                    }
                                }
                            }
                        });
                    }

                    // Wait on final exit
                    match child.wait().await {
                        Ok(status) => {
                            if status.success() {
                                state.status = TaskStatus::Ended;
                            } else {
                                state.status = TaskStatus::Error;
                            }
                        }
                        Err(e) => {
                            state.stderr.push_str(&format!(
                                "Failed waiting on command: {}\n",
                                e
                            ));
                            state.status = TaskStatus::Error;
                        }
                    }
                }
                Err(e) => {
                    state.stderr = format!("Failed to spawn command '{}': {}", state.command, e);
                    state.status = TaskStatus::Error;
                }
            }

            // Merge partial logs in aggregator with final `state`
            {
                let mut guard = manager_clone.tasks_in_memory.lock().await;
                if let Some(ts) = guard.get(&task_id) {
                    state.stdout = ts.stdout.clone();
                    state.stderr = ts.stderr.clone();
                }
                // Overwrite aggregator with final state
                guard.insert(task_id.clone(), state.clone());
            }
            let _ = manager_clone.save().await;
        });

        Ok(task_id_clone)
    }

    /// Return partial or final logs
    pub async fn get_task_status(&self, task_id: &str) -> Result<TaskState> {
        let guard = self.tasks_in_memory.lock().await;
        let st = guard
            .get(task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;
        Ok(st.clone())
    }

    /// New method to list tasks by optional status filter
    pub async fn list_tasks(&self, filter_status: Option<TaskStatus>) -> Vec<TaskState> {
        let guard = self.tasks_in_memory.lock().await;
        guard
            .values()
            .filter(|task| {
                // If no filter provided, return all
                // If filter provided, return only tasks matching that status
                if let Some(ref wanted) = filter_status {
                    task.status == *wanted
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }
}

/// A helper function to retrieve the last `n` lines from a string.
fn last_n_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() > n {
        lines[lines.len() - n..].join("\n")
    } else {
        s.to_string()
    }
}

// Parameter structs for SDK-based tools
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StartTaskParams {
    #[schemars(description = "The command line string to spawn when starting a new task")]
    pub command_string: String,
    
    #[schemars(description = "A human-friendly reason or rationale for creating this task")]
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetStatusParams {
    #[schemars(description = "The ID of the task to retrieve status for")]
    pub task_id: String,
    
    #[serde(default = "default_lines")]
    #[schemars(description = "How many trailing lines from stdout/stderr to return. Defaults to 100.")]
    pub lines: usize,
}

fn default_lines() -> usize {
    100
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListTasksParams {
    #[serde(default)]
    #[schemars(description = "Optional filter for tasks (created, running, ended, error)")]
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LongRunningTaskTool {
    manager: Arc<Mutex<LongRunningTaskManager>>,
}

impl LongRunningTaskTool {
    pub fn new(manager_path: &str) -> Self {
        let manager = LongRunningTaskManager::new(manager_path.to_string());
        
        Self {
            manager: Arc::new(Mutex::new(manager)),
        }
    }
    
    pub async fn load_persistent_tasks(&self) -> Result<()> {
        let manager = self.manager.lock().await;
        manager.load_persistent_tasks().await
    }
    
    // Helper method to perform start_task operation
    async fn spawn_task_internal(&self, command_string: String, reason: String) -> Result<String> {
        let manager = self.manager.lock().await;
        let task_id = manager.spawn_task(&command_string, &reason).await?;
        
        Ok(task_id)
    }
    
    // Helper method to get task status
    async fn get_status_internal(&self, task_id: &str, lines: usize) -> Result<(TaskState, String, String)> {
        let manager = self.manager.lock().await;
        let state = manager.get_task_status(task_id).await?;
        
        // Get only the last N lines of stdout/stderr
        let stdout_short = last_n_lines(&state.stdout, lines);
        let stderr_short = last_n_lines(&state.stderr, lines);
        
        Ok((state.clone(), stdout_short, stderr_short))
    }
    
    // Helper method to list tasks
    async fn list_tasks_internal(&self, status_filter: Option<String>) -> Vec<TaskState> {
        let manager = self.manager.lock().await;
        
        // Convert status_str => Option<TaskStatus>
        let filter_status = match status_filter.as_deref() {
            Some("created") => Some(TaskStatus::Created),
            Some("running") => Some(TaskStatus::Running),
            Some("ended") => Some(TaskStatus::Ended),
            Some("error") => Some(TaskStatus::Error),
            None => None,        // no filter => all tasks
            _ => None,           // unrecognized => return all
        };
        
        manager.list_tasks(filter_status).await
    }
}

#[tool(tool_box)]
impl LongRunningTaskTool {
    #[tool(description = "Start a new long-running shell task. Use this for any shell command that might take longer than 1 minute to complete. The task runs in the background, continues after this conversation ends, and its status/output can be checked later using 'get_status' or 'list_tasks'.")]
    pub async fn start_task(
        &self,
        #[tool(aggr)] params: StartTaskParams
    ) -> String {
        info!("Starting long-running task: {}", params.command_string);
        
        match self.spawn_task_internal(params.command_string.clone(), params.reason.clone()).await {
            Ok(task_id) => {
                format!("Task started with ID: {}\nReason: {}", task_id, params.reason)
            }
            Err(e) => {
                error!("Failed to start task: {}", e);
                format!("Error starting task: {}", e)
            }
        }
    }
    
    #[tool(description = "Get the status and output of a long-running task. This will show if the task is still running and display its stdout/stderr.")]
    pub async fn get_status(
        &self,
        #[tool(aggr)] params: GetStatusParams
    ) -> String {
        info!("Getting status for task ID: {}", params.task_id);
        
        match self.get_status_internal(&params.task_id, params.lines).await {
            Ok((state, stdout, stderr)) => {
                format!(
                    "Task ID: {}\nStatus: {:?}\nReason: {}\nCommand: {}\n\n=== STDOUT (last {} lines) ===\n{}\n\n=== STDERR (last {} lines) ===\n{}",
                    params.task_id,
                    state.status,
                    state.reason,
                    state.command,
                    params.lines,
                    stdout,
                    params.lines,
                    stderr
                )
            }
            Err(e) => {
                error!("Failed to get task status: {}", e);
                format!("Error getting task status: {}", e)
            }
        }
    }
    
    #[tool(description = "List all tasks or filter by status (created, running, ended, error). Shows a summary of each task without the full output.")]
    pub async fn list_tasks(
        &self,
        #[tool(aggr)] params: ListTasksParams
    ) -> String {
        info!("Listing tasks with filter: {:?}", params.status);
        
        let tasks = self.list_tasks_internal(params.status).await;
        
        if tasks.is_empty() {
            return "No tasks found.".to_string();
        }
        
        let mut result = String::new();
        result.push_str(&format!("Found {} tasks:\n\n", tasks.len()));
        
        for task in tasks {
            result.push_str(&format!(
                "Task ID: {}\nStatus: {:?}\nReason: {}\nCommand: {}\nStdout: {} bytes, Stderr: {} bytes\n\n",
                task.task_id,
                task.status,
                task.reason,
                task.command,
                task.stdout.len(),
                task.stderr.len()
            ));
        }
        
        result
    }
}
