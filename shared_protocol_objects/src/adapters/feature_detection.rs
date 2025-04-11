use std::sync::atomic::{AtomicBool, Ordering};

// Static flag to track which implementation is *actually* being used at runtime.
// This is useful even if the feature flag is enabled, as fallback might occur.
static USING_RMCP_RUNTIME: AtomicBool = AtomicBool::new(false);

/// Check if the RMCP SDK implementation is currently active at runtime.
/// This might be false even if the `rmcp-integration` feature is enabled,
/// if the adapter failed to initialize and fell back to the native implementation.
pub fn is_using_rmcp() -> bool {
    USING_RMCP_RUNTIME.load(Ordering::Relaxed)
}

/// Set the runtime status of RMCP usage. Called by the transport factory.
pub fn set_using_rmcp(value: bool) {
    let previously_set = USING_RMCP_RUNTIME.swap(value, Ordering::Relaxed);

    // Log only if the state changes or during the initial setting.
    if value != previously_set || !previously_set { // Log on first set true or change
        if value {
            tracing::info!("Runtime check: Using RMCP SDK implementation.");
        } else {
            // Only log fallback if RMCP was expected (feature enabled)
            if cfg!(feature = "rmcp-integration") {
                 tracing::info!("Runtime check: Using native implementation (RMCP adapter failed or disabled).");
            } else {
                 tracing::info!("Runtime check: Using native implementation (RMCP feature disabled).");
            }
        }
    }
}

/// Initialize feature detection state based on compile-time flags.
/// This should be called once at application startup.
pub fn initialize() {
    // Set the initial state based on whether the feature is compiled.
    // The actual runtime usage might change later if adapter creation fails.
    if cfg!(feature = "rmcp-integration") {
        // Assume RMCP will be used initially if the feature is enabled.
        // `set_using_rmcp(true)` will be called by the factory if successful.
        // `set_using_rmcp(false)` will be called by the factory if it fails and falls back.
        tracing::info!("RMCP integration feature is enabled at compile time.");
        // We don't set USING_RMCP_RUNTIME here; the factory does it based on success/failure.
    } else {
        tracing::info!("RMCP integration feature is disabled at compile time. Using native implementation.");
        // Explicitly set to false as RMCP will never be used.
        set_using_rmcp(false);
    }
}
