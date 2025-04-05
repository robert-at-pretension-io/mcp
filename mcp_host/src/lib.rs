// MCP Host library
pub mod ai_client;
pub mod anthropic;
pub mod deepseek;
pub mod gemini;
pub mod openai;
pub mod conversation_service;
pub mod repl;
pub mod main_repl;
pub mod conversation_state;
pub mod host;
pub mod smiley_tool_parser;
pub mod rllm_adapter; 

// Re-export key components 
pub use crate::host::MCPHost;
pub use crate::host::config;
pub use crate::host::error::{HostError, Result};
pub use crate::host::server_manager::ManagedServer;
pub use crate::rllm_adapter::RLLMClient;
