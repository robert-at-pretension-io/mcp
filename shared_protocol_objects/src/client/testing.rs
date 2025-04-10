use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use crate::{CallToolResult, ListToolsResult, ToolInfo, ToolResponseContent}; // Added ListToolsResult
use super::trait_def::ReplClient;

/// Mock implementation of ReplClient for testing
///
/// This mock client can be configured with predefined tools and responses,
/// allowing test code to verify client-server interactions without requiring
/// actual network communication.
#[derive(Clone)] // Add Clone derive
pub struct MockReplClient {
    name: String,
    tools: Vec<ToolInfo>,
    responses: HashMap<String, CallToolResult>,
    list_tools_cursor: Option<String>, // Add cursor support for list_tools
}

impl MockReplClient {
    /// Create a new mock client with the given name
    pub fn new(name: &str) -> Self {
        let mut client = Self {
            name: name.to_string(),
            tools: Vec::new(),
            responses: HashMap::new(),
            list_tools_cursor: None,
        };

        // Add default mock tool
        client.add_tool("test_tool", "A test tool", serde_json::json!({}));
        client.add_response("test_tool", "Test tool output");
        
        client
    }
    
    /// Add a tool to the mock client
    pub fn add_tool(&mut self, name: &str, description: &str, schema: Value) -> &mut Self {
        self.tools.push(ToolInfo {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: schema,
            annotations: None, // Added missing field
        });
        self
    }
    
    /// Add a response for a specific tool
    pub fn add_response(&mut self, tool_name: &str, response: &str) -> &mut Self {
        self.responses.insert(tool_name.to_string(), CallToolResult {
            content: vec![ToolResponseContent {
                type_: "text".to_string(),
                text: response.to_string(),
                annotations: None,
            }],
            is_error: None,
            // Removed _meta, progress, total
        });
        self
    }
    
    /// Add an error response for a specific tool
    pub fn add_error_response(&mut self, tool_name: &str, error_message: &str) -> &mut Self {
        self.responses.insert(tool_name.to_string(), CallToolResult {
            content: vec![ToolResponseContent {
                type_: "text".to_string(),
                text: error_message.to_string(),
                annotations: None,
            }],
            is_error: Some(true),
            // Removed _meta, progress, total
        });
        self
    }
}

#[async_trait]
impl ReplClient for MockReplClient {
    fn name(&self) -> &str {
        &self.name
    }

    async fn list_tools(&self) -> Result<ListToolsResult> {
        // Simulate pagination if a cursor is set
        Ok(ListToolsResult {
            tools: self.tools.clone(),
            next_cursor: self.list_tools_cursor.clone(),
        })
    }

    async fn call_tool(&self, tool_name: &str, _args: Value) -> Result<CallToolResult> {
        self.responses.get(tool_name)
            .cloned()
            .ok_or_else(|| anyhow!("No mock response for tool {}", tool_name))
    }
    
    async fn close(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use crate::ListToolsResult;

    #[tokio::test]
    async fn test_mock_client() {
        let mut client = MockReplClient::new("test-server");

        // Test the default tool
        let list_result = client.list_tools().await.unwrap();
        assert_eq!(list_result.tools.len(), 1);
        assert_eq!(list_result.tools[0].name, "test_tool");
        assert!(list_result.next_cursor.is_none());

        // Add a custom tool
        client.add_tool("custom_tool", "A custom tool", serde_json::json!({
            "type": "object",
            "properties": {
                "param1": { "type": "string" }
            }
        }));
        
        client.add_response("custom_tool", "Custom tool output");

        // Test listing tools again
        let list_result = client.list_tools().await.unwrap();
        assert_eq!(list_result.tools.len(), 2);
        assert_eq!(list_result.tools[1].name, "custom_tool");

        // Test calling tools
        let result = client.call_tool("test_tool", Value::Null).await.unwrap();
        assert_eq!(result.content[0].text, "Test tool output");
        
        let result = client.call_tool("custom_tool", Value::Null).await.unwrap();
        assert_eq!(result.content[0].text, "Custom tool output");
        
        // Test error case
        assert!(client.call_tool("nonexistent_tool", Value::Null).await.is_err());
        
        // Test error response
        client.add_error_response("error_tool", "This is an error");
        let result = client.call_tool("error_tool", Value::Null).await.unwrap();
        assert!(result.is_error.unwrap());
        assert_eq!(result.content[0].text, "This is an error");
    }
}
