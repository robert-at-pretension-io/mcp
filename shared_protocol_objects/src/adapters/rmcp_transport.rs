use crate::rpc::{Transport, NotificationHandler};
use crate::{JsonRpcRequest, JsonRpcResponse, JsonRpcNotification};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::transport::{TokioChildProcess, IntoTransport};
use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage, RequestId, NumberOrString};
use rmcp::{Service as SdkService, RoleClient, serve_client}; // Use SDK Service
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::process::Command; // Use tokio::process::Command
use std::time::Duration;

// Import the protocol adapter for conversions
use super::rmcp_protocol::RmcpProtocolAdapter;
// Import telemetry helpers
use super::telemetry::{increment_request_count, increment_error_count, increment_notification_count, RequestTimer};

/// Adapter that wraps the RMCP SDK Service implementation (Based on Section 2.5 of the new guide)
/// This adapter implements our `Transport` trait but uses the higher-level `rmcp::Service` internally.
pub struct RmcpTransportAdapter {
    // Use the SDK's Service directly
    inner: Arc<SdkService<RoleClient>>,
    // Counter for generating request IDs if our original request doesn't have one
    // or if we need to generate IDs for SDK calls that don't map 1:1 to our requests.
    // Note: The guide suggests using u32 for Number IDs.
    request_id_counter: AtomicU32,
    // Store the notification handler provided by our application
    notification_handler: Arc<tokio::sync::Mutex<Option<NotificationHandler>>>,
    // Task handle for the background notification listener - Removed as storing it back is problematic
    // _notification_listener_handle: Option<tokio::task::JoinHandle<()>>,
    // Store timeout duration
    request_timeout: Duration,
}

impl RmcpTransportAdapter {
    /// Create a new adapter using the SDK's Service.
    pub async fn new(mut command: Command) -> Result<Self> {
        Self::new_with_timeout(command, Duration::from_secs(30)).await // Default timeout
    }

     /// Create a new adapter using the SDK's Service with a specific timeout.
    pub async fn new_with_timeout(mut command: Command, request_timeout: Duration) -> Result<Self> {
        tracing::debug!("Creating RmcpTransportAdapter (wrapping SDK Service) for command: {:?}", command);
        // Create the transport using the SDK
        let transport = TokioChildProcess::new(&mut command)?
            .into_transport()?;

        // Create the service using the SDK's serve_client function
        // This likely handles initialization internally.
        let service = serve_client(transport).await?;
        let service_arc = Arc::new(service);

        Ok(Self {
            inner: service_arc,
            request_id_counter: AtomicU32::new(1), // Start counter at 1
            notification_handler: Arc::new(tokio::sync::Mutex::new(None)),
            // _notification_listener_handle: None, // Removed field
            request_timeout,
        })
    }

    /// Generate the next request ID according to the guide's suggestion (u32).
    fn next_request_id(&self) -> RequestId {
        let id = self.request_id_counter.fetch_add(1, Ordering::SeqCst);
        NumberOrString::Number(id)
    }

    /// Helper to map SDK errors to anyhow::Error
    fn map_sdk_error<E: std::fmt::Display>(err: E) -> anyhow::Error {
        increment_error_count(); // Increment telemetry on error
        anyhow!("RMCP SDK error: {}", err)
    }
}

// Implement our Transport trait for the adapter
#[async_trait]
impl Transport for RmcpTransportAdapter {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let method_name = request.method.clone();
        let timer = RequestTimer::new(&method_name);
        increment_request_count();

        // Convert our request to the SDK's specific ClientJsonRpcMessage variant
        // This requires knowing which SDK Service method corresponds to our request method.
        let sdk_request = RmcpProtocolAdapter::to_sdk_request(&request)?;

