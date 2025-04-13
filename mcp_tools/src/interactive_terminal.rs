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
use pty_process::{Command as PtyCommand, Pty, PtyMaster}; // Removed WaitStatus
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
#[derive(Debug)] // Add Debug derive
struct TerminalSessionState {
    pty_master: Arc<Mutex<PtyMaster>>, // pty-process master handle
    output_buffer: Arc<Mutex<String>>,
    status: Arc<Mutex<SessionStatus>>,
    reader_handle: JoinHandle<()>,
    process_pid: Option<i32>, // Store the PID for potential killing
    shell_path: String,
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

        let pty = Pty::new()?;
        let pts = pty.pts()?; // Get slave PTY device

        // Configure the command to run in the PTY
        let mut cmd = PtyCommand::new(shell_path);
        cmd.pty(pts); // Assign the PTY slave

        // Spawn the command
        let child = cmd.spawn()?;
        let pid = child.pid(); // Get PID if possible
        info!("Spawned shell process with PID: {:?}", pid);

        let master = pty.master()?; // Get the master handle

        let session_id = Uuid::new_v4().to_string();
        let output_buffer = Arc::new(Mutex::new(String::new()));
        let status = Arc::new(Mutex::new(SessionStatus::Starting)); // Start in Starting state
        let pty_master_arc = Arc::new(Mutex::new(master));

        // Spawn reader task
        let reader_buffer_clone = Arc::clone(&output_buffer);
        let reader_status_clone = Arc::clone(&status);
        // Add explicit type annotation here
        let reader_master_clone: Arc<Mutex<PtyMaster>> = Arc::clone(&pty_master_arc);
        let session_id_clone = session_id.clone();

        let reader_handle = tokio::spawn(async move {
            let mut buf = [0; 2048]; // Larger buffer might be slightly more efficient
            loop {
                let master_guard = reader_master_clone.lock().await;
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
                        if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                            // Use nix::errno::Errno::EIO for comparison
                            if io_err.kind() == std::io::ErrorKind::Other && io_err.raw_os_error() == Some(Errno::EIO as i32) {
                                info!("Session {}: PTY master closed (EIO), likely due to session stop.", session_id_clone);
                            } else {
                                error!("Error reading from PTY master for session {}: {} (Kind: {:?}, OS Error: {:?})", session_id_clone, e, io_err.kind(), io_err.raw_os_error());
                            }
                        } else {
                             error!("Non-IO error reading from PTY master for session {}: {}", session_id_clone, e);
                        }

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
            process_pid: pid,
            shell_path: shell_path.to_string(),
        });

