use serde_json::Value;
use thiserror::Error;
use tracing::error; // Import the error macro

/// MCP client errors
#[derive(Debug, Error)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(#[from] anyhow::Error),
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Request timeout")]
    Timeout,
    
    #[error("Client not initialized")]
    NotInitialized,
    
    #[error("RPC error {code}: {message}")]
    RpcError { 
        code: i64, 
        message: String,
        data: Option<Value>,
    },
    
    #[error("No result in response")]
    NoResult,
    
    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),
    
    #[error("Protocol error: {0}")]
    Protocol(String),
}

// Removed manual From<serde_json::Error> implementation as #[from] handles it.
