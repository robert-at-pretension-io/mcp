// Conditionally compile modules based on the feature flag
#[cfg(feature = "rmcp-integration")]
mod rmcp_transport;
#[cfg(feature = "rmcp-integration")]
mod rmcp_protocol;
#[cfg(feature = "rmcp-integration")]
mod rmcp_service;
#[cfg(feature = "rmcp-integration")]
mod transport_factory;
#[cfg(feature = "rmcp-integration")]
mod error;
#[cfg(feature = "rmcp-integration")]
mod feature_detection;
#[cfg(feature = "rmcp-integration")]
mod telemetry;

// Export public interfaces only if the feature is enabled
#[cfg(feature = "rmcp-integration")]
pub use rmcp_transport::RmcpProcessTransportAdapter;
#[cfg(feature = "rmcp-integration")]
pub use rmcp_protocol::RmcpProtocolAdapter;
#[cfg(feature = "rmcp-integration")]
pub use rmcp_service::RmcpServiceAdapter;
#[cfg(feature = "rmcp-integration")]
pub use transport_factory::create_process_transport;
#[cfg(feature = "rmcp-integration")]
pub use error::AdapterError;
#[cfg(feature = "rmcp-integration")]
pub use feature_detection::{initialize as initialize_feature_detection, is_using_rmcp, set_using_rmcp};
#[cfg(feature = "rmcp-integration")]
pub use telemetry::{increment_request_count, increment_error_count, increment_notification_count, get_metrics, RequestTimer};


// Re-export our notification handler type regardless of the feature flag,
// as it's part of the core Transport trait definition.
pub use crate::rpc::NotificationHandler;

/// Feature detection for RMCP SDK availability (compile-time check)
pub fn is_rmcp_available() -> bool {
    // This function now reflects whether the feature was enabled during compilation
    cfg!(feature = "rmcp-integration")
}

// Define a placeholder or default behavior when the feature is not enabled
#[cfg(not(feature = "rmcp-integration"))]
pub mod transport_factory {
    use crate::rpc::{ProcessTransport, Transport};
    use anyhow::Result;
    use tokio::process::Command;
    use std::sync::Arc;

    /// Create a transport using the native implementation when RMCP is not available.
    pub async fn create_process_transport(command: Command) -> Result<Box<dyn Transport>> {
         tracing::info!("RMCP feature not enabled, using native transport implementation");
         // Create our native implementation
         let native = ProcessTransport::new(command).await?;
         Ok(Box::new(native))
    }
}

// Provide dummy implementations or re-exports for types needed when the feature is off,
// if they are used unconditionally elsewhere. For now, we assume conditional usage.
// If compilation errors arise due to missing types when the feature is off,
// add necessary placeholders here. E.g.:
// #[cfg(not(feature = "rmcp-integration"))]
// pub type RmcpServiceAdapter = (); // Placeholder type

// Dummy telemetry functions when feature is off
#[cfg(not(feature = "rmcp-integration"))]
pub mod telemetry {
    pub fn increment_request_count() {}
    pub fn increment_error_count() {}
    pub fn increment_notification_count() {}
    pub fn get_metrics() -> (usize, usize, usize) { (0, 0, 0) }
    pub struct RequestTimer;
    impl RequestTimer {
        pub fn new(_method: &str) -> Self { Self }
        pub fn finish(self) {}
    }
}

// Dummy feature detection functions when feature is off
#[cfg(not(feature = "rmcp-integration"))]
pub mod feature_detection {
     pub fn initialize() {
         tracing::info!("Using native implementation (RMCP feature disabled)");
     }
     pub fn is_using_rmcp() -> bool { false }
     pub fn set_using_rmcp(_value: bool) { /* No-op */ }
}
