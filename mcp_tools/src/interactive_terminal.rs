use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use schemars::JsonSchema;
use uuid::Uuid;
// Updated imports for pty-process 0.5.1 with async feature
use pty_process::{Command as PtyCommand, Pty, open}; // Import open, keep Pty
// Removed explicit PtyMaster import
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use nix::errno::Errno; // Import Errno for EIO comparison


use rmcp::tool;

type SessionId = String;

// --- Parameter Structs ---

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StartTerminalParams {
    #[serde(default = "default_shell")]
    #[schemars(description = "Optional: Path to the shell executable (e.g., /bin/bash, /bin/zsh). Defaults to /bin/bash.")]
    pub shell: String,
    // Potentially add initial commands, working directory later
}

fn default_shell() -> String {
    "/bin/bash".to_string()
}


#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunInTerminalParams {
    #[schemars(description = "The ID of the active terminal session")]
    pub session_id: SessionId,
    #[schemars(description = "The command to execute in the terminal session")]
    pub command: String,
    #[serde(default = "default_timeout_ms")]
    #[schemars(description = "Timeout in milliseconds to wait for command completion marker")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    15000 // 15 seconds default
}


#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetOutputParams {
    #[schemars(description = "The ID of the active terminal session")]
    pub session_id: SessionId,
    #[serde(default)]
    #[schemars(description = "Optional: Number of trailing lines to retrieve from the buffer")]
    pub lines: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StopTerminalParams {
    #[schemars(description = "The ID of the terminal session to stop")]
    pub session_id: SessionId,
}

// --- Internal State ---

#[derive(Debug, Clone, PartialEq)]
enum SessionStatus {
    Starting,
    Running,
    Stopped,
    Error,
}

// Represents an active terminal session state
// #[derive(Debug)] // Cannot derive Debug because pty_process::Pty doesn't implement it
struct TerminalSessionState {
    pty_master: Arc<Mutex<Pty>>, // Use pty_process::Pty for the master handle
    output_buffer: Arc<Mutex<String>>,
    status: Arc<Mutex<SessionStatus>>,
    reader_handle: JoinHandle<()>,
    process_pid: Option<i32>, // Store the PID for potential killing
    shell_path: String,
}

// Manual Debug implementation, skipping pty_master
impl std::fmt::Debug for TerminalSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalSessionState")
            // Skip pty_master as it doesn't implement Debug
            // .field("pty_master", &self.pty_master)
            .field("output_buffer", &self.output_buffer)
            .field("status", &self.status)
            .field("reader_handle", &self.reader_handle)
            .field("process_pid", &self.process_pid)
            .field("shell_path", &self.shell_path)
            .finish_non_exhaustive() // Use non_exhaustive if skipping fields
    }
}


// --- Tool Implementation ---

#[derive(Debug, Clone)]
pub struct InteractiveTerminalTool {
    sessions: Arc<Mutex<HashMap<SessionId, Arc<TerminalSessionState>>>>,
}

impl InteractiveTerminalTool {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // --- Internal Helper Methods ---

