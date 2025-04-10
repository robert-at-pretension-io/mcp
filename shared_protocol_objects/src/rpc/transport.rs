use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::sync::Arc;
// Added AsyncReadExt for read_buf
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}; 
use tokio::process::{Child, ChildStdin, ChildStderr, ChildStdout, Command}; // Added ChildStderr
use std::process::Stdio;
use tokio::sync::{Mutex};
use tracing::{debug, error, info, trace, warn};

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
    #[allow(dead_code)]
    process: Arc<Mutex<Child>>, // Wrap Child in Arc<Mutex> for potential future use (e.g., kill)
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub stdout: Arc<Mutex<ChildStdout>>,
    pub stderr: Arc<Mutex<ChildStderr>>, // Added stderr field
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,
    // Removed _stderr_reader_handle as we'll spawn directly
}

impl ProcessTransport {
    /// Create a new process transport
    pub async fn new(mut command: Command) -> Result<Self> {
        // Set up process with piped stdin/stdout/stderr
        command.stdin(Stdio::piped())
               .stdout(Stdio::piped())
               .stderr(Stdio::piped()); // Capture stderr

        debug!("Spawning process: {:?}", command);
        let mut child = command.spawn()?;
        
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
            process: Arc::new(Mutex::new(child)), // Wrap child process
            stdin: stdin_arc,
            stdout: stdout_arc,
            stderr: stderr_arc.clone(), // Clone Arc for the struct field
            notification_handler: Arc::new(Mutex::new(None)),
        };

