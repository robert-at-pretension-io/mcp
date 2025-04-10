use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use shared_protocol_objects::{
    CallToolParams, CallToolResult, Implementation, 
    InitializeResult, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    ServerCapabilities, ToolInfo, ToolResponseContent
};
use shared_protocol_objects::rpc::{Transport, NotificationHandler};

// Removed unused MockReplClient import
use serde_json::{json, Value};

/// Mock transport for testing
pub struct MockTransport {
    tools: Vec<ToolInfo>,
    call_results: HashMap<String, Vec<ToolResponseContent>>,
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,
    is_initialized: Arc<Mutex<bool>>,
}

impl MockTransport {
    pub fn new() -> Self {
        let mut mock = Self {
            tools: Vec::new(),
            call_results: HashMap::new(),
            notification_handler: Arc::new(Mutex::new(None)),
            is_initialized: Arc::new(Mutex::new(true)), // Pre-initialize for tests
        };
        
        // Add some default tools
        mock.add_tool("test_tool", "A test tool", json!({
            "type": "object",
            "properties": {
                "param1": {"type": "string"}
            }
        }));
        
        // Add a default response for the test tool
        mock.add_call_result("test_tool", "Test tool output");
        
        mock
    }
    
    pub fn add_tool(&mut self, name: &str, description: &str, schema: Value) {
        self.tools.push(ToolInfo {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: schema,
            annotations: None, // Added missing field
        });
    }
    
    pub fn add_call_result(&mut self, tool_name: &str, output: &str) {
        let content = ToolResponseContent {
            type_: "text".to_string(),
            text: output.to_string(),
            annotations: None,
        };
        
        self.call_results.entry(tool_name.to_string())
            .or_insert_with(Vec::new)
            .push(content);
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        // Mark the client as initialized if it's an initialize request
        if request.method == "initialize" {
            let mut init = self.is_initialized.lock().unwrap();
            *init = true;
        }
        
        // Check if the client is initialized for non-initialize requests
        if request.method != "initialize" {
            let init = self.is_initialized.lock().unwrap();
            if !*init {
                return Err(anyhow::anyhow!("Client not initialized"));
            }
        }
        
        match request.method.as_str() {
            "initialize" => {
                let result = InitializeResult {
                    protocol_version: "2025-03-26".to_string(),
                    capabilities: ServerCapabilities {
                        experimental: None,
                        logging: None,
                        prompts: None,
                        resources: None,
                        tools: Some(shared_protocol_objects::ToolsCapability {
                            list_changed: true,
                        }),
                    },
                    server_info: Implementation {
                        name: "mock-server".to_string(),
                        version: "1.0.0".to_string(),
                    },
                    instructions: None, // Use instructions field instead of _meta
                };

                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(json!(result)),
                    error: None,
                })
            },
            "tools/list" => {
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(json!({
                        "tools": self.tools
                    })),
                    error: None,
                })
            },
            "tools/call" => {
                // Parse params as CallToolParams
                if let Ok(params) = serde_json::from_value::<CallToolParams>(request.params.unwrap_or_default()) {
                    let tool_name = params.name;
                    
                    let contents = self.call_results.get(&tool_name)
                        .cloned()
                        .unwrap_or_else(|| vec![ToolResponseContent {
                            type_: "text".to_string(),
                            text: format!("No result defined for tool: {}", tool_name),
                            annotations: None,
                        }]);
                    
                    let result = CallToolResult {
                        content: contents,
                        is_error: None,
                        // Removed _meta, progress, total
                    };

                    Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(json!(result)),
                        error: None,
                    })
                } else {
                    // Return an error for invalid tool calls
                    Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(shared_protocol_objects::JsonRpcError {
                            code: -32602,
                            message: "Invalid params".to_string(),
                            data: None,
                        }),
                    })
                }
            },
            _ => {
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(shared_protocol_objects::JsonRpcError {
                        code: -32601,
                        message: format!("Method not implemented in mock: {}", request.method),
                        data: None,
                    }),
                })
            }
        }
    }
    
    async fn send_notification(&self, _notification: JsonRpcNotification) -> Result<()> {
        Ok(())
    }
    
    async fn subscribe_to_notifications(&self, handler: NotificationHandler) -> Result<()> {
        let mut handler_guard = self.notification_handler.lock().unwrap();
        *handler_guard = Some(handler);
        Ok(())
    }
    
    async fn close(&self) -> Result<()> {
        // Nothing to do for mock
        Ok(())
    }
}
