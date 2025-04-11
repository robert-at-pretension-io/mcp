use serde_json::Value;
use thiserror::Error;

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

// Add From implementation for serde_json::Error
impl From<serde_json::Error> for McpError {
    fn from(error: serde_json::Error) -> Self {
        error!("JSON deserialization error: {}", error); // Log the error
        Self::Protocol(format!("Failed to deserialize response: {}", error))
    }
}
