use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use tokio::io::AsyncReadExt;
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

        // Small delay to ensure process has time to handle the request
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Now read the response directly
        info!("Attempting to acquire stdout lock for response...");
        let mut stdout_guard = self.stdout.lock().await;
        info!("Successfully acquired stdout lock for response.");
        
        // Simpler approach: read the response as a string with a timeout
        let timeout_duration = std::time::Duration::from_secs(30); // Shorter timeout
        info!("Reading response with timeout of {}s", timeout_duration.as_secs());

        // Create a buffer for reading
        let mut buf = Vec::with_capacity(16384);
        let mut reader = tokio::io::BufReader::new(&mut *stdout_guard);
        
        // Try to read a complete line with timeout
        let read_future = async {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => Err(anyhow!("EOF reading response")),
                Ok(_) => Ok(line),
                Err(e) => Err(anyhow!("Error reading response: {}", e)),
            }
        };
        
        let response_str = match tokio::time::timeout(timeout_duration, read_future).await {
            Ok(Ok(line)) => {
                info!("Successfully read response line of {} bytes", line.len());
                line.trim().to_string()
            },
            Ok(Err(e)) => {
                error!("Error reading response: {}", e);
                return Err(e);
            },
            Err(_) => {
                error!("Timeout reading response");
                
                // Try to read what's available before giving up
                info!("Attempting to read any available data before timeout");
                match reader.read_to_end(&mut buf).await {
                    Ok(n) if n > 0 => {
                        warn!("Read {} bytes after timeout", n);
                        match String::from_utf8(buf) {
                            Ok(s) => {
                                warn!("Response after timeout: {}", s);
                                s
                            },
                            Err(e) => {
                                error!("Invalid UTF-8 in response: {}", e);
                                return Err(anyhow!("Timeout and invalid UTF-8 in response"));
                            }
                        }
                    },
                    _ => {
                        return Err(anyhow!("Timeout waiting for response"));
                    }
                }
            }
        };

        // Release the stdout lock
        info!("Releasing stdout lock before parsing.");
        drop(stdout_guard);
        info!("Stdout lock released.");

        // Log the raw response string before parsing
        info!("Attempting to parse response string (first 500 chars): {:.500}", response_str);

        // Parse the response string
        let response = serde_json::from_str::<JsonRpcResponse>(&response_str)
            .map_err(|e| anyhow!("Failed to parse response: {}, raw: {}", e, response_str))?;

        // Log the successfully parsed response
        debug!("Successfully parsed response: {:?}", response);
        info!("Successfully parsed response ID: {:?}", response.id);

        // Basic ID check - log warning if mismatch, but proceed.
        if response.id != request.id {
            warn!(
                "Response ID mismatch for method {}: expected {:?}, got {:?}. This might indicate server issues.",
                request.method, request.id, response.id
            );
        }
        Ok(response)
    }
    
    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        let notification_str = serde_json::to_string(&notification)? + "\n";
        info!("Sending notification: {}", notification_str.trim());
        
        {
            let mut stdin_guard = self.stdin.lock().await;
            stdin_guard.write_all(notification_str.as_bytes()).await?;
            stdin_guard.flush().await?;
            debug!("Stdin flushed for notification: {}", notification.method);
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
            let mut stdin_guard = self.stdin.lock().await;
            debug!("Closing stdin to signal EOF to child process");
            // Explicitly flush before closing
            if let Err(e) = stdin_guard.flush().await {
                error!("Error flushing stdin before close: {}", e);
            }
            
            // Explicitly close stdin after flushing
            std::mem::drop(stdin_guard);
            debug!("Stdin has been flushed and dropped");
        }
        
        // Try to gracefully kill the process (best effort)
        {
            let mut process_guard = self.process.lock().await;
            debug!("Attempting to kill child process gracefully");
            // This is a best-effort attempt; log errors but continue
            if let Err(e) = process_guard.start_kill() {
                error!("Error starting process kill: {}", e);
            } else {
                debug!("Process kill signal sent successfully");
            }
        }
        
        Ok(())
    }
}
