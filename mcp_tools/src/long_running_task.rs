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
    /// Store the process ID, skip serialization as it's runtime-specific
    #[serde(skip)]
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Created,
    Running,
    Ended, // Task completed normally
    Error, // Task failed or errored
    Stopped, // Task was manually stopped
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
            pid: None, // Initialize PID as None
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
                    // --- Store the PID ---
                    let process_id = child.id();
                    state.pid = process_id; // Store PID in the local state copy first
                    if let Some(pid_val) = process_id {
                        info!("Task {} started with PID: {}", task_id, pid_val);
                        // Update the PID in the shared map immediately
                        {
                             let mut guard = manager_clone.tasks_in_memory.lock().await;
                             if let Some(ts) = guard.get_mut(&task_id) {
                                 ts.pid = Some(pid_val);
                             }
                        }
                    } else {
                         error!("Task {} spawned but could not get PID.", task_id);
                         // Proceed, but stopping might not work
                    }
                    // --- End Store PID ---

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
                                        } else {
                                            // Task might have been cleared, log and stop reading
                                            warn!("Task {} not found in map while reading stdout. Stopping reader.", task_id_for_stdout);
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Error reading stdout for task {}: {}", task_id_for_stdout, e);
                                        let mut guard = manager_for_stdout.tasks_in_memory.lock().await;
                                        // Attempt to log the error to the task's stderr if it still exists
                                        if let Some(ts) = guard.get_mut(&task_id_for_stdout) {
                                            ts.stderr.push_str(&format!("[Error reading stdout stream: {}]\n", e));
                                        } else {
                                             warn!("Task {} not found in map while handling stdout read error.", task_id_for_stdout);
                                        }
                                        break; // Stop reading on error
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
                                        } else {
                                            // Task might have been cleared, log and stop reading
                                            warn!("Task {} not found in map while reading stderr. Stopping reader.", task_id_for_stderr);
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Error reading stderr for task {}: {}", task_id_for_stderr, e);
                                        let mut guard = manager_for_stderr.tasks_in_memory.lock().await;
                                         // Attempt to log the error to the task's stderr if it still exists
                                        if let Some(ts) = guard.get_mut(&task_id_for_stderr) {
                                            ts.stderr.push_str(&format!("[Error reading stderr stream: {}]\n", e));
                                        } else {
                                             warn!("Task {} not found in map while handling stderr read error.", task_id_for_stderr);
                                        }
                                        break; // Stop reading on error
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

            // Update the final status in the shared map directly.
            // The stdout/stderr have already been updated by the reader tasks.
            // This block is reached only when child.wait() completes.
            info!("Task {} process finished. Updating final status to {:?}.", task_id, state.status);
            {
                let mut guard = manager_clone.tasks_in_memory.lock().await;
                if let Some(ts) = guard.get_mut(&task_id) {
                    // Update only the status field of the existing TaskState
                    ts.status = state.status; // Use the final status determined above (Ended or Error)
                } else {
                    // This case might happen if the task was cleared concurrently.
                    error!("Task {} not found in map for final status update after process exit.", task_id);
                }
            }
            // Save the updated state
            if let Err(e) = manager_clone.save().await {
                 error!("Failed to save final task state for {}: {}", task_id, e);
            }
            info!("Task {} monitoring task finished.", task_id);
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
    #[schemars(description = "Optional filter for tasks (created, running, ended, error, stopped)")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StopTaskParams {
    #[schemars(description = "The ID of the running task to stop")]
    pub task_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClearTasksParams {
    // No parameters needed for this action
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
            Some("stopped") => Some(TaskStatus::Stopped),
            None => None,        // no filter => all tasks
            _ => None,           // unrecognized => return all (or maybe error?)
        };

        manager.list_tasks(filter_status).await
    }

    // Helper method to stop a task
    async fn stop_task_internal(&self, task_id: &str) -> Result<String> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let manager = self.manager.lock().await;
        let mut tasks_guard = manager.tasks_in_memory.lock().await;

        match tasks_guard.get_mut(task_id) {
            Some(task) => {
                if task.status != TaskStatus::Running {
                    return Ok(format!("Task {} is not currently running (status: {:?}). Cannot stop.", task_id, task.status));
                }

                if let Some(pid_val) = task.pid {
                    info!("Attempting to stop task {} (PID: {})", task_id, pid_val);
                    let pid = Pid::from_raw(pid_val as i32);
                    match kill(pid, Signal::SIGTERM) { // Send SIGTERM first for graceful shutdown
                        Ok(_) => {
                            task.status = TaskStatus::Stopped;
                            task.stderr.push_str("\n[Task manually stopped via SIGTERM]");
                            info!("Sent SIGTERM to task {} (PID: {})", task_id, pid_val);
                            // Drop the lock before saving
                            drop(tasks_guard);
                            let _ = manager.save().await;
                            Ok(format!("Stop signal (SIGTERM) sent to task {}. Status set to Stopped.", task_id))
                        }
                        Err(e) => {
                            error!("Failed to send SIGTERM to task {} (PID: {}): {}. Attempting SIGKILL.", task_id, pid_val, e);
                            // If SIGTERM fails (e.g., process doesn't exist anymore), try SIGKILL
                            match kill(pid, Signal::SIGKILL) {
                                Ok(_) => {
                                    task.status = TaskStatus::Stopped;
                                    task.stderr.push_str("\n[Task manually stopped via SIGKILL]");
                                    info!("Sent SIGKILL to task {} (PID: {})", task_id, pid_val);
                                    // Drop the lock before saving
                                    drop(tasks_guard);
                                    let _ = manager.save().await;
                                    Ok(format!("Stop signal (SIGKILL) sent to task {}. Status set to Stopped.", task_id))
                                }
                                Err(e2) => {
                                     error!("Failed to send SIGKILL to task {} (PID: {}): {}", task_id, pid_val, e2);
                                     // Update status anyway? Maybe Error? Or leave as Running but log failure?
                                     // Let's mark as error if we couldn't kill it.
                                     task.status = TaskStatus::Error;
                                     task.stderr.push_str(&format!("\n[Failed to stop task: {}]", e2));
                                     drop(tasks_guard);
                                     let _ = manager.save().await;
                                     Err(anyhow!("Failed to send SIGTERM or SIGKILL to process {}: {}", pid_val, e2))
                                }
                            }
                        }
                    }
                } else {
                    // Task is running but PID is missing - this shouldn't happen with the new code
                    error!("Task {} is running but has no PID stored. Cannot stop.", task_id);
                    task.status = TaskStatus::Error; // Mark as error if we can't control it
                    task.stderr.push_str("\n[Error: Cannot stop task - PID missing]");
                     drop(tasks_guard);
                     let _ = manager.save().await;
                    Err(anyhow!("Task {} is running but PID is missing. Cannot stop.", task_id))
                }
            }
            None => Err(anyhow!("Task not found: {}", task_id)),
        }
    }

    // Helper method to clear all tasks
    async fn clear_tasks_internal(&self) -> Result<String> {
        let manager = self.manager.lock().await;
        let task_ids: Vec<String> = { // Collect task IDs to avoid borrowing issues while iterating and modifying
            let tasks_guard = manager.tasks_in_memory.lock().await;
            tasks_guard.keys().cloned().collect()
        };

        let mut stopped_count = 0;
        let mut failed_to_stop = Vec::new();
        let total_tasks = task_ids.len();

        info!("Attempting to clear {} tasks.", total_tasks);

        // Drop the manager lock before calling stop_task_internal which acquires it
        drop(manager);

        for task_id in &task_ids {
            // Check if task is running before attempting to stop
            let is_running = {
                let mgr = self.manager.lock().await;
                let tasks = mgr.tasks_in_memory.lock().await;
                tasks.get(task_id).map_or(false, |t| t.status == TaskStatus::Running)
            };

            if is_running {
                match self.stop_task_internal(task_id).await {
                    Ok(_) => {
                        stopped_count += 1;
                    }
                    Err(e) => {
                        error!("Failed to stop task {} during clear: {}", task_id, e);
                        failed_to_stop.push(task_id.clone());
                        // Continue trying to clear other tasks
                    }
                }
            }
        }

        // Re-acquire lock to clear the map and save
        let manager = self.manager.lock().await;
        {
            let mut tasks_guard = manager.tasks_in_memory.lock().await;
            tasks_guard.clear();
            info!("Cleared tasks map in memory.");
        } // Drop lock before saving

        match manager.save().await {
            Ok(_) => info!("Saved empty task state to persistence."),
            Err(e) => {
                error!("Failed to save cleared task state: {}", e);
                // Return error, but tasks are cleared from memory at least
                return Err(anyhow!("Failed to save cleared task state: {}", e));
            }
        }

        let mut result_message = format!("Cleared {} tasks from memory and persistence.", total_tasks);
        if stopped_count > 0 {
            result_message.push_str(&format!(" Stopped {} running tasks.", stopped_count));
        }
        if !failed_to_stop.is_empty() {
            result_message.push_str(&format!(" Failed to stop tasks: {:?}", failed_to_stop));
        }

        Ok(result_message)
    }
}