        let mut sessions_guard = self.sessions.lock().await;
        sessions_guard.insert(session_id.clone(), session_state);
        info!("Interactive terminal session {} started.", session_id);
        Ok(session_id)
    }

    async fn run_command_internal(&self, session_id: &str, command: &str, timeout_ms: u64) -> Result<String> {
        let session_state = {
            let sessions_guard = self.sessions.lock().await;
            sessions_guard.get(session_id).cloned() // Clone the Arc<SessionState>
        };

        let state = match session_state {
            Some(s) => s,
            None => return Err(anyhow!("Session not found: {}", session_id)),
        };

        // Check status - allow running commands even if 'Starting'
        let current_status = *state.status.lock().await;
        if current_status == SessionStatus::Stopped || current_status == SessionStatus::Error {
             return Err(anyhow!("Session {} is not running (status: {:?}).", session_id, current_status));
        }
        if current_status == SessionStatus::Starting {
            warn!("Session {} is still starting, command execution might be delayed or capture initial prompt.", session_id);
        }

        let start_marker = format!("MCP_START_MARKER_{}", Uuid::new_v4());
        let end_marker = format!("MCP_END_MARKER_{}", Uuid::new_v4());
        // Use a simpler marker strategy: just echo start/end markers around the command's *expected* output.
        // We rely on reading until the *end* marker appears after the command is sent.

        // Write command and markers
        // Ensure the command itself ends with a newline
        let command_with_newline = if command.ends_with('\n') { command.to_string() } else { format!("{}\n", command) };
        let full_command_to_send = format!(
            "echo '{}' && {} && echo '{}'\n", // Chain with && to ensure markers echo only on success? Or just sequence? Let's sequence.
            start_marker,
            command_with_newline.trim_end(), // The command itself
            end_marker
            // Adding a final newline might be needed depending on shell
        );
        // Alternative: Separate echoes
        // let full_command_to_send = format!(
        //     "echo '{}'\n{}\necho '{}'\n",
        //     start_marker,
        //     command_with_newline,
        //     end_marker
        // );


        // Record buffer state *before* writing
        let (initial_buffer_len, initial_buffer_snapshot) = {
            let buffer_guard = state.output_buffer.lock().await;
            (buffer_guard.len(), buffer_guard.clone())
        };


        { // Scope for writer lock
            let mut writer_guard = state.pty_master.lock().await;
            writer_guard.write_all(full_command_to_send.as_bytes()).await?;
            // No flush needed for PtyMaster according to pty-process docs? Let's assume not for now.
            // writer_guard.flush().await?;
            debug!("Session {}: Sent command and markers.", session_id);
        }


        // Wait for the end marker in the output buffer
        let timeout_duration = Duration::from_millis(timeout_ms);
        let start_time = std::time::Instant::now();

        loop {
            // Check elapsed time
            if start_time.elapsed() > timeout_duration {
                warn!("Session {}: Timeout waiting for end marker '{}'", session_id, end_marker);
                // Return buffer content *after* the initial snapshot
                let current_buffer = state.output_buffer.lock().await;
                let output_slice = if current_buffer.len() > initial_buffer_len {
                    &current_buffer[initial_buffer_len..]
                } else {
                    "" // Buffer hasn't grown or somehow shrunk?
                };
                return Ok(format!("TIMEOUT\nOutput since command sent:\n{}", output_slice.trim_start()));
            }

            // Check buffer for end marker *after* the initial length
            let current_buffer = state.output_buffer.lock().await;
            if current_buffer.len() > initial_buffer_len {
                let search_area = &current_buffer[initial_buffer_len..];
                if let Some(end_marker_pos_rel) = search_area.rfind(&end_marker) {
                    let end_marker_pos_abs = initial_buffer_len + end_marker_pos_rel;
                    debug!("Session {}: Found end marker '{}' at pos {}", session_id, end_marker, end_marker_pos_abs);

                    // Now find the start marker within the relevant part of the buffer
                    // Look between the initial length and the end marker position
                    let relevant_output_slice = &current_buffer[initial_buffer_len..end_marker_pos_abs];

                    if let Some(start_marker_pos_rel) = relevant_output_slice.find(&start_marker) {
                         let start_marker_pos_abs = initial_buffer_len + start_marker_pos_rel;
                         // Extract content between start marker (+ its length + newline) and end marker
                         let content_start = start_marker_pos_abs + start_marker.len() + 1; // +1 for newline after echo
                         let content_end = end_marker_pos_abs; // Position *before* the end marker

                         if content_start <= content_end {
                            let command_output = current_buffer[content_start..content_end].trim().to_string();
                            info!("Session {}: Successfully extracted command output ({} bytes)", session_id, command_output.len());
                            // Optional: Clean the command itself from the output if the shell echoes it?
                            // This is tricky. Let's return raw output between markers for now.
                            return Ok(command_output);
                         } else {
                             warn!("Session {}: Markers found but positions invalid (start={}, end={}). Returning empty.", session_id, content_start, content_end);
                             return Ok("".to_string());
                         }
                    } else {
                        // End marker found, but start marker wasn't in the expected place.
                        // This might happen if the command failed before echoing the start marker.
                        warn!("Session {}: Found end marker but not start marker. Command might have failed early. Returning output before end marker.", session_id);
                        return Ok(format!("MARKER_ERROR (Start marker missing)\nOutput before end marker:\n{}", relevant_output_slice.trim()));
                    }
                }
            } // else: end marker not found yet

            // Drop the lock and yield/sleep briefly before checking again
            drop(current_buffer);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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
                 info!("Session {}: Closing PTY master.", session_id);
                 let master_guard = state.pty_master.lock().await;
                 master_guard.close()?; // Close the master FD
                 drop(master_guard);

                 // Wait a short time for the reader task to finish due to EOF
                 tokio::time::sleep(Duration::from_millis(100)).await;

                 // Abort the reader task if it hasn't finished (it should have due to EOF/close)
                 if !state.reader_handle.is_finished() {
                     warn!("Session {}: Reader task still running after PTY close, aborting.", session_id);
                     state.reader_handle.abort();
                 }

                 // Attempt to kill the process group if PID is known, just in case closing PTY wasn't enough
                 if let Some(pid_val) = state.process_pid {
                     info!("Session {}: Attempting to kill process group {} (PID: {})", session_id, pid_val, pid_val);
                     let pid = Pid::from_raw(pid_val);
                     match kill(pid, Signal::SIGKILL) { // Send SIGKILL directly to ensure termination
                         Ok(_) => info!("Session {}: Sent SIGKILL to process {}.", session_id, pid_val),
                         Err(e) => warn!("Session {}: Failed to send SIGKILL to process {}: {} (process might have already exited)", session_id, pid_val, e),
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

    #[tool(description = "Runs a command within an active terminal session and returns its output. Waits for the command to complete or times out.")]
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
