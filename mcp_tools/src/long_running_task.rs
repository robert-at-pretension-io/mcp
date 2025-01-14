use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::process::Stdio;  // <-- use std::process::Stdio

use tokio::{fs, sync::Mutex};
use tokio::process::Command;  // <-- from tokio
use futures::StreamExt;        // for lines.next()
use tokio_util::codec::{FramedRead, LinesCodec};  // for line-by-line reading
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
    pub async fn spawn_task(&self, command: &str) -> Result<String> {
        let task_id = format!("task-{}", uuid::Uuid::new_v4());
        let task_id_clone = task_id.clone();
        let mut state = TaskState {
            task_id: task_id.clone(),
            command: command.to_string(),
            status: TaskStatus::Created,
            stdout: String::new(),
            stderr: String::new(),
        };

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

            // Use std::process::Stdio (imported above)
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

                    // Wait on the child's final exit
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

            // final update
            {
                let mut guard = manager_clone.tasks_in_memory.lock().await;
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
}

pub fn long_running_tool_info() -> ToolInfo {
    ToolInfo {
        name: "long_running_tool".to_string(),
        description: Some(
            r#"
Run long-running bash commands asynchronously, with partial output streaming.
Commands:
- start_task
- get_status
"#
            .to_string(),
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "enum": ["start_task", "get_status"] },
                "commandString": { "type": "string" },
                "taskId": { "type": "string" }
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

            let task_id = manager.spawn_task(command_string).await?;

            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: format!("Task started with id: {}", task_id),
                    annotations: Some(HashMap::from([("task_id".to_string(), json!(task_id))])),
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
                        "Task ID: {}\nStatus: {:?}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        task_id, state.status, state.stdout, state.stderr
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
        _ => {
            let msg = format!("Invalid command '{}'. Use start_task or get_status", command);
            Ok(error_response(id, INVALID_PARAMS, &msg))
        }
    }
}