        // Use the SDK service to send the request.
        // This part needs careful mapping from our generic request to specific SDK service methods.
        // The guide's example is limited. We need to handle different methods.
        let sdk_response_result = match sdk_request {
            ClientJsonRpcMessage::Initialize(init_params) => {
                // The guide suggests `serve_client` handles initialization.
                // If explicit initialization is needed *after* service creation,
                // it might look like this, but the SDK service API needs confirmation.
                // let sdk_init_result = self.inner.initialize(
                //     init_params.protocol_version,
                //     init_params.client_info,
                //     init_params.capabilities,
                //     // other params...
                // ).await.map_err(Self::map_sdk_error)?;
                // Ok(ServerJsonRpcMessage::InitializeResult(sdk_init_result))

                // For now, assume initialization was implicit or handle error if called explicitly post-creation
                 Err(anyhow!("Explicit 'initialize' request not supported via SDK Service adapter after creation"))

            },
            ClientJsonRpcMessage::ListTools(list_params) => {
                 // Map our ListToolsParams (if any) to SDK ListToolsParams
                 let sdk_list_params = rmcp::model::ListToolsParams {
                     cursor: list_params.cursor, // Pass cursor if present
                     // Map other params if needed
                 };
                 let sdk_list_result = self.inner.list_tools(sdk_list_params)
                     .await
                     .map_err(Self::map_sdk_error)?;
                 // Wrap the SDK result type in the ServerJsonRpcMessage enum variant
                 Ok(ServerJsonRpcMessage::ListToolsResult(sdk_list_result))
            },
            ClientJsonRpcMessage::CallTool(call_params) => {
                 // Map our CallToolParams to SDK CallToolParams
                 let sdk_call_params = rmcp::model::CallToolParams {
                     name: call_params.name,
                     arguments: call_params.arguments,
                     // Map other params if needed
                 };
                 let sdk_call_result = self.inner.call_tool(sdk_call_params)
                     .await
                     .map_err(Self::map_sdk_error)?;
                 Ok(ServerJsonRpcMessage::CallToolResult(sdk_call_result))
            },
            ClientJsonRpcMessage::Request(generic_req) => {
                 // Handle generic requests if the SDK Service supports them
                 // This might involve a method like `self.inner.request(method, params)`
                 // The SDK API needs confirmation for generic request handling via Service.
                 // For now, return an error.
                 Err(anyhow!("Generic request method '{}' not directly supported via SDK Service adapter", generic_req.method))
            }
            // Handle other specific ClientJsonRpcMessage variants (Shutdown, Exit, NotificationsInitialized) if they are sent via send_request
            ClientJsonRpcMessage::Shutdown => {
                // Shutdown is likely handled by `close()` method, not send_request
                Err(anyhow!("'shutdown' should be sent via close(), not send_request()"))
            }
             ClientJsonRpcMessage::Exit => {
                // Exit is likely handled by `close()`, not send_request
                 Err(anyhow!("'exit' should be sent via close(), not send_request()"))
             }
             ClientJsonRpcMessage::NotificationsInitialized => {
                 // This is usually sent as a notification, not a request expecting a response
                 Err(anyhow!("'notifications/initialized' should be sent via send_notification()"))
             }
            // Notifications should not be sent via send_request
            ClientJsonRpcMessage::Notification(_) => Err(anyhow!("Cannot send notification via send_request")),
        };

