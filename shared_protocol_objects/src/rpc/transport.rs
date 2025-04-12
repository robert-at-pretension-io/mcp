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
use serde_json::Value;
// Import trace macro along with others
use tracing::{debug, error, info, warn, trace}; 

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

        // Start the notification listener to handle incoming notifications
        transport.start_notification_listener().await?;
        
        Ok(transport)
    }

    /// Read a response matching the given request ID with proper buffering
    async fn read_matching_response(
        &self,
        method: &str,
        request_id: &Value,
        timeout_duration: std::time::Duration
    ) -> Result<JsonRpcResponse> {
        // Lock stdout for reading the response
        debug!("Acquiring stdout lock to read response for method: {}", method);
        let mut stdout_guard = self.stdout.lock().await;
        debug!("Acquired stdout lock for method: {}", method);

        let mut reader = BufReader::new(&mut *stdout_guard);
        let mut buffer = String::new();
        // Removed unused found_matching_response variable

        // Read until we find a matching response or hit timeout
        loop { // Changed to infinite loop, exits via return
            // Clear the buffer for this iteration if needed
            if !buffer.is_empty() {
                buffer.clear();
            }
            
            // Use timeout for the read_line operation to prevent hangs
            let read_result = tokio::time::timeout(
                timeout_duration, 
                reader.read_line(&mut buffer)
            ).await;
            
            match read_result {
                // Successfully read something (or EOF)
                Ok(Ok(0)) => {
                    error!("EOF received while waiting for response to request id {:?} (method {}). Server stdout closed.", 
                           request_id, method);
                    return Err(anyhow!("Server closed stdout unexpectedly (EOF)"));
                }
                Ok(Ok(n)) => {
                    debug!("Read {} bytes while waiting for response to request id {:?}", n, request_id);
                    
                    // Skip empty lines completely
                    if buffer.trim().is_empty() {
                        continue;
                    }
                    
                    trace!("Raw line received: {:?}", buffer);
                    
                    // Process the non-empty line
                    let line = buffer.trim().to_string();
                    
                    // Try parsing as JSON
                    match serde_json::from_str::<serde_json::Value>(&line) {
                        Ok(json_value) => {
                            // Is this a notification? (has method, no id)
                            if json_value.get("method").is_some() && json_value.get("id").is_none() {
                                debug!("Received notification while waiting for response to method {}", method);
                                
                                // Try to handle the notification
                                if let Ok(notification) = serde_json::from_value::<JsonRpcNotification>(json_value.clone()) {
                                    info!("Processing notification: method={}", notification.method);
                                    
                                    // Clone notification before acquiring lock to minimize lock time
                                    let notification_clone = notification.clone();
                                    
                                    // Use a block to limit the scope of the handler_guard
                                    {
                                        let handler_guard = self.notification_handler.lock().await;
                                        if let Some(handler) = &*handler_guard {
                                            // Call handler within the guard's scope
                                            handler(notification_clone).await;
                                        }
                                    }
                                } else {
                                    warn!("Failed to parse notification JSON: {}", line);
                                }
                                
                                // Continue waiting for our actual response
                                continue;
                            } 
                            // Is this a response? (has id)
                            else if let Some(id) = json_value.get("id") {
                                debug!("Received response with id {:?}, expecting {:?}", id, request_id);
                                
                                // Does this ID match our request?
                                if *id == *request_id {
                                    debug!("ID match found for method {}", method);
                                    
                                    // We found our response, now parse it
                                    drop(stdout_guard); // Release lock before potentially expensive parsing
                                    
                                    match serde_json::from_str::<JsonRpcResponse>(&line) {
                                        Ok(response) => {
                                            info!("Successfully parsed response for method {}", method);
                                            return Ok(response);
                                        }
                                        Err(e) => {
                                            error!("Failed to parse matching response for method {}: {}", method, e);
                                            error!("Raw response that failed parsing: {}", line);
                                            return Err(anyhow!("Failed to parse JSON response: {}. Raw: '{}'", e, line));
                                        }
                                    }
                                } else {
                                    warn!("Received response with non-matching id: expected {:?}, got {:?}", 
                                          request_id, id);
                                    // Continue waiting for our response
                                    continue;
                                }
                            } 
                            // Malformed JSON-RPC message
                            else {
                                warn!("Received malformed JSON-RPC message (no method or id): {}", line);
                                continue;
                            }
                        }
                        Err(e) => {
                            // Failed to parse as JSON - might be incomplete or malformed
                            error!("Failed to parse JSON from line: {} (Error: {})", line, e);
                            
                            // For debugging, let's try to see if this is a valid but partial message
                            if line.starts_with('{') && !line.ends_with('}') {
                                warn!("Possible incomplete JSON detected - missing closing brace");
                            } else if line.contains("}\n{") {
                                warn!("Multiple JSON objects detected in one line - protocol error");
                            }
                            
                            // Continue reading to find a valid response
                            continue;
                        }
                    }
                }
                Ok(Err(e)) => {
                    error!("IO error reading response for method {}: {}", method, e);
                    return Err(anyhow!("IO error reading response: {}", e));
                }
                Err(_) => { // Timeout elapsed
                    error!("Timeout ({:?}s) waiting for response to method {} (id {:?})", 
                           timeout_duration.as_secs(), method, request_id);
                    
                    // Try to read any buffered data for diagnostics
                    let mut debug_buffer = String::new();
                    let debug_timeout = std::time::Duration::from_millis(100);
                    
                    match tokio::time::timeout(debug_timeout, reader.read_to_string(&mut debug_buffer)).await {
                        Ok(Ok(bytes)) if bytes > 0 => {
                            warn!("After timeout, read {} additional bytes: '{}'", bytes, debug_buffer);
                        }
                        _ => {
                            warn!("No additional data available after timeout");
                        }
                    }
                    
                    // Release the lock before returning
                    drop(stdout_guard);
                    
                    return Err(anyhow!(
                        "Timeout after {}s waiting for response to method {} (id {:?})",
                        timeout_duration.as_secs(), method, request_id
                    ));
                }
            }
        }
        
        // This should be unreachable due to the early returns above,
        // but added as a fallback
        error!("Reached end of read_matching_response without finding matching response");
        Err(anyhow!("Failed to find matching response for method {}", method))
    }

    /// Start a background task to listen for and handle JSON-RPC notifications
    async fn start_notification_listener(&self) -> Result<()> {
        // Clone the Arc<Mutex<ChildStdout>> for the listener task
        let stdout_arc = Arc::clone(&self.stdout);
        // Clone the notification handler for the listener task
        let notification_handler_arc = Arc::clone(&self.notification_handler);
        
        // Spawn a Tokio task for the notification listener
        tokio::spawn(async move {
            info!("Notification listener task started");
            
            // Create a BufReader without locking stdout yet
            // We'll lock for each individual read operation
            loop {
                // Lock stdout for the minimum time needed
                let mut stdout_guard = match stdout_arc.lock().await {
                    guard => guard,
                };
                
                let mut reader = BufReader::new(&mut *stdout_guard);
                let mut line = String::new();
                
                // Read a line from stdout
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        info!("Notification listener: EOF received. Server stdout closed.");
                        break;
                    }
                    Ok(n) => {
                        trace!("Notification listener: read {} bytes", n);
                        // Process the line only if it's not empty
                        if line.trim().is_empty() {
                            continue;
                        }
                        
                        // Release the stdout lock before processing to minimize lock time
                        drop(stdout_guard);
                        
                        // Parse the line as a JSON-RPC message
                        match serde_json::from_str::<serde_json::Value>(&line) {
                            Ok(json_value) => {
                                // Check if this is a notification (has method, no id)
                                // or a response (has id)
                                if json_value.get("method").is_some() && json_value.get("id").is_none() {
                                    // This is a notification
                                    match serde_json::from_value::<JsonRpcNotification>(json_value) {
                                        Ok(notification) => {
                                            info!("Received notification: method={}", notification.method);
                                            
                                            // Clone the notification for the handler
                                            let notification_clone = notification.clone();
                                            
                                            // Use a block to limit the scope of the handler_guard
                                            {
                                                let handler_guard = notification_handler_arc.lock().await;
                                                if let Some(handler) = &*handler_guard {
                                                    // Call the notification handler within the guard's scope
                                                    handler(notification_clone).await;
                                                } else {
                                                    debug!("No notification handler registered, notification ignored");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to parse notification: {}", e);
                                            error!("Raw notification data: {}", line.trim());
                                        }
                                    }
                                } else {
                                    // This is a response or malformed message, will be handled by send_request
                                    trace!("Ignored non-notification message: {}", line.trim());
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse JSON-RPC message: {}", e);
                                error!("Raw message data: {}", line.trim());
                            }
                        }
                    }
                    Err(e) => {
                        error!("Notification listener: Error reading from stdout: {}", e);
                        break;
                    }
                }
            }
            
            info!("Notification listener task ended");
        });
        
        info!("Notification listener task spawned");
        Ok(())
    }
}

