use crate::ProgressParams; // Use ProgressParams which includes the token
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Handles progress notifications for long-running operations
/// by associating handlers with progress tokens.
#[derive(Clone)] // Allow cloning if needed (e.g., for passing to notification handlers)
pub struct ProgressTracker {
    // Use Arc<Mutex<...>> for thread-safe interior mutability
    handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(ProgressParams) -> BoxFuture<'static, ()> + Send + Sync>>>>,
}

impl ProgressTracker {
    /// Create a new progress tracker.
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a handler for a specific progress token.
    pub fn register<F>(&self, token: String, handler: F)
    where
        F: Fn(ProgressParams) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        debug!("Registering progress handler for token: {}", token);
        let mut guard = self.handlers.lock().expect("ProgressTracker lock poisoned");
        guard.insert(token, Box::new(handler));
    }

    /// Handle incoming progress parameters, dispatching to the correct handler based on the token.
    pub fn handle(&self, params: ProgressParams) -> BoxFuture<'static, ()> {
        let token = params.progress_token.clone();
        debug!("Handling progress for token: {}", token);
        let handler_opt = {
            let guard = self.handlers.lock().expect("ProgressTracker lock poisoned");
            // We need to clone the Box<Fn...> out or use Arc if handlers need to live longer
            // For simplicity here, let's assume the handler is called immediately.
            // A more robust implementation might use Arc<Fn...>.
            guard.get(&token).map(|h| h(params)) // Call directly if found
        };

        if let Some(future) = handler_opt {
            debug!("Found handler for progress token: {}", token);
            future
        } else {
            warn!("No progress handler found for token: {}", token);
            Box::pin(async {}) // Return a no-op future
        }
    }

    /// Remove the handler associated with a progress token.
    pub fn unregister(&self, token: &str) {
        debug!("Unregistering progress handler for token: {}", token);
        let mut guard = self.handlers.lock().expect("ProgressTracker lock poisoned");
        guard.remove(token);
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}