    async fn start_session_internal(&self, shell_path: &str) -> Result<SessionId> {
        info!("Starting new interactive terminal session with shell: {}", shell_path);

        // Use pty_process::open() which returns (Pty, Pts)
        let (pty, pts) = open()?; // Use the imported open function

        // Configure the command to run in the PTY
        let cmd = PtyCommand::new(shell_path);
        // cmd.pty(pts); // This method is removed

        // Spawn the command, passing pts directly to spawn
        let child = cmd.spawn(pts)?;
        // Use child.id() to get the PID (returns Option<u32>)
        let pid = child.id();
        info!("Spawned shell process with PID: {:?}", pid);

        // The 'pty' variable from open() is the master handle we need
        // No need for into_async_master()

        let session_id = Uuid::new_v4().to_string();
        let output_buffer = Arc::new(Mutex::new(String::new()));
        let status = Arc::new(Mutex::new(SessionStatus::Starting)); // Start in Starting state
        // Create Arc<Mutex<Pty>> directly
        let pty_master_arc: Arc<Mutex<Pty>> = Arc::new(Mutex::new(pty));

        // Spawn reader task
        let reader_buffer_clone = Arc::clone(&output_buffer);
        let reader_status_clone = Arc::clone(&status);
        // Add explicit type annotation for the Arc clone
        let reader_master_clone: Arc<Mutex<Pty>> = Arc::clone(&pty_master_arc);
        let session_id_clone = session_id.clone();

        let reader_handle = tokio::spawn(async move {
            let mut buf = [0; 2048]; // Larger buffer might be slightly more efficient
            loop {
                // Declare master_guard as mutable
                let mut master_guard = reader_master_clone.lock().await;
                // Use select! to handle both reading and status changes? Or just read?
                // Let's stick to simple read for now.
                let read_result = master_guard.read(&mut buf).await;
                drop(master_guard); // Release lock before processing

                match read_result {
                    Ok(0) => {
                        info!("PTY master EOF reached for session {}", session_id_clone);
                        let mut status_guard = reader_status_clone.lock().await;
                        // Only transition from Running to Stopped on EOF
                        if *status_guard == SessionStatus::Running || *status_guard == SessionStatus::Starting {
                            *status_guard = SessionStatus::Stopped;
                        }
                        break;
                    }
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]);
                        debug!("Session {}: Read {} bytes", session_id_clone, n);
                        // Append to buffer
                        let mut buffer_guard = reader_buffer_clone.lock().await;
                        buffer_guard.push_str(&data);
                        // Optional: Limit buffer size
                        const MAX_BUFFER_SIZE: usize = 1_000_000; // 1MB limit
                        if buffer_guard.len() > MAX_BUFFER_SIZE {
                            let start_index = buffer_guard.len() - MAX_BUFFER_SIZE;
                            *buffer_guard = buffer_guard[start_index..].to_string();
                            warn!("Session {}: Output buffer truncated to {} bytes", session_id_clone, MAX_BUFFER_SIZE);
                        }

                        // Check if we should transition from Starting to Running
                        // Heuristic: Look for a common shell prompt pattern (e.g., $, #, >)
                        // This is imperfect but helps signal readiness.
                        let mut status_guard = reader_status_clone.lock().await;
                        if *status_guard == SessionStatus::Starting {
                             // Check last few chars for prompt-like characters
                             let buffer_len = buffer_guard.len();
                             let tail = &buffer_guard[buffer_len.saturating_sub(10)..]; // Check last 10 chars
                             if tail.contains('$') || tail.contains('#') || tail.contains('>') {
                                 info!("Session {}: Detected likely prompt, transitioning to Running state.", session_id_clone);
                                 *status_guard = SessionStatus::Running;
                             }
                        }
                    }
                    Err(e) => {
                        // Check if the error is expected on close (e.g., EIO)
                        // Assuming 'e' is already std::io::Error as returned by AsyncRead::read
                        // Use nix::errno::Errno::EIO for comparison
                        if e.kind() == std::io::ErrorKind::Other && e.raw_os_error() == Some(Errno::EIO as i32) {
                            info!("Session {}: PTY master closed (EIO), likely due to session stop.", session_id_clone);
                        } else {
                            error!("Error reading from PTY master for session {}: {} (Kind: {:?}, OS Error: {:?})", session_id_clone, e, e.kind(), e.raw_os_error());
                        }
                        // Removed incorrect 'else' block here, as 'e' is guaranteed to be std::io::Error

                        let mut status_guard = reader_status_clone.lock().await;
                        // Only transition to Error if not already Stopped
                        if *status_guard != SessionStatus::Stopped {
                            *status_guard = SessionStatus::Error;
                        }
                        break;
                    }
                }
            }
            info!("PTY reader task finished for session {}", session_id_clone);
        });

        let session_state = Arc::new(TerminalSessionState {
            pty_master: Arc::clone(&pty_master_arc),
            output_buffer: Arc::clone(&output_buffer),
            status: Arc::clone(&status),
            reader_handle,
            process_pid: pid.map(|id| id as i32), // Convert Option<u32> to Option<i32>
            shell_path: shell_path.to_string(),
        });

        let mut sessions_guard = self.sessions.lock().await;
        sessions_guard.insert(session_id.clone(), session_state);
        info!("Interactive terminal session {} started.", session_id);
        Ok(session_id)
    }

    async fn run_command_internal(&self, session_id: &str, command: &str, _timeout_ms: u64) -> Result<String> { // Prefix unused timeout_ms
        let session_state = {
            let sessions_guard = self.sessions.lock().await;
            sessions_guard.get(session_id).cloned() // Clone the Arc<SessionState>
        };

        let state = match session_state {
            Some(s) => s,
            None => return Err(anyhow!("Session not found: {}", session_id)),
        };

        // Check status - allow running commands even if 'Starting'
        // Clone the status after locking to avoid moving out of the guard
        let current_status = state.status.lock().await.clone();
        if current_status == SessionStatus::Stopped || current_status == SessionStatus::Error {
             return Err(anyhow!("Session {} is not running (status: {:?}).", session_id, current_status));
        }
        if current_status == SessionStatus::Starting {
            warn!("Session {} is still starting, command execution might be delayed.", session_id);
        }

        // Ensure the command ends with a newline for shell execution
        let command_with_newline = if command.ends_with('\n') {
            command.to_string()
        } else {
            format!("{}\n", command)
        };

        // Clone necessary data for the spawned task
        let pty_master_arc = Arc::clone(&state.pty_master);
        let session_id_clone = session_id.to_string();
        let command_log_clone = command.trim().to_string(); // Clone for logging within task

        // Spawn the write operation into a separate task
        tokio::spawn(async move {
            match pty_master_arc.lock().await.write_all(command_with_newline.as_bytes()).await {
                Ok(_) => {
                    debug!("Session {}: Successfully sent command: {}", session_id_clone, command_log_clone);
                    // Implicit flush might happen when the lock guard is dropped here.
                }
                Err(e) => {
                    // Log the error if writing fails in the background task
                    error!("Session {}: Failed to write command '{}': {}", session_id_clone, command_log_clone, e);
                    // Consider updating task status to Error here if possible/needed
                }
            }
        });

        // Return immediately, indicating the command was dispatched asynchronously
        Ok(format!("Command sent asynchronously to session {}. Use 'get_terminal_output' to see results later.", session_id))
    }

     async fn get_output_internal(&self, session_id: &str, lines: Option<usize>) -> Result<String> {
         let session_state = {
             let sessions_guard = self.sessions.lock().await;
             sessions_guard.get(session_id).cloned()
         };

         match session_state {
             Some(state) => {
                 let buffer_guard = state.output_buffer.lock().await;
                 if let Some(n) = lines {
                     // Get last n lines
                     Ok(buffer_guard.lines().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"))
                 } else {
                     Ok(buffer_guard.clone())
                 }
             }
             None => Err(anyhow!("Session not found: {}", session_id)),
         }
     }

     async fn stop_session_internal(&self, session_id: &str) -> Result<String> {
         let session_state = {
             let mut sessions_guard = self.sessions.lock().await;
             sessions_guard.remove(session_id) // Remove from map first
         };

         match session_state {
             Some(state) => {
                 info!("Stopping session {}", session_id);
                 let mut status_guard = state.status.lock().await;
                 if *status_guard == SessionStatus::Stopped {
                     return Ok(format!("Session {} already stopped.", session_id));
                 }
                 *status_guard = SessionStatus::Stopped; // Mark as stopped immediately
                 drop(status_guard); // Release status lock

                 // Close the PTY master handle. This should signal EOF to the reader task
                 // and potentially cause the shell process to exit (or receive SIGHUP).
                 info!("Session {}: Dropping PTY master handle (should close FD).", session_id);
                 let master_guard = state.pty_master.lock().await;
                 // master_guard.close()?; // Pty doesn't have close, dropping should handle it
                 drop(master_guard); // Explicitly drop the guard to release the lock and the Pty

                 // Wait a short time for the reader task to finish due to EOF
                 tokio::time::sleep(Duration::from_millis(100)).await;

                 // Abort the reader task if it hasn't finished (it should have due to EOF/close)
                 if !state.reader_handle.is_finished() {
                     warn!("Session {}: Reader task still running after PTY close, aborting.", session_id);
                     state.reader_handle.abort();
                 }

                 // Attempt to kill the process group if PID is known, just in case closing PTY wasn't enough
                 if let Some(pid_val) = state.process_pid {
                     let pid = Pid::from_raw(pid_val); // Create Pid before moving into closure
                     info!("Session {}: Attempting to stop process group {} (PID: {})", session_id, pid_val, pid_val);

                     // --- Try SIGTERM first using spawn_blocking ---
                     let term_pid = pid; // Clone Pid for the first closure
                     let term_result = tokio::task::spawn_blocking(move || {
                         kill(term_pid, Signal::SIGTERM)
                     }).await;

                     match term_result {
                         Ok(Ok(_)) => {
                             info!("Session {}: Sent SIGTERM to process {}.", session_id, pid_val);
                             // Wait a very short moment to see if it exits gracefully
                             tokio::time::sleep(Duration::from_millis(50)).await;
                             // Check if process still exists (optional, kill might fail harmlessly if it's gone)
                         }
                         Ok(Err(e)) => {
                             warn!("Session {}: Failed to send SIGTERM to process {}: {} (may already be stopped or permissions issue)", session_id, pid_val, e);
                         }
                         Err(e) => {
                             error!("Session {}: Spawn_blocking failed for SIGTERM on process {}: {}", session_id, pid_val, e); // JoinError
                         }
                     }

                     // --- Always attempt SIGKILL using spawn_blocking as a fallback/ensure termination ---
                     // This handles cases where SIGTERM failed, was ignored, or we just want to be sure.
                     info!("Session {}: Attempting SIGKILL for process {} (PID: {})", session_id, pid_val, pid_val);
                     let kill_pid = pid; // Clone Pid for the second closure
                     let kill_result = tokio::task::spawn_blocking(move || {
                         kill(kill_pid, Signal::SIGKILL)
                     }).await;

                     match kill_result {
                         Ok(Ok(_)) => info!("Session {}: Sent SIGKILL to process {}.", session_id, pid_val),
                         Ok(Err(nix::Error::Sys(errno)) ) if errno == nix::errno::Errno::ESRCH => {
                             // ESRCH means "No such process", which is fine if SIGTERM worked or it exited already
                             info!("Session {}: SIGKILL unnecessary for process {} (already exited).", session_id, pid_val);
                         }
                         Ok(Err(e)) => warn!("Session {}: Failed to send SIGKILL to process {}: {}", session_id, pid_val, e),
                         Err(e) => error!("Session {}: Spawn_blocking failed for SIGKILL on process {}: {}", session_id, pid_val, e), // JoinError
                     }

                 } else {
                     warn!("Session {}: No PID recorded, cannot explicitly kill process.", session_id);
                 }

                 info!("Session {} stopped.", session_id);
                 Ok(format!("Session {} stopped.", session_id))
             }
             None => Err(anyhow!("Session not found: {}", session_id)),
         }
     }
}


