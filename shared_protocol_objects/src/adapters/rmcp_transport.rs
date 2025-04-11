use crate::rpc::{Transport, NotificationHandler}; // Use NotificationHandler from rpc module
use crate::{JsonRpcRequest, JsonRpcResponse, JsonRpcNotification};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::transport::{TokioChildProcess, IntoTransport};
use std::sync::Arc;
use tokio::sync::Mutex;
use futures::{Sink, Stream, SinkExt, StreamExt};
use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage, Id};
use std::collections::HashMap;
use tokio::sync::oneshot;
use std::time::Duration; // Added for timeout

// Import the protocol adapter for conversions
use super::rmcp_protocol::RmcpProtocolAdapter;
// Import telemetry helpers
use super::telemetry::{increment_request_count, increment_error_count, increment_notification_count, RequestTimer};


/// Adapter that wraps the RMCP TokioChildProcess implementation
pub struct RmcpProcessTransportAdapter {
    // Keep the process handle for potential explicit shutdown if needed later
    #[allow(dead_code)]
    inner_process: Arc<Mutex<TokioChildProcess>>,
    // Channel sender to send messages to the process sink task
    to_process_tx: Arc<Mutex<futures::channel::mpsc::Sender<ClientJsonRpcMessage>>>,
    // Shared notification handler
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,
    // Map to store pending requests (Request ID -> Response Sender)
    pending_requests: Arc<Mutex<HashMap<Id, oneshot::Sender<Result<ServerJsonRpcMessage>>>>>,
    // Keep task handles for cleanup/shutdown
    _sink_task: Arc<tokio::task::JoinHandle<()>>,
    _stream_task: Arc<tokio::task::JoinHandle<()>>,
    // Default timeout for requests
    request_timeout: Duration,
}

impl RmcpProcessTransportAdapter {
    pub async fn new(command: &mut tokio::process::Command) -> Result<Self> {
        Self::new_with_timeout(command, Duration::from_secs(30)).await // Default timeout
    }

