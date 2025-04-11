use anyhow::Result;
use tokio::process::Command;
use crate::rpc::Transport; // Use the Transport trait from rpc module
use std::time::Duration; // Added for timeout configuration

// Import the specific transport types
use super::rmcp_transport::RmcpProcessTransportAdapter;
use crate::rpc::ProcessTransport as NativeProcessTransport; // Alias native transport

// Import feature detection helper
use super::feature_detection;

/// Create a transport using the best available implementation (RMCP if enabled, otherwise native).
pub async fn create_process_transport(mut command: Command) -> Result<Box<dyn Transport>> {
    create_process_transport_with_timeout(command, Duration::from_secs(30)).await // Default timeout
}

/// Create a transport with a specific request timeout.
pub async fn create_process_transport_with_timeout(mut command: Command, request_timeout: Duration) -> Result<Box<dyn Transport>> {
    // Check if the RMCP feature is enabled and attempt to use it
    if cfg!(feature = "rmcp-integration") {
        match RmcpProcessTransportAdapter::new_with_timeout(&mut command, request_timeout).await {
            Ok(adapter) => {
                tracing::info!("Successfully created RMCP transport adapter.");
                feature_detection::set_using_rmcp(true); // Mark RMCP as active
                Ok(Box::new(adapter))
            },
            Err(e) => {
                // Log the specific error causing the failure before falling back
                tracing::error!(error = %e, "Failed to create RmcpTransportAdapter"); // Log the actual error object
                tracing::warn!("Falling back to native transport implementation due to RMCP adapter creation failure.");
                feature_detection::set_using_rmcp(false); // Mark native as active

                // Create our native implementation with the same timeout
                let native = NativeProcessTransport::new_with_timeout(command, request_timeout).await?;
                Ok(Box::new(native))
            }
        }
    } else {
        // RMCP feature is not enabled, directly use the native implementation
        tracing::info!("RMCP feature not enabled, using native transport implementation.");
        feature_detection::set_using_rmcp(false); // Mark native as active

        let native = NativeProcessTransport::new_with_timeout(command, request_timeout).await?;
        Ok(Box::new(native))
    }
}
