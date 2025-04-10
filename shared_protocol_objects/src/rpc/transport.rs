use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
            let mut stderr_locked = stderr_arc.lock().await; // Lock stderr Arc
            let mut reader = BufReader::new(&mut *stderr_locked); // Pass mutable reference
            // Use a byte buffer instead of a String line buffer
            let mut buffer = bytes::BytesMut::with_capacity(1024); 

            loop {
                match reader.read_buf(&mut buffer).await {
                    Ok(0) => {
                        // EOF
                        info!("Server stderr stream closed.");
                        break;
                    }
                    Ok(n) if n > 0 => {
                        // Log the bytes read as a string (lossy conversion)
                        // Split into lines for potentially cleaner logging, handle partial lines
                        let output = String::from_utf8_lossy(&buffer[..n]);
                        for line in output.lines() {
                             warn!("[Server STDERR] {}", line.trim());
                        }
                        // Clear the buffer after processing
                        buffer.clear(); 
                    }
                    Ok(_) => {
                        // n == 0, but not EOF? Should not happen with read_buf typically, but handle defensively.
                        // Small sleep to avoid tight loop if something weird happens.
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
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
        drop(stdin_guard);
        
        // Now read the response directly
        info!("Acquiring stdout lock for response");
        let mut stdout_guard = self.stdout.lock().await;
        let mut reader = BufReader::new(&mut *stdout_guard);
        let mut response_line = String::new();
        
        // Add a timeout to the read
        info!("Starting read_line with {}s timeout...", 300); // Log timeout duration
        match tokio::time::timeout(std::time::Duration::from_secs(300), reader.read_line(&mut response_line)).await {
            Ok(Ok(0)) => {
                error!("Child process closed stdout (read 0 bytes) before sending full response");
                return Err(anyhow!("Child process closed stdout before sending full response"));
            },
            Ok(Ok(bytes_read)) => {
                // ---> ADDED LOGS <---
                info!("read_line successful: Read {} bytes from stdout", bytes_read);
                info!("Raw response line (before trim): {:?}", response_line);
                // ---> END ADDED LOGS <---
                let response_str = response_line.trim();
                info!("Trimmed response string: {}", response_str);

                // Release the stdout lock
                drop(stdout_guard);
                
                // Parse the response
                // Parse the response
                let response = serde_json::from_str::<JsonRpcResponse>(response_str)
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
            },
            Ok(Err(e)) => {
                error!("Error reading from stdout: {}", e);
                Err(anyhow!("I/O error reading from stdout: {}", e)) // More specific error
            },
            Err(_) => { // This is the TimeoutExpired error
                error!("read_line timed out after {} seconds", 300); // Log timeout duration
                Err(anyhow!("Timed out waiting for response line from server")) // More specific error
            }
        }
    }
    
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