#[tool(tool_box)]
impl LongRunningTaskTool {
    #[tool(description = "Start a new long-running shell task. Use this for any shell command that might take longer than 1 minute to complete, or for tasks that need to run in the background while other tools are used. The task runs asynchronously, continues after this conversation ends, and its status/output can be checked later using 'get_status' or 'list_tasks'.")]
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

    #[tool(description = "Stop a currently running background task. This attempts to gracefully terminate the process using SIGTERM, falling back to SIGKILL if necessary. Use this to cancel tasks that are no longer needed or are running indefinitely.")]
    pub async fn stop_task(
        &self,
        #[tool(aggr)] params: StopTaskParams
    ) -> String {
        info!("Attempting to stop task ID: {}", params.task_id);

        match self.stop_task_internal(&params.task_id).await {
            Ok(message) => {
                info!("Stop task result for {}: {}", params.task_id, message);
                message
            }
            Err(e) => {
                error!("Failed to stop task {}: {}", params.task_id, e);
                format!("Error stopping task {}: {}", params.task_id, e)
            }
        }
    }

    #[tool(description = "Stops all currently running tasks and removes ALL tasks (running, completed, errored, etc.) from the manager's memory and persistence file. Use with caution, as this permanently deletes task history.")]
    pub async fn clear_tasks(
        &self,
        #[tool(aggr)] _params: ClearTasksParams // Params struct is empty but required by macro
    ) -> String {
        info!("Attempting to clear all tasks.");

        match self.clear_tasks_internal().await {
            Ok(message) => {
                info!("Clear tasks result: {}", message);
                message
            }
            Err(e) => {
                error!("Failed to clear tasks: {}", e);
                format!("Error clearing tasks: {}", e)
            }
        }
    }
}