        // Spawn stderr reader task
        // Spawn stderr reader task
        tokio::spawn(async move {
            let mut stderr_locked = stderr_arc.lock().await; // Lock stderr Arc outside BufReader
            let mut reader = BufReader::new(&mut *stderr_locked); // Pass mutable reference to locked stderr
            let mut line = String::new();
            loop {
                // Reverted to read_line for stderr as it seems simpler and no issues were observed there.
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // EOF
                        info!("Server stderr stream closed.");
                        break;
                    }
                    Ok(_) => {
                        // Log the line with a prefix
                        warn!("[Server STDERR] {}", line.trim_end());
                        line.clear(); // Clear buffer for next line
                    }
                    Err(e) => {
                        error!("Error reading from server stderr: {}", e);
                        break;
                    }
                }
            }
        });
        
        // Skip notification listener for now as it's causing issues
        // transport.start_notification_listener().await?;
        
        Ok(transport)
    }

    /// Start the background task for listening to incoming notifications
    async fn start_notification_listener(&self) -> Result<()> {
        let stdout = Arc::clone(&self.stdout);
        let notification_handler = Arc::clone(&self.notification_handler);
        
        tokio::spawn(async move {
            info!("Starting notification listener");
            let reader_stdout = Arc::clone(&stdout);
            
            // Use BytesMut for better buffer handling
            let mut response_buffer = bytes::BytesMut::with_capacity(8192);
            
            loop {
                // Use tokio::select to handle backpressure and avoid tight polling loops
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                        // This branch exists just to provide a small pause between iterations
                        // to avoid CPU spinning when no data is available
                    }
                    
                    // Try to read from stdout but don't block request processing
                    result = async {
                        // Try to acquire the lock - non-blocking
                        if let Ok(mut stdout_guard) = reader_stdout.try_lock() {
                            trace!("Notification listener acquired stdout lock");
                            // Only read if there's actual data available (prevent blocking)
                            // Create a reader from the stdout guard
                            let mut reader = BufReader::new(&mut *stdout_guard);
                            
                            // Try a small read to see if data is available
                            // Use a shorter timeout to avoid blocking request processing
                            match tokio::time::timeout(
                                tokio::time::Duration::from_millis(5),
                                reader.read_buf(&mut response_buffer)
                            ).await {
                                Ok(Ok(0)) => {
                                    // EOF
                                    info!("Child process closed stdout, stopping notification listener");
                                    return Err("EOF");
                                }
                                Ok(Ok(n)) => {
                                    trace!("Notification listener read {} bytes", n);
                                    // Check if we have complete messages (ending with newlines)
                                    while let Some(newline_pos) = response_buffer.iter().position(|&b| b == b'\n') {
                                        // Extract a complete line
                                        let line_bytes = response_buffer.split_to(newline_pos + 1);
                                        if let Ok(line) = String::from_utf8(line_bytes.freeze().to_vec()) {
                                            let line = line.trim().to_string();
                                            
                                            // Process the line if it's not empty
                                            if !line.is_empty() {
                                                trace!("Processing line: {}", line);
                                                
                                                // Check if this looks like a notification (has method, no id)
                                                if line.contains("\"method\":") && !line.contains("\"id\":") {
                                                    debug!("Found notification: {}", line);
                                                    
                                                    // Try to parse as notification
                                                    if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(&line) {
                                                        info!("Parsed notification for method: {}", notification.method);
                                                        
                                                        // Check if we have a handler and call it in a separate task
                                                        let notif_clone = notification.clone();
                                                        let handler_for_task = Arc::clone(&notification_handler);
                                                        
                                                        tokio::spawn(async move {
                                                            if let Some(handler) = &*handler_for_task.lock().await {
                                                                debug!("Calling notification handler for {}", notification.method);
                                                                handler(notif_clone).await;
                                                            } else {
                                                                debug!("No notification handler registered");
                                                            }
                                                        });
                                                    } else {
                                                        warn!("Failed to parse notification: {}", line);
                                                    }
                                                } else {
                                                    // This is likely a response to a request
                                                    trace!("Line appears to be a response, skipping in notification listener");
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(Err(e)) => {
                                    error!("Error reading from stdout in notification listener: {}", e);
                                    return Err("Read error");
                                }
                                Err(_) => {
                                    // Timeout (no data currently available)
                                    trace!("No data available for notification listener");
                                }
                            }
                            
                            // Always drop the lock to allow request/response handlers to proceed
                            drop(stdout_guard);
                        }
                        
                        // Always return Ok to continue the loop
                        Ok(())
                    } => {
                        match result {
                            Err(_) => {
                                // Exit the loop on errors (EOF or read error)
                                break;
                            }
                            _ => { /* Continue loop */ }
                        }
                    }
                }
            }
            
            info!("Notification listener stopped");
        });
        
        // Small delay to ensure the listener is started before proceeding
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        Ok(())
    }
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
        
        // Now read the response directly
        info!("Acquiring stdout lock for response");
        let mut stdout_guard = self.stdout.lock().await;
        let mut reader = BufReader::new(&mut *stdout_guard);
        // Use BytesMut buffer to accumulate response data
        let mut response_buffer = bytes::BytesMut::with_capacity(8192); // Start with 8KB, might grow
        let response_str: String; // To hold the final decoded string

        // Add a timeout to the read loop
        let timeout_duration = std::time::Duration::from_secs(300);
        info!("Starting response read loop with {}s timeout...", timeout_duration.as_secs());

        match tokio::time::timeout(timeout_duration, async {
            let mut retry_count = 0;
            let max_retries = 5;
            
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

                // No newline yet, read more data
                trace!("No newline found, attempting to read more data...");
                match reader.read_buf(&mut response_buffer).await {
                    Ok(0) => {
                        // EOF reached before finding a newline
                        warn!("EOF reached before newline found. Buffer size: {}", response_buffer.len());
                        
                        // If we have data in the buffer, try to use it even if no newline was found
                        if !response_buffer.is_empty() {
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
                       
                        // If the buffer is empty and we haven't exceeded max retries, try again
                        if retry_count < max_retries {
                            retry_count += 1;
                            warn!("Empty buffer at EOF, retry {}/{}. Waiting 500ms before retry...", 
                                  retry_count, max_retries);
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                            
                            // Try to reopen the stdout connection
                            info!("Retry {}/{}: Continuing read loop after EOF", retry_count, max_retries);
                            continue;
                        } else {
                            // Max retries exceeded
                            error!("Max retries ({}) exceeded after EOF. Giving up.", max_retries);
                            return Err(anyhow!("Max retries exceeded after EOF"));
                        }
                    }
                    Ok(n) => {
                        // Read n bytes successfully, loop will check for newline again
                        // Use info level for read success to ensure visibility
                        info!("Read {} bytes from stdout, accumulated {} bytes", n, response_buffer.len());
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
                }
            }
        }).await {
            Ok(Ok(line)) => {
                // Successfully read a line (complete or partial at EOF)
                // Check if we got an empty line from an EOF retry
                if line.is_empty() {
                    error!("Empty response returned. This likely indicates a server communication issue.");
                    return Err(anyhow!("Empty response from server - connection may be broken"));
                }
                
                info!("Raw response line (before trim): {:?}", line);
                response_str = line.trim().to_string(); // Trim whitespace/newline
                info!("Trimmed response string: {}", response_str);
            }
            Ok(Err(e)) => {
                // Inner future returned an error (I/O, decoding, buffer limit)
                error!("Error during response read loop: {}", e);
                return Err(e);
            }
            Err(_) => { // Outer timeout error
                error!("Response read timed out after {} seconds", timeout_duration.as_secs());
                return Err(anyhow!("Timed out waiting for response line from server"));
            }
        }

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