#[async_trait]
impl Transport for ProcessTransport {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        // Define constants for better readability
        const REQUEST_TIMEOUT_SECS: u64 = 60;
        // Removed unused EMERGENCY_READ_TIMEOUT_MS

        // Ensure our request ends with a proper newline
        // In JSON-RPC over stdin/stdout, each message must be a single line ending with \n
        let mut request_str = serde_json::to_string(&request)?;
        if !request_str.ends_with('\n') {
            request_str.push('\n');
        }
        
        trace!("Sending raw request: {}", request_str.trim());
        info!("Sending request method: {}, id: {:?}", request.method, request.id);

        // Send the request with proper message boundary
        {
            let mut stdin_guard = self.stdin.lock().await;
            debug!("Writing request to stdin (method: {})", request.method);
            stdin_guard.write_all(request_str.as_bytes()).await?;
            
            // Explicitly flush to ensure the message is sent immediately
            debug!("Flushing stdin after writing request");
            stdin_guard.flush().await?;
        }
        debug!("Request sent and stdin flushed (method: {})", request.method);

        // Prepare for reading the response
        let timeout_duration = std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS);
        let request_id = request.id.clone(); // Store the request ID for matching responses

        // Use a separate function to handle the response reading with proper buffering
        let response = self.read_matching_response(
            &request.method,
            &request_id,
            timeout_duration
        ).await?;

        // Log the successfully parsed response 
        info!("Successfully parsed response for request id {:?}. Response ID: {:?}. Has result: {}, Has error: {}",
              request.id, response.id, response.result.is_some(), response.error.is_some());
        debug!("Parsed response details: {:?}", response);

        // Strict ID check - return error if mismatch.
        // Allow Null ID response if request ID was Null (for notifications treated as requests)
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
        // Ensure the notification string ends with a newline
        let mut notification_str = serde_json::to_string(&notification)?;
        if !notification_str.ends_with('\n') {
            notification_str.push('\n');
        }
        
        trace!("Sending raw notification: {}", notification_str.trim());
        info!("Sending notification method: {}", notification.method);

        // Lock stdin, write notification, and flush
        {
            let mut stdin_guard = self.stdin.lock().await;
            debug!("Writing notification to stdin (method: {})", notification.method);
            stdin_guard.write_all(notification_str.as_bytes()).await?;
            
            debug!("Flushing stdin after writing notification");
            stdin_guard.flush().await?;
        }
        debug!("Notification sent and stdin flushed (method: {})", notification.method);

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
