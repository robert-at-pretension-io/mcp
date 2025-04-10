use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use crate::{CallToolResult, ToolInfo};
use crate::rpc::{McpClient, Transport, ProcessTransport};
use super::trait_def::ReplClient;

/// Adapter for McpClient that implements ReplClient
///
/// This adapter wraps an McpClient and implements the ReplClient trait,
/// providing a standardized interface for client code to interact with
/// MCP-compatible servers.
pub struct McpClientAdapter<T: Transport> {
    client: McpClient<T>,
    name: String,
}

impl<T: Transport> McpClientAdapter<T> {
    /// Create a new adapter wrapping the given McpClient
    pub fn new(client: McpClient<T>, name: String) -> Self {
        Self { client, name }
    }
    
    /// Get a reference to the underlying McpClient
    pub fn inner_client(&self) -> &McpClient<T> {
        &self.client
    }
    
    /// Get a mutable reference to the underlying McpClient
    pub fn inner_client_mut(&mut self) -> &mut McpClient<T> {
        &mut self.client
    }
}

/// Type alias for the production version with ProcessTransport
pub type ProcessClientAdapter = McpClientAdapter<ProcessTransport>;

#[async_trait]
impl<T: Transport> ReplClient for McpClientAdapter<T> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn list_tools(&self) -> Result<ListToolsResult> {
        self.client.list_tools().await
    }

    async fn call_tool(&self, tool_name: &str, args: Value) -> Result<CallToolResult> {
        self.client.call_tool(tool_name, args).await
    }
    
    async fn close(self: Box<Self>) -> Result<()> {
        self.client.close().await
    }
}
