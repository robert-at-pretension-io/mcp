use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::process::Stdio;

use tokio::{fs, sync::Mutex};
use tokio::process::Command;
use futures::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};
use tracing::debug;

use shared_protocol_objects::{
    error_response, success_response,
    CallToolParams, CallToolResult, JsonRpcResponse,
    ToolInfo, ToolResponseContent, INVALID_PARAMS
};

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

            // Launch the process
            let mut child = Command::new("bash")
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

pub fn long_running_tool_info() -> ToolInfo {
    ToolInfo {
        name: "long_running_tool".to_string(),
        description: Some(
            "Executes long-running bash commands with real-time output streaming. Use this for:
            
            1. Running background processes
            2. Monitoring command progress
            3. Capturing partial output
            4. Managing multiple concurrent tasks
            
            Key features:
            - Real-time stdout/stderr streaming
            - Persistent task tracking
            - Status monitoring (running/ended/error)
            - Task listing with filters
            
            Example commands:
            - Start: {'command':'start_task','commandString':'long_running.sh','reason':'Processing data'}
            - Check: {'command':'get_status','taskId':'task-123'}
            - List: {'command':'list_tasks','status':'running'}".to_string(),
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "enum": ["start_task", "get_status", "list_tasks"] },
                "commandString": { "type": "string" },
                "taskId": { "type": "string" },
                "reason": { "type": "string" },
                "status": { "type": "string" }  // e.g. "running", "ended", "error"
            },
            "required": ["command"]
        }),
    }
}

pub async fn handle_long_running_tool_call(
    params: CallToolParams,
    manager: &LongRunningTaskManager,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    let command = params.arguments
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Missing 'command' field"))?;

    match command {
        "start_task" => {
            let command_string = params.arguments
                .get("commandString")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Missing 'commandString'"))?;

            let reason = params.arguments
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("No reason given");

            let task_id = manager.spawn_task(command_string, reason).await?;

            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: format!(
                        "Task started with id: {}\nReason: {}",
                        task_id, reason
                    ),
                    annotations: Some(HashMap::from([
                        ("task_id".to_string(), json!(task_id)),
                        ("reason".to_string(), json!(reason))
                    ])),
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None,
            };
            Ok(success_response(id, serde_json::to_value(tool_res)?))
        }
        "get_status" => {
            let task_id = params.arguments
                .get("taskId")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Missing 'taskId'"))?;

            let state = manager.get_task_status(task_id).await?;

            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: format!(
                        "Task ID: {}\nStatus: {:?}\nReason: {}\nCommand: {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        task_id, state.status, state.reason, state.command, state.stdout, state.stderr
                    ),
                    annotations: None,
                }],
                is_error: Some(state.status == TaskStatus::Error),
                _meta: None,
                progress: None,
                total: None,
            };
            Ok(success_response(id, serde_json::to_value(tool_res)?))
        }
        "list_tasks" => {
            let status_str = params.arguments
                .get("status")
                .and_then(Value::as_str);

            // Convert status_str => Option<TaskStatus>
            let filter_status = match status_str {
                Some("created") => Some(TaskStatus::Created),
                Some("running") => Some(TaskStatus::Running),
                Some("ended") => Some(TaskStatus::Ended),
                Some("error") => Some(TaskStatus::Error),
                None => None,        // no filter => all tasks
                _ => None,           // unrecognized => return all or error
            };

            let tasks = manager.list_tasks(filter_status).await;

            let tasks_json: Vec<Value> = tasks.iter().map(|t| {
                json!({
                    "taskId": t.task_id,
                    "status": t.status,
                    "reason": t.reason,
                    "command": t.command,
                    "stdoutLen": t.stdout.len(),
                    "stderrLen": t.stderr.len()
                })
            }).collect();

            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".to_string(),
                    text: serde_json::to_string_pretty(&tasks_json)
                        .unwrap_or("[]".to_string()),
                    annotations: None,
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None,
            };
            Ok(success_response(id, serde_json::to_value(tool_res)?))
        }
        _ => {
            let msg = format!("Invalid command '{}'. Use start_task, get_status, or list_tasks", command);
            Ok(error_response(id, INVALID_PARAMS, &msg))
        }
    }
}
