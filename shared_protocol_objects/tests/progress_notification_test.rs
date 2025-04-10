use anyhow::Result;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::test;
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::test;

use shared_protocol_objects::{
    create_notification, JsonRpcNotification, ProgressParams, // Use ProgressParams
};
use futures::future::BoxFuture;


// Mock transport for testing notification handling
struct NotificationTestTransport {
    sent_notifications: Arc<Mutex<Vec<JsonRpcNotification>>>,
    notification_handler: Arc<Mutex<Option<Box<dyn Fn(JsonRpcNotification) -> BoxFuture<'static, ()> + Send + Sync + 'static>>>>,
}

impl NotificationTestTransport {
    fn new() -> Self {
        Self {
            sent_notifications: Arc::new(Mutex::new(Vec::new())),
            notification_handler: Arc::new(Mutex::new(None)),
        }
    }
    
    async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        // Store the notification
        {
            let mut guard = self.sent_notifications.lock().unwrap();
            guard.push(notification.clone());
        }
        
        // Trigger the handler if set
        if let Some(handler) = &*self.notification_handler.lock().unwrap() {
            handler(notification).await;
        }
        
        Ok(())
    }
    
    async fn subscribe_to_notifications(&self, handler: Box<dyn Fn(JsonRpcNotification) -> BoxFuture<'static, ()> + Send + Sync + 'static>) -> Result<()> {
        let mut guard = self.notification_handler.lock().unwrap();
        *guard = Some(handler);
        Ok(())
    }
    
    fn get_sent_notifications(&self) -> Vec<JsonRpcNotification> {
        let guard = self.sent_notifications.lock().unwrap();
        guard.clone()
    }
}

// Test progress notification creation
async fn test_progress_params_creation() { // Renamed test
    // Create progress params
    let progress_token = "token-123".to_string();
    let params = ProgressParams {
        progress_token: progress_token.clone(), // Add token
        progress: 50,
        total: Some(100),
        message: Some("Processing data...".to_string()),
    };

    // Verify fields
    assert_eq!(params.progress_token, progress_token, "Token should match");
    assert_eq!(progress.progress, 50, "Progress should be 50");
    assert_eq!(progress.total, Some(100), "Total should be 100");
    assert_eq!(progress.message, Some("Processing data...".to_string()), "Message should match");
    
    // Create a notification wrapping the progress
    let notification = create_notification("progress", json!(progress));
    
    // Verify notification structure
    assert_eq!(notification.jsonrpc, "2.0", "JSON-RPC version should be 2.0");
    assert_eq!(notification.method, "progress", "Method should be progress");
    
    // Check params field
    let params = notification.params;
    assert!(params.is_object(), "Params should be an object");
    assert_eq!(params.get("progress").unwrap(), 50, "Progress should be 50");
    assert_eq!(params.get("total").unwrap(), 100, "Total should be 100");
    assert_eq!(params.get("message").unwrap(), "Processing data...", "Message should match");
}

// Test sending progress notifications
#[test]
async fn test_sending_progress_notifications() -> Result<()> {
    // Create test transport
    let transport = NotificationTestTransport::new();
    let token = "task-abc".to_string();

    // Create progress params
    let params1 = ProgressParams {
        progress_token: token.clone(),
        progress: 25,
        total: Some(100),
        message: Some("Started processing...".to_string()),
    };

    let params2 = ProgressParams {
        progress_token: token.clone(),
        progress: 50,
        total: Some(100),
        message: Some("Halfway done...".to_string()),
    };

    let params3 = ProgressParams {
        progress_token: token.clone(),
        progress: 100,
        total: Some(100),
        message: Some("Completed!".to_string()),
    };

    // Send notifications
    transport.send_notification(create_notification("notifications/progress", json!(params1))).await?;
    transport.send_notification(create_notification("notifications/progress", json!(params2))).await?;
    transport.send_notification(create_notification("notifications/progress", json!(params3))).await?;
    
    // Verify sent notifications
    let notifications = transport.get_sent_notifications();
    assert_eq!(notifications.len(), 3, "Should have 3 notifications");

    // Check first notification params
    let p1: ProgressParams = serde_json::from_value(notifications[0].params.clone()).unwrap();
    assert_eq!(p1.progress, 25);
    assert_eq!(p1.progress_token, token);

    // Check second notification params
    let p2: ProgressParams = serde_json::from_value(notifications[1].params.clone()).unwrap();
    assert_eq!(p2.progress, 50);

    // Check third notification params
    let p3: ProgressParams = serde_json::from_value(notifications[2].params.clone()).unwrap();
    assert_eq!(p3.progress, 100);
    assert_eq!(p3.message.unwrap(), "Completed!");
    
    Ok(())
}

