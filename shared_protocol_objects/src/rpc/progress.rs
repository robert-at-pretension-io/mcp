use std::collections::HashMap;
use futures::future::BoxFuture;

use crate::ProgressNotification;

/// Handles progress notifications for long-running operations
pub struct ProgressTracker {
    handlers: HashMap<String, Box<dyn Fn(ProgressNotification) -> BoxFuture<'static, ()> + Send + Sync>>,
}

impl ProgressTracker {
    /// Create a new progress tracker
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
    
    /// Register a handler for a progress token
    pub fn register<F>(&mut self, token: String, handler: F)
    where
        F: Fn(ProgressNotification) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        self.handlers.insert(token, Box::new(handler));
    }
    
    /// Handle a progress notification
    pub fn handle(&self, notification: ProgressNotification) -> BoxFuture<'static, ()> {
        // In actual implementation, you'd match on the token field in ProgressNotification
        // For this example, assuming notification has a progress_token field:
        let token = notification.progress.to_string(); // Just using progress as token for example
        
        if let Some(handler) = self.handlers.get(&token) {
            handler(notification)
        } else {
            Box::pin(async {})
        }
    }
    
    /// Remove a handler
    pub fn unregister(&mut self, token: &str) {
        self.handlers.remove(token);
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}