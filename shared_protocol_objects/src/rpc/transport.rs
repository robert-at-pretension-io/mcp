use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::sync::Arc;
// Removed unused AsyncReadExt
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
// Revert to using tokio::process::Command
use tokio::process::{Child, ChildStdin, ChildStderr, ChildStdout, Command}; 
use std::process::Stdio; // Keep Stdio
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
    // Restore process field using tokio::process::Child
    #[allow(dead_code)] // Keep allow dead_code for now
    process: Arc<Mutex<Child>>, 
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub stdout: Arc<Mutex<ChildStdout>>,
    pub stderr: Arc<Mutex<ChildStderr>>,
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,
    // Removed _child_pid field
}

impl ProcessTransport {
    /// Create a new process transport using tokio::process::Command
    pub async fn new(mut command: Command) -> Result<Self> { // Changed back to tokio::process::Command
        // Set up tokio::process::Command with piped stdin/stdout/stderr
        command.stdin(Stdio::piped())
               .stdout(Stdio::piped())
               .stderr(Stdio::piped()); // Capture stderr

        debug!("Spawning process using tokio::process: {:?}", command);
        // Spawn using tokio::process::Command
        let mut child = command.spawn()
             .map_err(|e| anyhow!("Failed to spawn process using tokio::process: {}", e))?;
        
        // Take tokio handles directly
        let stdin = child.stdin.take()
            .ok_or_else(|| anyhow!("Failed to get stdin handle from child process"))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| anyhow!("Failed to get stdout handle from child process"))?;
        let stderr = child.stderr.take() // Take stderr
            .ok_or_else(|| anyhow!("Failed to get stderr handle from child process"))?;

        let stdin_arc = Arc::new(Mutex::new(stdin));
        let stdout_arc = Arc::new(Mutex::new(stdout));
        let stderr_arc = Arc::new(Mutex::new(stderr)); // Wrap stderr

        let transport = Self {
            process: Arc::new(Mutex::new(child)), // Store the tokio::process::Child
            stdin: stdin_arc,
            stdout: stdout_arc,
            stderr: stderr_arc.clone(), // Clone Arc for the struct field
            notification_handler: Arc::new(Mutex::new(None)),
            // Removed _child_pid field
        };

        // --- Re-enable stderr reader task ---
        let stderr_reader_arc = stderr_arc; // Use the Arc created above
        tokio::spawn(async move {
            // Lock the Arc<Mutex<ChildStderr>>
            let mut stderr_locked = stderr_reader_arc.lock().await;
            let mut reader = BufReader::new(&mut *stderr_locked); // Pass mutable reference to locked stderr
            let mut line = String::new();
            info!("Stderr reader task started."); // Log start
            loop {
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        info!("Stderr reader task: read_line returned Ok(0) (EOF). Server stderr stream closed.");
                        break;
                    }
                    Ok(n) => { // Log bytes read
                        info!("Stderr reader task: read_line returned Ok({}) bytes.", n);
                        warn!("[Server STDERR] {}", line.trim_end());
                        line.clear();
                    }
                    Err(e) => {
                        error!("Stderr reader task: Error reading from server stderr: {}", e);
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
        info!("Attempting to acquire stdout lock for response...");
        let mut stdout_guard = self.stdout.lock().await;
        info!("Successfully acquired stdout lock for response.");
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
        info!("Calling reader.read_line() within timeout block...");
        match tokio::time::timeout(timeout_duration, reader.read_line(&mut response_line)).await {
            Ok(Ok(0)) => { // EOF
                error!("read_line returned Ok(0) (EOF). Child process closed stdout without sending response line.");
                return Err(anyhow!("Child process closed stdout without sending response"));
            }
            Ok(Ok(n)) => { // Successfully read a line
                info!("read_line returned Ok({}) bytes.", n);
                response_str = response_line.trim().to_string(); // Assign to existing variable
                info!("Trimmed response string (first 100 chars): {:.100}", response_str); // Log only first 100 chars
            }
            Ok(Err(e)) => { // I/O error
                error!("read_line returned I/O error: {}", e);
                return Err(anyhow!("I/O error reading response line: {}", e));
            }
            Err(_) => { // Timeout
                error!("read_line timed out after {} seconds", timeout_duration.as_secs());
                return Err(anyhow!("Timed out waiting for response line from server"));
            }
        }
        info!("Finished reading response line.");
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
        info!("Releasing stdout lock before parsing.");
        drop(stdout_guard);
        info!("Stdout lock released.");

        // Log the raw response string before parsing
        // Log only first 500 chars for brevity in case of large responses
        info!("Attempting to parse response string (first 500 chars): {:.500}", response_str);

        // Parse the response string
        let response = serde_json::from_str::<JsonRpcResponse>(&response_str)
            .map_err(|e| anyhow!("Failed to parse response: {}, raw: {}", e, response_str))?;

        // Log the successfully parsed response
        // Use debug level for potentially verbose full response object
        debug!("Successfully parsed response: {:?}", response);
        info!("Successfully parsed response ID: {:?}", response.id);


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
