// Client interface for MCP servers
mod trait_def;
mod adapter;
mod testing;

pub use trait_def::ReplClient;
pub use adapter::{McpClientAdapter, ProcessClientAdapter};
pub use testing::MockReplClient;