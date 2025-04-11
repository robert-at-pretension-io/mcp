use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::sync::Arc;
// Removed unused AsyncReadExt
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
// Import Command from std::process
use std::process::{Command as StdCommand, Stdio, Child as StdChild};
// Remove unused TokioCommand import alias
use tokio::process::{ChildStdin, ChildStderr, ChildStdout}; 
use tokio::sync::{Mutex};
use tracing::{debug, error, info, warn};

use crate::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// Handler type for processing notifications
pub type NotificationHandler = Box<dyn Fn(JsonRpcNotification) -> BoxFuture<'static, ()> + Send + Sync + 'static>;

/// Abstract transport layer for JSON-RPC communication
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Send a request and wait for a response
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
    
    /// Send a notification (no response expected)
    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()>;
    
    /// Set up notification handling
    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()>;
    
    /// Close the transport
    async fn close(&self) -> Result<()>;
}

/// Transport for communicating with a child process via stdin/stdout
pub struct ProcessTransport {
    // process field removed as StdChild is not Send/Sync
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub stdout: Arc<Mutex<ChildStdout>>,
    pub stderr: Arc<Mutex<ChildStderr>>,
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,
    _child_pid: u32, // Added field to store PID
}

impl ProcessTransport {
    /// Create a new process transport using std::process::Command
    pub async fn new(mut command: StdCommand) -> Result<Self> {
        // Set up std::process::Command with piped stdin/stdout/stderr
        command.stdin(Stdio::piped())
               .stdout(Stdio::piped())
               .stderr(Stdio::piped()); // Capture stderr

        debug!("Spawning process using std::process: {:?}", command);
        // Spawn using std::process::Command
        let mut child: StdChild = command.spawn()
            .map_err(|e| anyhow!("Failed to spawn process using std::process: {}", e))?;

        // Take std handles
        let std_stdin = child.stdin.take()
            .ok_or_else(|| anyhow!("Failed to get std stdin handle from child process"))?;
        let std_stdout = child.stdout.take()
            .ok_or_else(|| anyhow!("Failed to get std stdout handle from child process"))?;
        let std_stderr = child.stderr.take()
            .ok_or_else(|| anyhow!("Failed to get std stderr handle from child process"))?;

        // Wrap std handles in Tokio async wrappers
        let stdin = ChildStdin::from_std(std_stdin)?;
        let stdout = ChildStdout::from_std(std_stdout)?;
        let stderr = ChildStderr::from_std(std_stderr)?;

        let stdin_arc = Arc::new(Mutex::new(stdin));
        let stdout_arc = Arc::new(Mutex::new(stdout));
        let stderr_arc = Arc::new(Mutex::new(stderr)); // Wrap Tokio stderr

        // We need a way to manage the StdChild lifetime or kill it.
        // For now, we can't store StdChild directly as it's not Send/Sync.
        // Let's store the PID and manage it manually if needed, though this is less ideal.
        let child_pid = child.id();
        debug!("Process spawned with PID: {}", child_pid);
        // Consider using libraries like `async_process` if more robust management is needed.

        let transport = Self {
            // process field removed as StdChild is not Send/Sync
            stdin: stdin_arc,
            stdout: stdout_arc,
            stderr: stderr_arc.clone(), // Clone Arc for the struct field
            notification_handler: Arc::new(Mutex::new(None)),
            // Store PID instead of the process handle
            _child_pid: child_pid, // Add a field to store PID (prefixed as it's mainly for debug/kill)
        };

        // --- Re-enable stderr reader task ---
        let stderr_reader_arc = stderr_arc; // Use the already cloned Arc
        tokio::spawn(async move {
            // Lock the Arc<Mutex<ChildStderr>>
            let mut stderr_locked = stderr_reader_arc.lock().await;
            let mut reader = BufReader::new(&mut *stderr_locked); // Pass mutable reference to locked stderr
            let mut line = String::new();
            loop {
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        info!("Server stderr stream closed.");
                        break;
                    }
                    Ok(_) => {
                        warn!("[Server STDERR] {}", line.trim_end());
                        line.clear();
                    }
                    Err(e) => {
                        error!("Error reading from server stderr: {}", e);
                        break;
                    }
                }
            }
        }); // End of tokio::spawn
        info!("Stderr reader task re-enabled.");
        // --- End re-enable ---

        // Skip notification listener for now as it's causing issues
        // transport.start_notification_listener().await?;
        
        Ok(transport)
    }

    // Removed the unused start_notification_listener function
}

