// MCP Host library
pub mod ai_client;
pub mod anthropic;
pub mod deepseek;
pub mod gemini;
pub mod conversation_service;
pub mod my_regex;
pub mod repl;
pub mod main_repl;
pub mod conversation_state;
pub mod host;

// Re-export MCPHost from host module
pub use crate::host::MCPHost;
