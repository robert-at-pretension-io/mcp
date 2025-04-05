use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Generates unique IDs for JSON-RPC requests
pub struct IdGenerator {
    counter: AtomicU64,
    use_uuid: bool,
}

impl IdGenerator {
    /// Create a new ID generator
    pub fn new(use_uuid: bool) -> Self {
        Self {
            counter: AtomicU64::new(1), // Always start at 1
            use_uuid,
        }
    }
    
    /// Create a new ID generator with specific starting value
    pub fn with_start_value(use_uuid: bool, start_value: u64) -> Self {
        Self {
            counter: AtomicU64::new(start_value),
            use_uuid,
        }
    }
    
    /// Create a new ID generator that uses numeric IDs
    pub fn new_numeric() -> Self {
        Self::new(false)
    }
    
    /// Create a new ID generator that uses UUID string IDs
    pub fn new_uuid() -> Self {
        Self::new(true)
    }
    
    /// Get the next ID
    pub fn next_id(&self) -> Value {
        if self.use_uuid {
            Value::String(Uuid::new_v4().to_string())
        } else {
            let id = self.counter.fetch_add(1, Ordering::SeqCst);
            Value::Number(id.into())
        }
    }
}