#[async_trait]
impl Transport for ProcessTransport {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let request_str = serde_json::to_string(&request)? + "\n";
        info!("Sending request: {}", request_str.trim());
        
        // First send the request directly
        {
            let mut stdin_guard = self.stdin.lock().await;
            info!("Writing request to stdin");
            stdin_guard.write_all(request_str.as_bytes()).await?;
            info!("Flushing stdin");
            stdin_guard.flush().await?;
            info!("Releasing stdin lock (scope end)");
            // Lock is automatically released at the end of this scope
        }

        // <<< ADD DELAY HERE >>>
        tokio::time::sleep(std::time::Duration::from_millis(100)).await; // 100ms delay
        // <<< END DELAY >>>
        
        // Now read the response directly
        info!("Acquiring stdout lock for response");
        let mut stdout_guard = self.stdout.lock().await;
        // --- Re-introduce BufReader with larger capacity ---
        let mut reader = BufReader::with_capacity(16384, &mut *stdout_guard); // Use 16KB buffer
        // Use BytesMut buffer to accumulate response data
        let _response_buffer = bytes::BytesMut::with_capacity(16384); // Prefix unused variable, remove mut
        let response_str: String; // To hold the final decoded string

        // Add a timeout to the read loop
        let timeout_duration = std::time::Duration::from_secs(300);
        info!("Starting response read loop (using read_line) with {}s timeout...", timeout_duration.as_secs());

        // --- Use read_line instead of read_buf loop ---
        let mut response_line = String::new(); // Use String directly
        match tokio::time::timeout(timeout_duration, reader.read_line(&mut response_line)).await {
            Ok(Ok(0)) => { // EOF
                error!("Child process closed stdout without sending response line.");
                return Err(anyhow!("Child process closed stdout without sending response"));
            }
            Ok(Ok(n)) => { // Successfully read a line
                info!("Successfully read line ({} bytes) using read_line", n);
                response_str = response_line.trim().to_string(); // Assign to existing variable
                info!("Trimmed response string: {}", response_str);
            }
            Ok(Err(e)) => { // I/O error
                error!("Error reading line from stdout: {}", e);
                return Err(anyhow!("I/O error reading response line: {}", e));
            }
            Err(_) => { // Timeout
                error!("Response read (read_line) timed out after {} seconds", timeout_duration.as_secs());
                return Err(anyhow!("Timed out waiting for response line from server"));
            }
        }
        // --- End of read_line logic ---

