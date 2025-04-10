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
        
        // We'll skip notification listener for now to simplify debugging
        // transport.start_notification_listener().await?;
        
        Ok(transport)
    }

    /// Start the background task for listening to incoming notifications
    #[allow(dead_code)] // Allow unused method for now
    async fn start_notification_listener(&self) -> Result<()> {
        let stdout = Arc::clone(&self.stdout);
        let notification_handler = Arc::clone(&self.notification_handler);
        
        tokio::spawn(async move {
            info!("Starting notification listener");
            let reader_stdout = Arc::clone(&stdout);
            let mut line_buffer = String::new();
            
            loop {
                // Try to acquire lock - skip this iteration if lock is not available
                // This allows request/response processing to take priority
                let mut stdout_guard = match reader_stdout.try_lock() {
                    Ok(guard) => {
                        trace!("Notification listener acquired stdout lock");
                        guard
                    },
                    Err(_) => {
                        // Lock is probably being held by a request handler
                        // Just wait and try again later
                        trace!("Notification listener couldn't acquire stdout lock, skipping iteration");
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                };
                
                // Now that we have the lock, read a line
                let mut reader = BufReader::new(&mut *stdout_guard);
                
                match tokio::time::timeout(
                    std::time::Duration::from_millis(100), 
                    reader.read_line(&mut line_buffer)
                ).await {
                    Ok(Ok(0)) => {
                        info!("Child process closed stdout, stopping notification listener");
                        break;
                    },
                    Ok(Ok(bytes_read)) => {
                        let line = line_buffer.trim().to_string();
                        line_buffer.clear();
                        
                        if line.is_empty() {
                            trace!("Empty line received in notification listener");
                            continue;
                        }
                        
                        trace!("Got line ({} bytes) from child process", bytes_read);
                        
                        // Drop the stdout guard ASAP to free the lock for request handlers
                        drop(stdout_guard);
                        
                        // Try to parse as notification first
                        if line.contains("\"method\":") && !line.contains("\"id\":") {
                            // This looks like a notification (has method, no id)
                            info!("Line appears to be a notification, attempting to parse");
                            
                            match serde_json::from_str::<JsonRpcNotification>(&line) {
                                Ok(notification) => {
                                    info!("Parsed notification for method: {}", notification.method);
                                    
                                    // Get a copy of the notification for potential handler
                                    let notif_clone = notification.clone();
                                    
                                    // Lock and check for handler
                                    let has_handler = {
                                        let handler_guard = notification_handler.lock().await;
                                        handler_guard.is_some()
                                    };
                                    
                                    // Call the handler if it exists (in separate task to avoid deadlock)
                                    if has_handler {
                                        info!("Dispatching notification to handler");
                                        let handler_for_task = Arc::clone(&notification_handler);
                                        
                                        tokio::spawn(async move {
                                            // Lock again inside the task
                                            if let Some(handler) = &*handler_for_task.lock().await {
                                                handler(notif_clone).await;
                                            }
                                        });
                                    } else {
                                        info!("No notification handler registered");
                                    }
                                },
                                Err(e) => {
                                    warn!("Failed to parse as notification: {}, raw: {}", e, line);
                                }
                            }
                        } else {
                            // This is likely a response, we'll let the request handler deal with it
                            trace!("Line appears to be a response, skipping in notification listener");
                        }
                    },
                    Ok(Err(e)) => {
                        error!("Error reading from child process: {}", e);
                        break;
                    },
                    Err(_) => {
                        // Read timed out, which is fine - release the lock and try again
                        trace!("Read timed out in notification listener");
                    }
                }
                // Small sleep to avoid CPU spinning
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            
            info!("Notification listener stopped");
        });
        
        Ok(())
    }
}

#[async_trait]
impl Transport for ProcessTransport {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let request_str = serde_json::to_string(&request)? + "\n";
        info!("Sending request: {}", request_str.trim());
        
        // First send the request directly
        let mut stdin_guard = self.stdin.lock().await;
        info!("Writing request to stdin");
        stdin_guard.write_all(request_str.as_bytes()).await?;
        info!("Flushing stdin");
        stdin_guard.flush().await?;
        // Explicitly close stdin after flushing
        info!("Closing stdin");
        drop(stdin_guard); 
        
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
            loop {
                // --- Start Enhanced Logging ---
                let newline_found = response_buffer.iter().position(|&b| b == b'\n');
                trace!("Read loop iteration: Buffer size = {}, Newline found = {:?}", response_buffer.len(), newline_found.is_some());
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
        
        let mut stdin_guard = self.stdin.lock().await;
        stdin_guard.write_all(notification_str.as_bytes()).await?;
        stdin_guard.flush().await?;
        info!("Notification sent successfully");
        
        Ok(())
    }
    
    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()> {
        let mut guard = self.notification_handler.lock().await;
        *guard = Some(handler);
        
        Ok(())
    }
    
    async fn close(&self) -> Result<()> {
        debug!("Closing process transport");
        
        // Don't try to kill the process directly since we can't get a mutable reference
        // We'll let the child process be dropped when transport is dropped
        
        Ok(())
    }
}