// Test receiving and handling progress notifications
#[test]
async fn test_receiving_progress_notifications() -> Result<()> {
    // Create test transport
    let transport = NotificationTestTransport::new();
    
    // Create channel for tests
    let (tx, mut rx) = mpsc::channel::<u32>(10);
    
    // Subscribe to notifications
    let tx_clone = tx.clone();
    let expected_token = "recv-token".to_string();

    transport.subscribe_to_notifications(Box::new(move |notification: JsonRpcNotification| {
        let tx = tx_clone.clone();
        let token = expected_token.clone();
        Box::pin(async move {
            if notification.method == "notifications/progress" { // Check standard method name
                if let Ok(params) = serde_json::from_value::<ProgressParams>(notification.params) {
                    if params.progress_token == token { // Check token
                        let _ = tx.send(params.progress).await;
                    }
                }
            }
        })
    })).await?;

    // Create and send progress notifications
    for progress_val in &[10, 30, 60, 90, 100] {
        let params = ProgressParams {
            progress_token: expected_token.clone(),
            progress: *progress_val,
            total: Some(100),
            message: None,
        };

        transport.send_notification(create_notification("notifications/progress", json!(params))).await?;
    }

    // Collect received progress values
    let mut received_progress = Vec::new();
    while let Ok(progress) = rx.try_recv() {
        received_progress.push(progress);
    }
    
    // Verify all progress values were received
    assert_eq!(received_progress, vec![10, 30, 60, 90, 100], "Should receive all progress values in order");
    
    Ok(())
}

// Test notification handling with complex logic
#[test]
async fn test_complex_notification_handling() -> Result<()> {
    // Create test transport
    let transport = NotificationTestTransport::new();
    
    // Create shared state for tests
    let progress_state = Arc::new(Mutex::new(Vec::<u32>::new()));
    let last_message = Arc::new(Mutex::new(String::new()));
    let token = "complex-task".to_string();

    // Subscribe to notifications with complex handling logic
    let progress_clone = Arc::clone(&progress_state);
    let message_clone = Arc::clone(&last_message);
    let expected_token = token.clone();

    transport.subscribe_to_notifications(Box::new(move |notification: JsonRpcNotification| {
        let progress_state = Arc::clone(&progress_clone);
        let last_message = Arc::clone(&message_clone);
        let token = expected_token.clone();

        Box::pin(async move {
            if notification.method == "notifications/progress" {
                if let Ok(params) = serde_json::from_value::<ProgressParams>(notification.params) {
                    if params.progress_token == token {
                        // Store progress value
                        {
                            let mut progress_guard = progress_state.lock().unwrap();
                            progress_guard.push(params.progress);
                        }

                        // Store message if provided
                        if let Some(msg) = params.message {
                            let mut msg_guard = last_message.lock().unwrap();
                            *msg_guard = msg;
                        }

                        // Artificial delay to simulate work
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    }
                }
            }
        })
    })).await?;

    // Send multiple notifications rapidly
    for i in 1..=5 {
        let progress_val = i * 20;
        let params = ProgressParams {
            progress_token: token.clone(),
            progress: progress_val,
            total: Some(100),
            message: Some(format!("Progress at {}%", progress_val)),
        };

        transport.send_notification(create_notification("notifications/progress", json!(params))).await?;
    }

    // Allow time for all notifications to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify progress values were collected
    let progress_values = {
        let guard = progress_state.lock().unwrap();
        guard.clone()
    };
    
    assert_eq!(progress_values, vec![20, 40, 60, 80, 100], "Should collect all progress values");
    
    // Verify last message
    let message = {
        let guard = last_message.lock().unwrap();
        guard.clone()
    };
    
    assert_eq!(message, "Progress at 100%", "Last message should be from the final notification");
    
    Ok(())
}

// Test progress notification with indeterminate progress
#[test]
async fn test_indeterminate_progress() -> Result<()> {
    // Create test transport
    let transport = NotificationTestTransport::new();
    let token = "indeterminate-token".to_string();

    // Create an indeterminate progress params (no total)
    let params = ProgressParams {
        progress_token: token.clone(),
        progress: 50, // Some arbitrary progress value
        total: None,  // No total means indeterminate
        message: Some("Processing...".to_string()),
    };

    // Send the notification
    transport.send_notification(create_notification("notifications/progress", json!(params))).await?;

    // Verify the notification
    let notifications = transport.get_sent_notifications();
    assert_eq!(notifications.len(), 1, "Should have 1 notification");

    let deserialized: ProgressParams = serde_json::from_value(notifications[0].params.clone()).unwrap();
    assert_eq!(deserialized.progress, 50);
    assert!(deserialized.total.is_none(), "Total should be None");
    assert_eq!(deserialized.progress_token, token);
    
    Ok(())
}
