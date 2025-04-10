use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use crate::{CallToolResult, ListToolsResult}; // Removed ToolInfo

/// A trait representing a client connection to an MCP server
/// 
/// This trait abstracts over different client implementations for
/// communicating with MCP-compatible tool servers. It provides methods
/// for listing available tools, calling tools, and managing the connection.
#[async_trait]
pub trait ReplClient: Send + Sync {
    /// Get the server's name
    fn name(&self) -> &str;

    /// List available tools on the server.
    /// Returns a result containing the list and potentially a cursor for pagination.
    async fn list_tools(&self) -> Result<ListToolsResult>;

    /// Call a tool with the given arguments.
    async fn call_tool(&self, tool_name: &str, args: Value) -> Result<CallToolResult>;

    /// Close the connection to the server.
    async fn close(self: Box<Self>) -> Result<()>;
}
