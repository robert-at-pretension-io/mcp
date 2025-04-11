#![cfg(feature = "sse_server")] // Only compile this module if sse_server feature is enabled

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{sse::Event, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::{stream::Stream, StreamExt};
use serde_json::{json, Value};
use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::sync::{broadcast, Mutex}; // Use tokio::sync::Mutex for async handler
use tokio_stream::wrappers::BroadcastStream;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

use crate::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

// Define the handler type for incoming requests (POST)
type RequestHandler =
    Box<dyn Fn(JsonRpcRequest) -> Result<Option<JsonRpcResponse>> + Send + Sync + 'static>;

// Shared state for the Axum server
#[derive(Clone)]
struct ServerState {
    notification_tx: broadcast::Sender<String>, // For sending notifications via SSE
    request_handler: Arc<Mutex<Option<RequestHandler>>>, // Handler for POST requests
}

pub struct SSEServerTransport {
    port: u16,
    notification_tx: broadcast::Sender<String>,
    request_handler: Arc<Mutex<Option<RequestHandler>>>,
    server_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl SSEServerTransport {
    pub fn new(port: u16) -> Self {
        let (notification_tx, _) = broadcast::channel(100); // Channel for SSE notifications

        Self {
            port,
            notification_tx,
            request_handler: Arc::new(Mutex::new(None)),
            server_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the handler for incoming JSON-RPC requests received via POST.
    pub fn set_request_handler(&mut self, handler: RequestHandler) {
        let mut guard = self.request_handler.blocking_lock(); // Use blocking lock during setup
        *guard = Some(handler);
    }

    /// Start the Axum server to handle SSE and POST requests.
    pub async fn start(&mut self) -> Result<()> {
        if self.server_handle.lock().await.is_some() {
            warn!("SSE Server already started.");
            return Ok(());
        }

        let state = ServerState {
            notification_tx: self.notification_tx.clone(),
            request_handler: self.request_handler.clone(),
        };

        let app = Router::new()
            .route("/sse", get(handle_sse_connection)) // SSE endpoint
            .route("/", post(handle_post_request)) // POST endpoint for requests/notifications
            .layer(TraceLayer::new_for_http())
            .with_state(state);

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port)); // Bind to all interfaces
        info!("Starting SSE/POST server on {}", addr);

        let server = axum::Server::bind(&addr).serve(app.into_make_service());

        let handle = tokio::spawn(async move {
            if let Err(e) = server.await {
                error!("SSE server error: {}", e);
            }
        });

        *self.server_handle.lock().await = Some(handle);
        Ok(())
    }

    /// Send a JSON-RPC notification to all connected SSE clients.
    pub fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        let message_str = serde_json::to_string(&notification)?;
        match self.notification_tx.send(message_str) {
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("Failed to send notification (no active SSE subscribers?): {}", e);
                // Don't error out if no one is listening
                Ok(())
            }
        }
    }

    /// Stop the Axum server.
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.server_handle.lock().await.take() {
            info!("Stopping SSE server...");
            handle.abort();
            // Optionally wait for the handle to ensure shutdown
            // let _ = handle.await;
            info!("SSE server stopped.");
        } else {
            warn!("SSE Server not running or already stopped.");
        }
        Ok(())
    }
}

// Axum handler for establishing the SSE connection
async fn handle_sse_connection(
    State(state): State<ServerState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("New SSE client connected.");
    let rx = state.notification_tx.subscribe();

    // Map broadcast messages to SSE events
    let stream = BroadcastStream::new(rx)
        .map(|msg_result| match msg_result {
            Ok(data) => {
                info!("Sending SSE event: {}", data);
                Ok(Event::default().data(data))
            }
            Err(e) => {
                // This can happen if the receiver lags too far behind
                warn!("SSE broadcast receiver lagged: {}", e);
                // Send an error event or just skip
                Ok(Event::default()
                    .event("error")
                    .data("Receiver lagged".to_string()))
            }
        })
        .filter_map(|res| async { res.ok() }); // Filter out errors for Infallible stream

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive-text"),
    )
}

// Axum handler for incoming POST requests (JSON-RPC requests/notifications)
async fn handle_post_request(
    State(state): State<ServerState>,
    headers: HeaderMap, // Can be used for auth etc.
    Json(body): Json<Value>,
) -> impl IntoResponse {
    info!("Received POST request: {}", body);
    let handler_guard = state.request_handler.lock().await;
    if let Some(handler) = handler_guard.as_ref() {
        // Attempt to parse as a request first
        match serde_json::from_value::<JsonRpcRequest>(body.clone()) {
            Ok(request) => {
                // It's a request, call the handler
                match handler(request) {
                    Ok(Some(response)) => {
                        info!("Sending response: {:?}", response);
                        (StatusCode::OK, Json(response))
                    }
                    Ok(None) => {
                        // Handler processed it but no response needed (e.g., notification handled)
                        info!("Request handled, no response needed.");
                        (StatusCode::NO_CONTENT, Json(Value::Null)) // Or OK with empty body
                    }
                    Err(e) => {
                        error!("Request handler error: {}", e);
                        let error_response = json!({
                            "jsonrpc": "2.0",
                            "error": {"code": INTERNAL_ERROR, "message": format!("Internal server error: {}", e)},
                            "id": body.get("id").cloned().unwrap_or(Value::Null) // Try to preserve ID
                        });
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(error_response))
                    }
                }
            }
            Err(_) => {
                // If not a request, check if it's a notification
                match serde_json::from_value::<JsonRpcNotification>(body.clone()) {
                    Ok(notification) => {
                        // It's a notification, try to handle it (handler might ignore it)
                        // We wrap it in a dummy request structure if the handler expects requests
                        let dummy_request = JsonRpcRequest {
                            jsonrpc: "2.0".to_string(),
                            method: notification.method,
                            params: Some(notification.params),
                            id: Value::Null, // Notifications don't have IDs in the same way
                        };
                        match handler(dummy_request) {
                             Ok(_) => {
                                  info!("Notification handled successfully.");
                                  (StatusCode::NO_CONTENT, Json(Value::Null))
                             },
                             Err(e) => {
                                  error!("Notification handler error: {}", e);
                                  // Don't send error response for notifications
                                  (StatusCode::INTERNAL_SERVER_ERROR, Json(Value::Null))
                             }
                        }
                    }
                    Err(_) => {
                        // Invalid JSON-RPC message
                        warn!("Received invalid JSON-RPC message via POST: {}", body);
                        let error_response = json!({
                            "jsonrpc": "2.0",
                            "error": {"code": PARSE_ERROR, "message": "Invalid JSON-RPC message"},
                            "id": null
                        });
                        (StatusCode::BAD_REQUEST, Json(error_response))
                    }
                }
            }
        }
    } else {
        error!("No request handler configured on the server.");
        let error_response = json!({
            "jsonrpc": "2.0",
            "error": {"code": INTERNAL_ERROR, "message": "Server not configured to handle requests"},
            "id": body.get("id").cloned().unwrap_or(Value::Null)
        });
        (StatusCode::INTERNAL_SERVER_ERROR, Json(error_response))
    }
}