    pub async fn new_with_timeout(command: &mut tokio::process::Command, request_timeout: Duration) -> Result<Self> {
        tracing::debug!("Creating RmcpProcessTransportAdapter for command: {:?}", command);
        // Create the TokioChildProcess from the SDK
        let process = TokioChildProcess::new(command)
            .map_err(|e| anyhow!("Failed to create TokioChildProcess: {}", e))?;

        // Create a channel for sending messages to the process sink task
        let (to_process_tx, mut to_process_rx) = futures::channel::mpsc::channel::<ClientJsonRpcMessage>(32);

        // Get the transport from the process
        let transport = process.into_transport()
            .map_err(|e| anyhow!("Failed to create transport from process: {}", e))?;

        // Split the transport into sink and stream
        let (mut sink, mut stream) = transport.split();

        // Create shared state
        let notification_handler = Arc::new(Mutex::new(None));
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));

        // Clone Arcs for tasks
        let nh_clone = notification_handler.clone();
        let pending_requests_clone = pending_requests.clone();

        // Spawn a task to forward messages from the channel to the actual process sink
        let sink_task = tokio::spawn(async move {
            while let Some(msg) = to_process_rx.next().await {
                tracing::trace!("Forwarding message to process sink: {:?}", msg);
                if let Err(e) = sink.send(msg).await {
                    tracing::error!("Error sending message to process sink: {}", e);
                    // Consider closing the channel or signaling an error state
                    break;
                }
            }
            tracing::info!("Process sink task finished.");
        });

        // Spawn a task to receive messages from the stream, handle notifications, and route responses
        let stream_task = tokio::spawn(async move {
            while let Some(message_result) = stream.next().await {
                match message_result {
                    Ok(msg) => {
                        tracing::trace!("Received message from process stream: {:?}", msg);
                        match msg {
                            ServerJsonRpcMessage::Notification(notification) => {
                                increment_notification_count(); // Increment telemetry counter
                                if let Some(handler) = nh_clone.lock().await.as_ref() {
                                    // Convert SDK notification to our format
                                    let our_notification = RmcpProtocolAdapter::from_sdk_notification(notification);
                                    // Call the handler (fire and forget)
                                    let handler_clone = handler.clone(); // Clone Arc for async move
                                    tokio::spawn(async move {
                                        handler_clone(our_notification).await;
                                    });
                                } else {
                                    tracing::warn!("Received notification but no handler registered.");
                                }
                            },
                            ServerJsonRpcMessage::InitializeResult(res) => {
                                let id = res.id.clone();
                                if let Some(tx) = pending_requests_clone.lock().await.remove(&id) {
                                    if tx.send(Ok(ServerJsonRpcMessage::InitializeResult(res))).is_err() {
                                        tracing::warn!("Failed to send InitializeResult response to waiting task (receiver dropped). ID: {:?}", id);
                                    }
                                } else {
                                     tracing::warn!("Received InitializeResult for unknown/timed-out request ID: {:?}", id);
                                }
                            },
                            ServerJsonRpcMessage::ListToolsResult(res) => {
                                let id = res.id.clone();
                                if let Some(tx) = pending_requests_clone.lock().await.remove(&id) {
                                     if tx.send(Ok(ServerJsonRpcMessage::ListToolsResult(res))).is_err() {
                                         tracing::warn!("Failed to send ListToolsResult response to waiting task (receiver dropped). ID: {:?}", id);
                                     }
                                } else {
                                     tracing::warn!("Received ListToolsResult for unknown/timed-out request ID: {:?}", id);
                                }
                            },
                            ServerJsonRpcMessage::CallToolResult(res) => {
                                let id = res.id.clone();
                                if let Some(tx) = pending_requests_clone.lock().await.remove(&id) {
                                     if tx.send(Ok(ServerJsonRpcMessage::CallToolResult(res))).is_err() {
                                         tracing::warn!("Failed to send CallToolResult response to waiting task (receiver dropped). ID: {:?}", id);
                                     }
                                } else {
                                     tracing::warn!("Received CallToolResult for unknown/timed-out request ID: {:?}", id);
                                }
                            },
                            ServerJsonRpcMessage::Error(err_res) => {
                                let id = err_res.id.clone();
                                if let Some(tx) = pending_requests_clone.lock().await.remove(&id) {
                                     // Wrap the error response in Ok, as the oneshot expects Result<ServerJsonRpcMessage>
                                     if tx.send(Ok(ServerJsonRpcMessage::Error(err_res))).is_err() {
                                         tracing::warn!("Failed to send Error response to waiting task (receiver dropped). ID: {:?}", id);
                                     }
                                } else {
                                     tracing::warn!("Received Error response for unknown/timed-out request ID: {:?}", id);
                                }
                            },
                             // Handle other potential response types if the SDK adds more
                             _ => {
                                 tracing::warn!("Received unhandled SDK response type: {:?}", msg);
                                 // Attempt to extract ID and notify pending request if possible, or log error
                                 // This part needs careful handling based on SDK specifics
                             }
                        }
                    },
                    Err(e) => {
                        tracing::error!("Error receiving message from process stream: {}", e);
                        // Propagate error to all pending requests
                        let mut pending = pending_requests_clone.lock().await;
                        for (_, tx) in pending.drain() {
                            let _ = tx.send(Err(anyhow!("Transport stream error: {}", e)));
                        }
                        break; // Exit the loop on stream error
                    }
                }
            }
            tracing::info!("Process stream task finished.");
            // Ensure pending requests are cleaned up if the stream closes unexpectedly
            let mut pending = pending_requests_clone.lock().await;
             for (_, tx) in pending.drain() {
                 let _ = tx.send(Err(anyhow!("Transport stream closed unexpectedly")));
             }
        });

        Ok(Self {
            inner_process: Arc::new(Mutex::new(process)), // Keep the process handle
            to_process_tx: Arc::new(Mutex::new(to_process_tx)),
            notification_handler,
            pending_requests,
            _sink_task: Arc::new(sink_task),
            _stream_task: Arc::new(stream_task),
            request_timeout,
        })
    }

    // Note: adapt_protocol_version is removed as version handling should be in protocol layer or client logic
}

// Implement our Transport trait using the SDK adapter
#[async_trait]
impl Transport for RmcpProcessTransportAdapter {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let method_name = request.method.clone(); // For telemetry timer
        let timer = RequestTimer::new(&method_name); // Start telemetry timer
        increment_request_count(); // Increment telemetry counter