        match sdk_response_result {
            Ok(sdk_response) => {
                // Convert the SDK ServerJsonRpcMessage response back to our JsonRpcResponse
                let response = RmcpProtocolAdapter::from_sdk_response(sdk_response)?;
                timer.finish();
                Ok(response)
            }
            Err(e) => {
                increment_error_count(); // Ensure error is counted
                timer.finish();
                Err(e) // Propagate the error
            }
        }
    }

    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        let method_name = notification.method.clone();
        let timer = RequestTimer::new(&method_name); // Time notifications too if desired
        // Don't increment request count for notifications, use notification count
        increment_notification_count();

        // Convert our notification to the SDK's ClientJsonRpcMessage format
        let sdk_notification_msg = RmcpProtocolAdapter::to_sdk_notification(&notification)?;

        // Use the SDK service to send the notification.
        // The SDK Service API needs confirmation on how it handles sending different notification types.
        match sdk_notification_msg {
             ClientJsonRpcMessage::NotificationsInitialized => {
                 // The SDK might handle this implicitly during initialization or have a specific method.
                 // Assuming `self.inner.notifications_initialized()` exists for demonstration.
                 // self.inner.notifications_initialized().await.map_err(Self::map_sdk_error)?;
                 tracing::debug!("SDK likely handles 'notifications/initialized' implicitly or via initialize call.");
                 Ok(())
             },
             ClientJsonRpcMessage::Notification(sdk_notification) => {
                 // Assuming a generic `self.inner.notification()` method exists.
                 self.inner.notification(sdk_notification.method, sdk_notification.params)
                     .await
                     .map_err(Self::map_sdk_error)?;
                 timer.finish();
                 Ok(())
             },
             ClientJsonRpcMessage::Exit => {
                 // Exit notification might be handled by `close()` or a specific method.
                 // Assuming `self.inner.exit()` exists for demonstration.
                 self.inner.exit().await.map_err(Self::map_sdk_error)?;
                 timer.finish();
                 Ok(())
             }
             // Other variants like Initialize, ListTools, CallTool, Request, Shutdown are not notifications
             _ => {
                 increment_error_count(); // Count this as an error
                 timer.finish();
                 Err(anyhow!("Unsupported message type passed to send_notification: {:?}", sdk_notification_msg))
             }
        }
    }

    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()> {
        tracing::debug!("Subscribing to notifications using SDK Service adapter");
        // Store the handler
        {
            let mut handler_guard = self.notification_handler.lock().await;
            if handler_guard.is_some() {
                 tracing::warn!("Overwriting existing notification handler during subscribe.");
            }
            *handler_guard = Some(handler.clone());
        } // Lock released

        // Spawn a task to listen for notifications from the SDK Service.
        // NOTE: This assumes the SDK Service allows multiple listeners or handles
        // registration idempotently if called multiple times. If not, we might need
        // to ensure this is only called once per adapter instance.
        // This requires the SDK Service to provide a way to receive notifications,
        // e.g., a stream or a callback registration mechanism.
        // Assuming `self.inner.on_notification()` exists and takes a callback.

        let service_clone = self.inner.clone();
        let handler_arc = self.notification_handler.clone();

        let handle = tokio::spawn(async move {
            // Example: Using a hypothetical on_notification callback registration
            let notification_callback = Box::new(move |sdk_notification: rmcp::model::Notification| {
                let handler_clone = handler_arc.clone(); // Clone Arc for async block
                tokio::spawn(async move { // Spawn task for each notification to avoid blocking listener
                    if let Some(handler) = handler_clone.lock().await.as_ref() {
                         match RmcpProtocolAdapter::from_sdk_notification(sdk_notification) {
                             Ok(our_notification) => {
                                 // Call the application's handler
                                 (handler)(our_notification).await;
                             }
                             Err(e) => {
                                 tracing::error!("Failed to convert SDK notification: {}", e);
                             }
                         }
                    }
                });
            });

            // Register the callback with the SDK service
            // This is hypothetical - replace with actual SDK API
            if let Err(e) = service_clone.on_notification(notification_callback).await {
                 tracing::error!("Failed to subscribe to SDK notifications: {}", e);
            } else {
                 tracing::info!("SDK Notification listener started.");
                 // Keep the listener alive (the await above might block or return)
                 // If on_notification returns immediately, we need another way to keep listening,
                 // maybe a stream:
                 // let mut stream = service_clone.notification_stream().await?;
                 // while let Some(sdk_notification) = stream.next().await {
                 //    // ... call handler ...
                 // }
                 // tracing::info!("SDK Notification listener finished.");
            } else {
                 tracing::info!("SDK Notification listener task finished.");
                 // Optional: Handle cleanup or notify if the listener stops unexpectedly.
            }
        });

        // We don't store the handle in self anymore.
        // If explicit cancellation of the listener is needed later,
        // the adapter design would need adjustment (e.g., using a shared cancellation token).
        drop(handle); // Avoid unused variable warning if handle isn't used

        Ok(())
    }

    async fn close(&self) -> Result<()> {
        let timer = RequestTimer::new("close");
        increment_request_count(); // Count close as a request-like action
        tracing::info!("Closing SDK Service adapter (calling cancel)");
        // Use the SDK service to close the connection (sends shutdown/exit)
        self.inner.cancel()
            .await
            .map_err(Self::map_sdk_error)?;
        timer.finish();
        Ok(())
    }
}
