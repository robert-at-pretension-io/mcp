use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant}; // Use std::time

// Counters for requests, errors, and notifications specific to the adapter layer
static ADAPTER_REQUEST_COUNT: AtomicUsize = AtomicUsize::new(0);
static ADAPTER_ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);
static ADAPTER_NOTIFICATION_COUNT: AtomicUsize = AtomicUsize::new(0);
// Consider adding counters per method if needed

pub fn increment_request_count() {
    ADAPTER_REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_error_count() {
    ADAPTER_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_notification_count() {
    ADAPTER_NOTIFICATION_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Get the current adapter metrics (requests, errors, notifications).
pub fn get_metrics() -> (usize, usize, usize) {
    let requests = ADAPTER_REQUEST_COUNT.load(Ordering::Relaxed);
    let errors = ADAPTER_ERROR_COUNT.load(Ordering::Relaxed);
    let notifications = ADAPTER_NOTIFICATION_COUNT.load(Ordering::Relaxed);

    (requests, errors, notifications)
}

/// Reset all adapter metrics (useful for testing or periodic reporting).
pub fn reset_metrics() {
    ADAPTER_REQUEST_COUNT.store(0, Ordering::Relaxed);
    ADAPTER_ERROR_COUNT.store(0, Ordering::Relaxed);
    ADAPTER_NOTIFICATION_COUNT.store(0, Ordering::Relaxed);
}


/// Simple timer for measuring request durations within the adapter layer.
/// Uses `tracing` debug level to log durations.
#[derive(Debug)]
pub struct RequestTimer {
    start: Instant,
    method: String, // Store method name as String
}

impl RequestTimer {
    /// Creates a new timer and records the start time.
    pub fn new(method: &str) -> Self {
        Self {
            start: Instant::now(),
            method: method.to_string(), // Convert &str to String
        }
    }

    /// Call when the request finishes to log the duration.
    pub fn finish(self) {
        let duration = self.start.elapsed();
        // Log duration at debug level
        tracing::debug!(method = %self.method, duration = ?duration, "Adapter request finished");

        // TODO: Integrate with a proper metrics system if available
        // e.g., record histogram of durations per method
        // metrics::histogram!("adapter_request_duration_seconds", duration.as_secs_f64(), "method" => self.method);
    }
}

// Ensure drop logs if finish() is not called explicitly (e.g., due to early return/error)
impl Drop for RequestTimer {
    fn drop(&mut self) {
        // Avoid double logging if finish() was called.
        // We can't easily know if finish was called, so maybe log differently or add a flag.
        // For simplicity now, we rely on explicit finish() calls.
        // Alternatively, always log in drop, and finish() just calculates duration without logging.
        // let duration = self.start.elapsed();
        // tracing::trace!(method = %self.method, duration = ?duration, "Adapter request timer dropped");
    }
}
