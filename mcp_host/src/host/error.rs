use thiserror::Error;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("Server error: {0}")]
    Server(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("AI provider error: {0}")]
    AIProvider(String),
    
    #[error("JSON-RPC error {code}: {message}")]
    RPC { code: i64, message: String },
    
    #[error("Transport error: {0}")]
    Transport(String),
    
    #[error("Server not found: {0}")]
    ServerNotFound(String),
    
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    JSON(#[from] serde_json::Error),
    
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, HostError>;