        // Convert our request to SDK format using the protocol adapter
        let sdk_request = RmcpProtocolAdapter::to_sdk_request(&request)?; // Use Result for potential conversion errors

        // Extract the ID for matching the response
        let request_id = match &sdk_request {
            ClientJsonRpcMessage::Initialize(r) => r.id.clone(),
            ClientJsonRpcMessage::ListTools(r) => r.id.clone(),
            ClientJsonRpcMessage::CallTool(r) => r.id.clone(),
            ClientJsonRpcMessage::Request(r) => r.id.clone(),
            _ => return Err(anyhow!("Cannot send non-request message type via send_request")),
        };

        // Create a oneshot channel for the response
        let (response_tx, response_rx) = oneshot::channel::<Result<ServerJsonRpcMessage>>();

        // Store the sender in the pending requests map
        { // Scope for mutex guard
            let mut pending = self.pending_requests.lock().await;
            if pending.insert(request_id.clone(), response_tx).is_some() {
                // This should ideally not happen with unique IDs, but handle defensively
                increment_error_count(); // Increment error counter
                timer.finish(); // Finish timer
                return Err(anyhow!("Duplicate request ID detected: {:?}", request_id));
            }
        } // Mutex guard dropped here

        // Send the request via the channel to the sink task
        if let Err(e) = self.to_process_tx.lock().await.send(sdk_request).await {
            increment_error_count(); // Increment error counter
            // Remove the pending request if sending failed
            self.pending_requests.lock().await.remove(&request_id);
            timer.finish(); // Finish timer
            return Err(anyhow!("Failed to send request to process sink task: {}", e));
        }

        // Wait for the response with a timeout
        match tokio::time::timeout(self.request_timeout, response_rx).await {
            Ok(Ok(Ok(sdk_response))) => {
                // Convert SDK response back to our format
                let our_response = RmcpProtocolAdapter::from_sdk_response(sdk_response)?;
                timer.finish(); // Finish timer successfully
                Ok(our_response)
            },
            Ok(Ok(Err(e))) => { // Error propagated from stream task
                increment_error_count();
                timer.finish();
                Err(anyhow!("Error received from transport stream: {}", e))
            }
            Ok(Err(_)) => { // oneshot channel closed/dropped
                increment_error_count();
                // The sender was dropped, likely because the stream task exited.
                // The pending request should have been cleaned up there.
                timer.finish();
                Err(anyhow!("Response channel closed unexpectedly for request ID: {:?}", request_id))
            }
            Err(_) => { // Timeout elapsed
                increment_error_count();
                // Remove the pending request on timeout
                self.pending_requests.lock().await.remove(&request_id);
                timer.finish();
                Err(anyhow!("Request timed out after {:?} for ID: {:?}", self.request_timeout, request_id))
            }
        }
    }

    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        // Convert our notification to SDK format using the protocol adapter
        let sdk_notification = RmcpProtocolAdapter::to_sdk_notification(&notification)?; // Use Result

        // Send the notification via the channel to the sink task
        let mut sink = self.to_process_tx.lock().await;
        sink.send(sdk_notification).await
            .map_err(|e| {
                increment_error_count(); // Increment error counter on failure
                anyhow!("Failed to send notification to process sink task: {}", e)
            })?;

        Ok(())
    }

    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()> {
        tracing::debug!("Subscribing to notifications");
        let mut guard = self.notification_handler.lock().await;
        if guard.is_some() {
            tracing::warn!("Overwriting existing notification handler.");
        }
        *guard = Some(handler);
        Ok(())
    }

    // Optional: Implement a close method if needed for graceful shutdown
    // async fn close(&self) -> Result<()> {
    //     // Signal tasks to shut down, close the process, etc.
    //     // Might involve dropping the sender channel or sending a specific shutdown signal.
    //     // Aborting tasks might be necessary if they don't exit cleanly.
    //     // self._sink_task.abort();
    //     // self._stream_task.abort();
    //     // let mut process = self.inner_process.lock().await;
    //     // process.kill().await?; // Or a cleaner shutdown if SDK provides one
    //     tracing::info!("RmcpProcessTransportAdapter closed");
    //     Ok(())
    // }
}

// Note: Conversion functions are now expected to be part of RmcpProtocolAdapter
// and are removed from here.