// --- Tool Trait Implementation ---

#[tool(tool_box)]
impl InteractiveTerminalTool {
    #[tool(description = "Starts a new persistent interactive terminal session (e.g., bash). Returns a unique session ID.")]
    pub async fn start_terminal_session(
        &self,
        #[tool(aggr)] params: StartTerminalParams,
    ) -> String {
        match self.start_session_internal(&params.shell).await {
            Ok(session_id) => format!("Terminal session started with ID: {}", session_id),
            Err(e) => {
                error!("Failed to start terminal session: {}", e);
                format!("Error starting session: {}", e)
            }
        }
    }

    #[tool(description = "Sends a command to run asynchronously within an active terminal session. Returns immediately. Use 'get_terminal_output' to view the command's output later.")]
    pub async fn run_in_terminal(
        &self,
        #[tool(aggr)] params: RunInTerminalParams,
    ) -> String {
        info!("Running command in session {}: {}", params.session_id, params.command);
        match self.run_command_internal(&params.session_id, &params.command, params.timeout_ms).await {
            Ok(output) => output,
            Err(e) => {
                error!("Error running command in session {}: {}", params.session_id, e);
                format!("Error running command: {}", e)
            }
        }
    }

    #[tool(description = "Retrieves the accumulated output buffer for a terminal session. Optionally returns only the last N lines.")]
     pub async fn get_terminal_output(
         &self,
         #[tool(aggr)] params: GetOutputParams,
     ) -> String {
         match self.get_output_internal(&params.session_id, params.lines).await {
             Ok(output) => output,
             Err(e) => {
                 error!("Error getting output for session {}: {}", params.session_id, e);
                 format!("Error getting output: {}", e)
             }
         }
     }

    #[tool(description = "Stops an active terminal session and cleans up its resources.")]
    pub async fn stop_terminal_session(
        &self,
        #[tool(aggr)] params: StopTerminalParams,
    ) -> String {
         match self.stop_session_internal(&params.session_id).await {
             Ok(msg) => msg,
             Err(e) => {
                 error!("Error stopping session {}: {}", params.session_id, e);
                 format!("Error stopping session: {}", e)
             }
         }
    }
}