        /* --- Start of removed read_buf loop ---
        match tokio::time::timeout(timeout_duration, async {
            let mut _retry_count = 0; // Prefixed with underscore
            let _max_retries = 5; // Prefixed with underscore
            
            loop {
                // --- Start Enhanced Logging ---
                let newline_found = response_buffer.iter().position(|&b| b == b'\n');
                trace!("Read loop iteration: Buffer size = {}, Newline found = {:?}, Retry count = {}", 
                      response_buffer.len(), newline_found.is_some(), retry_count);
                // --- End Enhanced Logging ---

                // Check if we found a newline in the current buffer
                if let Some(newline_pos) = newline_found { // Use the variable checked above
                    info!("Newline found at position {}", newline_pos); // Log position
                    // Found newline, extract the line
                    let line_bytes = response_buffer.split_to(newline_pos + 1); // Include newline
                    trace!("Extracted line bytes ({} bytes): {:?}", line_bytes.len(), line_bytes); // Log extracted bytes
                    // Decode *only* the extracted line
                    match String::from_utf8(line_bytes.freeze().to_vec()) { // Use freeze().to_vec() for efficiency if needed
                        Ok(line) => {
                            info!("Successfully read and decoded line ({} bytes)", line.len());
                            return Ok(line); // Return the complete line
                        }
                        Err(e) => {
                            error!("UTF-8 decoding error after finding newline: {}", e);
                            return Err(anyhow!("UTF-8 decoding error in response: {}", e));
                        }
                    }
                }

                // No newline yet, read more data using BufReader's read_buf
                trace!("No newline found, attempting to read more data using BufReader...");
                match reader.read_buf(&mut response_buffer).await {
                    Ok(0) => {
                        // EOF reached before finding a newline
                        warn!("EOF reached before newline found. Buffer size: {}", response_buffer.len());
                        if response_buffer.is_empty() {
                            error!("Child process closed stdout without sending any response data.");
                            return Err(anyhow!("Child process closed stdout without sending response"));
                        } else {
                            // EOF, but we have partial data without a newline. Try to decode what we have.
                            warn!("Child process closed stdout with partial data and no trailing newline.");
                            trace!("Partial data at EOF ({} bytes): {:?}", response_buffer.len(), response_buffer); // Log partial data
                            match String::from_utf8(response_buffer.to_vec()) {
                                Ok(line) => {
                                    info!("Successfully decoded partial line at EOF ({} bytes)", line.len());
                                    return Ok(line); // Return the partial line
                                }
                                Err(e) => {
                                     error!("UTF-8 decoding error for partial data at EOF: {}", e);
                                     return Err(anyhow!("UTF-8 decoding error in partial response at EOF: {}", e));
                                }
                            }
                        }
                    }
                    Ok(n) => {
                        // Read n bytes successfully, loop will check for newline again
                        // Use info level for read success to ensure visibility
                        info!("Read {} bytes using BufReader, accumulated {} bytes", n, response_buffer.len());
                        // Optional: Add a check for excessively large buffers to prevent OOM
                        if response_buffer.len() > 1_000_000 { // Example limit: 1MB
                             error!("Response buffer exceeded 1MB limit without newline. Aborting.");
                             return Err(anyhow!("Response exceeded buffer limit without newline"));
                        }
                    }
                    Err(e) => {
                        error!("Error reading from stdout: {}", e);
                        return Err(anyhow!("I/O error reading from stdout: {}", e));
                    }
        */ // --- End of removed read_buf loop ---

        // Release the stdout lock
        drop(stdout_guard);

        // Parse the response string
        let response = serde_json::from_str::<JsonRpcResponse>(&response_str)
            .map_err(|e| anyhow!("Failed to parse response: {}, raw: {}", e, response_str))?;

        // Basic ID check - log warning if mismatch, but proceed.
        // Strict applications might want to return an error here.
        if response.id != request.id {
            warn!(
                "Response ID mismatch for method {}: expected {:?}, got {:?}. This might indicate server issues.",
                request.method, request.id, response.id
            );
            // Depending on strictness, you might return an error:
            // return Err(anyhow!("Response ID mismatch: expected {:?}, got {:?}", request.id, response.id));
        }
        Ok(response)
        // <<< The closing brace for the match was missing here >>>
    } // <<< This closes the send_request function >>>
    
    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        let notification_str = serde_json::to_string(&notification)? + "\n";
        info!("Sending notification: {}", notification_str.trim());
        
        {
            let mut stdin_guard = self.stdin.lock().await;
            stdin_guard.write_all(notification_str.as_bytes()).await?;
            stdin_guard.flush().await?;
            info!("Notification sent successfully, releasing stdin lock (scope end)");
        }
        
        Ok(())
    }
    
    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()> {
        let mut guard = self.notification_handler.lock().await;
        *guard = Some(handler);
        
        Ok(())
    }
    
    async fn close(&self) -> Result<()> {
        debug!("Closing process transport");
        
        // Explicitly close stdin to signal EOF to the child process
        {
            let _stdin_guard = self.stdin.lock().await;
            debug!("Closing stdin to signal EOF to child process");
            // Let the guard drop naturally which will close the handle
            // when it goes out of scope
        }
        
        // Don't try to kill the process directly since we can't get a mutable reference
        // We'll let the child process be dropped when transport is dropped
        
        Ok(())
    }
}
