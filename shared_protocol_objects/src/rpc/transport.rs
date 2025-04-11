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
        // Add a small delay between messages to avoid race conditions
        // Consider if this is truly necessary or if locking handles it.
        // tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let request_str = serde_json::to_string(&request)? + "\n";
        // Use trace for potentially large/frequent raw messages
        trace!("Sending raw request: {}", request_str.trim());
        info!("Sending request method: {}, id: {:?}", request.method, request.id);

        // Send the request
        {
            let mut stdin_guard = self.stdin.lock().await;
            // info!("Writing request to stdin"); // Can be noisy, use debug or trace
            stdin_guard.write_all(request_str.as_bytes()).await?;
            // info!("Flushing stdin"); // Can be noisy
            stdin_guard.flush().await?;
            // info!("Releasing stdin lock (scope end)"); // Can be noisy
        }

        // Read the response
        // info!("Attempting to acquire stdout lock for response..."); // Can be noisy
        let mut stdout_guard = self.stdout.lock().await;
        // info!("Successfully acquired stdout lock for response."); // Can be noisy

        let timeout_duration = std::time::Duration::from_secs(60); // Slightly longer default timeout
        // info!("Reading response line with timeout of {}s", timeout_duration.as_secs()); // Can be noisy

        let mut reader = tokio::io::BufReader::new(&mut *stdout_guard);
        let mut response_line = String::new();

        // Use timeout for the read_line operation
        let read_result = tokio::time::timeout(timeout_duration, reader.read_line(&mut response_line)).await;

        let response_str = match read_result {
            Ok(Ok(0)) => {
                error!("EOF received while waiting for response to request id {:?} (method {}). Server likely closed stdout.", request.id, request.method);
                return Err(anyhow!("Server closed stdout unexpectedly (EOF)"));
            }
            Ok(Ok(n)) => {
                info!("Successfully read response line ({} bytes) for request id {:?}", n, request.id);
                response_line.trim().to_string() // Trim whitespace and newline
            }
            Ok(Err(e)) => {
                error!("IO error reading response for request id {:?}: {}", request.id, e);
                return Err(anyhow!("IO error reading response: {}", e));
            }
            Err(_) => { // Timeout elapsed
                error!("Timeout ({:?}) waiting for response to request id {:?} (method {})", timeout_duration, request.id, request.method);
                // Attempt to read any buffered data for debugging, but don't block
                let mut remaining_buf = String::new();
                // Use a short timeout for this debug read
                // Need to import Duration from std::time
                use std::time::Duration; 
                match tokio::time::timeout(Duration::from_millis(100), reader.read_to_string(&mut remaining_buf)).await {
                    Ok(Ok(bytes_read)) if bytes_read > 0 => {
                         warn!("Read {} additional bytes after timeout: '{}'", bytes_read, remaining_buf.trim());
                    }
                    _ => {
                         warn!("No additional data read after timeout.");
                    }
                }
                return Err(anyhow!("Timeout waiting for response"));
            }
        };

        // Release the stdout lock *before* parsing
        // info!("Releasing stdout lock before parsing."); // Can be noisy
        drop(stdout_guard);
        // info!("Stdout lock released."); // Can be noisy

        // Log raw response at trace level
        trace!("Raw response string received: {}", response_str);
        info!("Attempting to parse response for request id {:?}", request.id);

        // Parse the response string
        let response: JsonRpcResponse = match serde_json::from_str(&response_str) {
             Ok(resp) => resp,
             Err(e) => {
                  error!("Failed to parse JSON response for request id {:?}: {}", request.id, e);
                  error!("Raw response data that failed parsing: {}", response_str);
                  // Consider including more context if possible (e.g., first/last N chars)
                  return Err(anyhow!("Failed to parse JSON response: {}. Raw data: '{}'", e, response_str));
             }
        };

        // Log the successfully parsed response ID and potentially result/error presence
        info!("Successfully parsed response for request id {:?}. Response ID: {:?}. Has result: {}, Has error: {}",
              request.id, response.id, response.result.is_some(), response.error.is_some());
        // Log full response at debug level
        debug!("Parsed response details: {:?}", response);

        // Strict ID check - return error if mismatch.
        // Allow Null ID response if request ID was Null (for notifications treated as requests, though unusual)
        if response.id != request.id && !(response.id.is_null() && request.id.is_null()) {
            error!(
                "Response ID mismatch for method {}: expected {:?}, got {:?}. This indicates a critical protocol error.",
                request.method, request.id, response.id
            );
            return Err(anyhow!("Response ID mismatch: expected {:?}, got {:?}", request.id, response.id));
        }

        Ok(response)
    }
    
    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        let notification_str = serde_json::to_string(&notification)? + "\n";
        // Use trace for potentially large/frequent raw messages
        trace!("Sending raw notification: {}", notification_str.trim());
        info!("Sending notification method: {}", notification.method);

        {
            let mut stdin_guard = self.stdin.lock().await;
            stdin_guard.write_all(notification_str.as_bytes()).await?;
            stdin_guard.flush().await?;
            // debug!("Stdin flushed for notification: {}", notification.method); // Can be noisy
            // info!("Notification sent successfully, releasing stdin lock (scope end)"); // Can be noisy
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
