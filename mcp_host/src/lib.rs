// MCP Host library
pub mod ai_client;
pub mod conversation_service;
pub mod repl;
pub mod main_repl;
pub mod conversation_state;
pub mod conversation_logic; // Add this line
pub mod host;
pub mod tool_parser;
pub mod rllm_adapter;
pub mod openrouter;

// Re-export key components 
pub use crate::host::MCPHost;
pub use crate::host::config;
pub use crate::host::error::{HostError, Result};
pub use crate::host::server_manager::ManagedServer;
// pub use crate::rllm_adapter::{RLLMClient, create_rllm_client_for_provider};
