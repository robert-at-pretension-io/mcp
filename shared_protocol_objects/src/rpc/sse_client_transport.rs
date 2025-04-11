use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::stream::StreamExt;
use reqwest::{Client as HttpClient, RequestBuilder, StatusCode};
use reqwest_eventsource::{Event, EventSource};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{self, Sender};
use tracing::{error, info, warn};

use crate::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use super::{NotificationHandler, Transport};

#[derive(Clone)] // Added Clone derive
pub struct SSEClientTransport {
    url: String,
    http_client: HttpClient,
    headers: Arc<Mutex<HashMap<String, String>>>,
    event_stream: Arc<Mutex<Option<EventSource>>>,
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,
    is_running: Arc<Mutex<bool>>,
    reconnect_delay: Duration,
}

impl SSEClientTransport {
    pub fn new(url: String) -> Self {
        // Create an HTTP client with keep-alive settings
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(300)) // 5 minute timeout for requests
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(10)
            .tcp_keepalive(Duration::from_secs(60))
            .http2_keep_alive_interval(Duration::from_secs(30))
            .http2_keep_alive_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|e| {
                warn!("Failed to build optimized HTTP client: {}, using default.", e);
                HttpClient::new()
            });

        Self {
            url,
            http_client,
            headers: Arc::new(Mutex::new(HashMap::new())),
            event_stream: Arc::new(Mutex::new(None)),
            notification_handler: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            reconnect_delay: Duration::from_secs(5),
        }
    }

    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        let mut h = self.headers.lock().unwrap();
        *h = headers;
    }

    pub fn add_header(&mut self, key: &str, value: &str) {
        let mut h = self.headers.lock().unwrap();
        h.insert(key.to_string(), value.to_string());
    }

    async fn create_event_source(&self) -> Result<EventSource> {
        let headers = self.headers.lock().unwrap().clone(); // Clone headers for builder
        let mut builder = EventSource::builder(self.url.parse()?); // Use builder method

        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }

        Ok(builder.build())
    }

    async fn send_post_request(&self, body: Value) -> Result<Value> {
        let mut request_builder = self.http_client.post(&self.url);

        let headers = self.headers.lock().unwrap();
        for (k, v) in headers.iter() {
            request_builder = request_builder.header(k, v);
        }

        let response = request_builder.json(&body).send().await?;

        if response.status() != StatusCode::OK {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Server responded with status: {}. Body: {}",
                response.status(),
                error_text
            ));
        }

        let response_body = response.json::<Value>().await?;
        Ok(response_body)
    }

    async fn start_listening_task(self) {
        let mut delay = self.reconnect_delay;
        loop {
            let is_running = *self.is_running.lock().unwrap();
            if !is_running {
                info!("SSE listening task stopping.");
                break;
            }

            info!("Attempting to connect to SSE stream at {}", self.url);
            match self.create_event_source().await {
                Ok(mut es) => {
                    info!("SSE stream connected.");
                    delay = self.reconnect_delay; // Reset delay on successful connection

                    while let Some(event) = es.next().await {
                         let is_still_running = *self.is_running.lock().unwrap();
                         if !is_still_running {
                              info!("SSE connection closing due to transport stop request.");
                              es.close();
                              break;
                         }
                        match event {
                            Ok(Event::Open) => {
                                info!("SSE stream opened.");
                            }
                            Ok(Event::Message(msg)) => {
                                info!("Received SSE message: {}", msg.data);
                                match serde_json::from_str::<Value>(&msg.data) {
                                    Ok(json_msg) => {
                                        // Check if it's a notification
                                        if json_msg.get("method").is_some() && json_msg.get("id").is_none() {
                                            match serde_json::from_value::<JsonRpcNotification>(json_msg) {
                                                Ok(notification) => {
                                                    if let Some(handler) = self.notification_handler.lock().unwrap().as_ref() {
                                                        let handler_future = handler(notification);
                                                        tokio::spawn(async move {
                                                            handler_future.await;
                                                        });
                                                    } else {
                                                        warn!("Received notification but no handler is subscribed.");
                                                    }
                                                }
                                                Err(e) => {
                                                    error!("Failed to parse valid JSON as notification: {}", e);
                                                }
                                            }
                                        } else {
                                            // It might be a response to a POST, ignore here
                                            warn!("Received non-notification message via SSE: {}", msg.data);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse SSE message data as JSON: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("SSE stream error: {}", e);
                                es.close(); // Close the stream on error
                                break; // Exit inner loop to trigger reconnect
                            }
                        }
                    }
                    info!("SSE stream closed or loop exited.");
                }
                Err(e) => {
                    error!("Failed to create EventSource: {}", e);
                }
            }

            let is_still_running = *self.is_running.lock().unwrap();
             if !is_still_running {
                  info!("SSE listening task stopping after connection attempt.");
                  break;
             }

            // Wait before attempting to reconnect, implement backoff
            info!("Waiting {:?} before attempting SSE reconnect.", delay);
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(60)); // Exponential backoff up to 60s
        }
    }
}

#[async_trait]
impl Transport for SSEClientTransport {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let response_value = self.send_post_request(serde_json::to_value(request)?).await?;
        serde_json::from_value(response_value)
            .map_err(|e| anyhow!("Failed to parse response value as JsonRpcResponse: {}", e))
    }

    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        // Notifications are typically sent via POST in this model
        self.send_post_request(serde_json::to_value(notification)?).await?;
        Ok(())
    }

    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()> {
        {
            let mut guard = self.notification_handler.lock().unwrap();
            *guard = Some(handler);
        }

        // Start the listening task if not already running
        let mut is_running_guard = self.is_running.lock().unwrap();
        if !*is_running_guard {
            *is_running_guard = true;
            let self_clone = self.clone(); // Clone for the task
            tokio::spawn(async move {
                self_clone.start_listening_task().await;
            });
            info!("SSE notification listener task started.");
        } else {
             info!("SSE notification listener task already running.");
        }

        Ok(())
    }

    async fn close(&self) -> Result<()> {
        info!("Closing SSE transport.");
        {
            let mut is_running = self.is_running.lock().unwrap();
            *is_running = false; // Signal the listening task to stop
        }
        // The listening task will close the EventSource when it detects the flag change
        Ok(())
    }
}
