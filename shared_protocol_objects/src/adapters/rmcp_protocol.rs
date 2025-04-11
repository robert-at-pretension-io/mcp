use crate::{
    JsonRpcRequest, JsonRpcResponse, JsonRpcNotification, JsonRpcError,
    InitializeParams, InitializeResult, ToolInfo, ListToolsParams, ListToolsResult, CallToolParams, CallToolResult,
    ClientInfo, Implementation, ClientCapabilities, ServerCapabilities, ToolResponseContent,
    ProgressParams,
    INVALID_PARAMS, METHOD_NOT_FOUND, INTERNAL_ERROR, SERVER_NOT_INITIALIZED, PROTOCOL_VERSION_MISMATCH,
};
use rmcp::model::{
    self as sdk, ClientJsonRpcMessage, ServerJsonRpcMessage, Notification, RequestId as SdkId, ErrorCode as SdkErrorCode,
    Request as SdkRequest, Response as SdkResponse, Error as SdkError, NumberOrString, ProgressParams as SdkProgressParams,
    Initialize as SdkInitialize, ListTools as SdkListTools, CallTool as SdkCallTool,
    InitializeResult as SdkInitializeResult, ListToolsResult as SdkListToolsResult, CallToolResult as SdkCallToolResult,
    Tool as SdkTool, ToolContent as SdkToolContent, ClientInfo as SdkClientInfo, ServerInfo as SdkServerInfo,
    ClientCapabilities as SdkClientCapabilities, ServerCapabilities as SdkServerCapabilities,
};
use serde_json::{Value, json};
use anyhow::{anyhow, Result, Context};
use std::sync::Arc;
use tracing::{debug, warn, error, trace, instrument};

// Current protocol version supported by our implementation
pub const CURRENT_PROTOCOL_VERSION: &str = "1.0";

/// Adapter that handles conversion between our protocol objects and SDK objects.
#[derive(Debug, Default)]
pub struct RmcpProtocolAdapter;

impl RmcpProtocolAdapter {
    /// Convert our JsonRpcRequest to SDK ClientJsonRpcMessage.
    /// Returns Result to handle potential conversion errors (e.g., invalid params).
    #[instrument(skip(request), level = "debug")]
    pub fn to_sdk_request(request: &JsonRpcRequest) -> Result<ClientJsonRpcMessage> {
        let sdk_id = convert_id_to_sdk(&request.id)
            .context("Failed to convert request ID to SDK format")?;
        let params = request.params.clone().unwrap_or(Value::Null);
        
        trace!("Converting request method '{}' to SDK format", request.method);

        match request.method.as_str() {
            "initialize" => {
                debug!("Processing 'initialize' request");
                let our_params: InitializeParams = serde_json::from_value(params)
                    .context("Failed to parse InitializeParams")?;
                
                // Check protocol version compatibility
                Self::check_protocol_version(&our_params.protocol_version)?;
                
                Ok(ClientJsonRpcMessage::Initialize(SdkInitialize {
                    id: sdk_id,
                    protocol_version: our_params.protocol_version,
                    capabilities: convert_capabilities_to_sdk(&our_params.capabilities)
                        .context("Failed to convert client capabilities")?,
                    client_info: convert_client_info_to_sdk(&our_params.client_info),
                    // Add any other fields supported by the SDK
                }))
            },
            "tools/list" => {
                debug!("Processing 'tools/list' request");
                let our_params: Option<ListToolsParams> = serde_json::from_value(params).ok();
                Ok(ClientJsonRpcMessage::ListTools(SdkListTools {
                    id: sdk_id,
                    cursor: our_params.and_then(|p| p.cursor),
                }))
            },
            "tools/call" => {
                debug!("Processing 'tools/call' request");
                let our_params: CallToolParams = serde_json::from_value(params)
                    .context("Failed to parse CallToolParams")?;
                Ok(ClientJsonRpcMessage::CallTool(SdkCallTool {
                    id: sdk_id,
                    name: our_params.name,
                    arguments: our_params.arguments,
                }))
            },
            "shutdown" => {
                debug!("Processing 'shutdown' request");
                Ok(ClientJsonRpcMessage::Shutdown)
            },
            "exit" => {
                debug!("Processing 'exit' request");
                Ok(ClientJsonRpcMessage::Exit)
            },
            // Generic request for methods not specifically handled
            _ => {
                debug!("Processing generic request method: {}", request.method);
                Ok(ClientJsonRpcMessage::Request(SdkRequest {
                    id: sdk_id,
                    method: request.method.clone(),
                    params: request.params.clone().unwrap_or(Value::Null),
                }))
            }
        }
    }

    /// Convert SDK ServerJsonRpcMessage to our JsonRpcResponse.
    /// Returns Result to handle potential conversion errors.
    #[instrument(skip(response), level = "debug")]
    pub fn from_sdk_response(response: ServerJsonRpcMessage) -> Result<JsonRpcResponse> {
        match response {
            ServerJsonRpcMessage::InitializeResult(res) => {
                debug!("Converting SDK InitializeResult to our JsonRpcResponse");
                let our_id = convert_id_from_sdk(&res.id);
                
                // Check protocol version compatibility
                Self::check_protocol_version(&res.protocol_version)?;
                
                let our_result = InitializeResult {
                    protocol_version: res.protocol_version,
                    capabilities: convert_capabilities_from_sdk(&res.capabilities)
                        .context("Failed to convert server capabilities")?,
                    server_info: convert_implementation_from_sdk(&res.server_info),
                    instructions: res.instructions,
                };
                
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: Some(serde_json::to_value(our_result)
                        .context("Failed to serialize InitializeResult")?),
                    error: None,
                })
            },
            ServerJsonRpcMessage::ListToolsResult(res) => {
                debug!("Converting SDK ListToolsResult to our JsonRpcResponse");
                let our_id = convert_id_from_sdk(&res.id);
                let our_result = ListToolsResult {
                    tools: res.tools.into_iter()
                        .map(convert_tool_info_from_sdk)
                        .collect::<Result<Vec<_>>>()
                        .context("Failed to convert tool information")?,
                    next_cursor: res.cursor,
                };
                
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: Some(serde_json::to_value(our_result)
                        .context("Failed to serialize ListToolsResult")?),
                    error: None,
                })
            },
            ServerJsonRpcMessage::CallToolResult(res) => {
                debug!("Converting SDK CallToolResult to our JsonRpcResponse");
                let our_id = convert_id_from_sdk(&res.id);
                let our_result = CallToolResult {
                    content: res.content.into_iter()
                        .map(convert_tool_response_content_from_sdk)
                        .collect::<Result<Vec<_>>>()
                        .context("Failed to convert tool response content")?,
                };
                
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: Some(serde_json::to_value(our_result)
                        .context("Failed to serialize CallToolResult")?),
                    error: None,
                })
            },
            ServerJsonRpcMessage::Response(res) => {
                debug!("Converting SDK generic Response to our JsonRpcResponse");
                let our_id = convert_id_from_sdk(&res.id);
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: Some(res.result),
                    error: None,
                })
            },
            ServerJsonRpcMessage::Error(err_res) => {
                debug!("Converting SDK Error to our JsonRpcResponse with error");
                let our_id = convert_id_from_sdk(&err_res.id);
                let our_error = convert_error_from_sdk(&err_res.error);
                
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: None,
                    error: Some(our_error),
                })
            },
            // Handle other potential SDK response types
            _ => Err(anyhow!("Unsupported SDK ServerJsonRpcMessage variant: {:?}", response)),
        }
    }

    /// Convert SDK Notification to our JsonRpcNotification.
    #[instrument(skip(notification), level = "debug")]
    pub fn from_sdk_notification(notification: sdk::Notification) -> Result<JsonRpcNotification> {
        match notification {
            sdk::Notification::Progress(params) => {
                debug!("Converting SDK Progress notification");
                let our_params = ProgressParams {
                    token: params.token,
                    value: params.value,
                };
                
                Ok(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: "$/progress".to_string(),
                    params: Some(serde_json::to_value(our_params)
                        .context("Failed to serialize ProgressParams")?),
                })
            },
            sdk::Notification::ShowMessage(params) => {
                debug!("Converting SDK ShowMessage notification");
                Ok(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: "window/showMessage".to_string(),
                    params: Some(serde_json::to_value(params)
                        .context("Failed to serialize ShowMessage params")?),
                })
            },
            sdk::Notification::LogMessage(params) => {
                debug!("Converting SDK LogMessage notification");
                Ok(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: "window/logMessage".to_string(),
                    params: Some(serde_json::to_value(params)
                        .context("Failed to serialize LogMessage params")?),
                })
            },
            sdk::Notification::CancelRequest(params) => {
                debug!("Converting SDK CancelRequest notification");
                Ok(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: "$/cancelRequest".to_string(),
                    params: Some(serde_json::to_value(params)
                        .context("Failed to serialize CancelRequest params")?),
                })
            },
            sdk::Notification::Generic { method, params } => {
                debug!("Converting SDK Generic notification with method: {}", method);
                Ok(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method,
                    params: Some(params),
                })
            },
            // Catch-all for unhandled specific notification variants
            _ => Err(anyhow!("Unsupported SDK Notification variant: {:?}", notification)),
        }
    }

    /// Convert our JsonRpcNotification to SDK ClientJsonRpcMessage (Notification variant).
    #[instrument(skip(notification), level = "debug")]
    pub fn to_sdk_notification(notification: &JsonRpcNotification) -> Result<ClientJsonRpcMessage> {
        let params = notification.params.clone().unwrap_or(Value::Null);

        match notification.method.as_str() {
            "notifications/initialized" => {
                debug!("Converting 'notifications/initialized' to SDK message");
                Ok(ClientJsonRpcMessage::NotificationsInitialized)
            },
            "$/progress" => {
                debug!("Converting '$/progress' notification to SDK format");
                let our_params: ProgressParams = serde_json::from_value(params)
                    .context("Failed to parse ProgressParams for notification")?;
                
                let sdk_params = SdkProgressParams {
                    token: our_params.token,
                    value: our_params.value,
                };
                
                Ok(ClientJsonRpcMessage::Notification(sdk::Notification::Progress(sdk_params)))
            },
            "$/cancelRequest" => {
                debug!("Converting '$/cancelRequest' notification to SDK format");
                let params_map = params.as_object()
                    .ok_or_else(|| anyhow!("cancelRequest params must be an object"))?;
                
                let id_value = params_map.get("id")
                    .ok_or_else(|| anyhow!("cancelRequest params missing 'id' field"))?;
                
                let sdk_id = convert_id_to_sdk(id_value)
                    .context("Failed to convert cancelRequest ID to SDK format")?;
                
                Ok(ClientJsonRpcMessage::Notification(sdk::Notification::CancelRequest {
                    id: sdk_id,
                }))
            },
            "exit" => {
                debug!("Converting 'exit' notification to SDK Exit message");
                Ok(ClientJsonRpcMessage::Exit)
            },
            // Default to generic notification for unhandled methods
            _ => {
                debug!("Converting generic notification '{}' to SDK format", notification.method);
                Ok(ClientJsonRpcMessage::Notification(sdk::Notification::Generic {
                    method: notification.method.clone(),
                    params: params,
                }))
            }
        }
    }
    
    /// Check if the provided protocol version is compatible with our implementation
    #[instrument(level = "debug")]
    fn check_protocol_version(version: &str) -> Result<()> {
        debug!("Checking protocol version compatibility: received '{}', supported '{}'", 
               version, CURRENT_PROTOCOL_VERSION);
        
        // Simple version check - in a real implementation, this might be more sophisticated
        if version != CURRENT_PROTOCOL_VERSION {
            error!("Protocol version mismatch: received '{}', supported '{}'", 
                   version, CURRENT_PROTOCOL_VERSION);
            
            // Return error but don't panic - the caller can decide how to handle this
            return Err(anyhow!(
                "Protocol version mismatch: received '{}', supported '{}'",
                version, CURRENT_PROTOCOL_VERSION
            ));
        }
        
        Ok(())
    }
}

// --- Helper Conversion Functions ---

/// Convert our Value ID to SDK RequestId
#[instrument(skip(id), level = "trace")]
fn convert_id_to_sdk(id: &Value) -> Result<SdkId> {
    match id {
        Value::Number(n) => {
            if let Some(i) = n.as_u64() {
                // SDK uses u32 for number IDs
                if i <= u32::MAX as u64 {
                    trace!("Converting number ID {} to SDK u32", i);
                    Ok(NumberOrString::Number(i as u32))
                } else {
                    // Convert large numbers to string ID
                    trace!("Converting large number ID {} to SDK string", i);
                    Ok(NumberOrString::String(i.to_string().into()))
                }
            } else if let Some(f) = n.as_f64() {
                // Handle floats by converting to string
                warn!("Converting float ID {} to SDK string (potential precision loss)", f);
                Ok(NumberOrString::String(f.to_string().into()))
            } else {
                Err(anyhow!("Numeric ID {} cannot be represented as u32 or string", n))
            }
        },
        Value::String(s) => {
            trace!("Converting string ID '{}' to SDK string", s);
            Ok(NumberOrString::String(s.clone().into()))
        },
        Value::Null => {
            warn!("Received null ID, SDK does not support null IDs");
            Err(anyhow!("SDK does not support null IDs"))
        },
        _ => {
            error!("Unsupported JSON-RPC ID type: {:?}", id);
            Err(anyhow!("Unsupported JSON-RPC ID type for SDK conversion: {:?}", id))
        },
    }
}

/// Convert SDK RequestId to our Value format
#[instrument(skip(id), level = "trace")]
fn convert_id_from_sdk(id: &SdkId) -> Value {
    match id {
        NumberOrString::Number(n) => {
            trace!("Converting SDK number ID {} to Value", n);
            json!(n)
        },
        NumberOrString::String(s) => {
            trace!("Converting SDK string ID '{}' to Value", s);
            json!(s.as_ref())
        },
    }
}

/// Convert our ClientInfo to SDK ClientInfo
#[instrument(skip(info), level = "trace")]
fn convert_client_info_to_sdk(info: &ClientInfo) -> sdk::ClientInfo {
    trace!("Converting ClientInfo '{}' v{} to SDK format", info.name, info.version);
    sdk::ClientInfo {
        name: info.name.clone(),
        version: info.version.clone(),
        // Add other fields if they exist in both structs
    }
}

/// Convert SDK ServerInfo to our Implementation type
#[instrument(skip(info), level = "trace")]
fn convert_implementation_from_sdk(info: &sdk::ServerInfo) -> Implementation {
    trace!("Converting SDK ServerInfo '{}' v{} to our Implementation", info.name, info.version);
    Implementation {
        name: info.name.clone(),
        version: info.version.clone(),
        // Add any additional fields needed
    }
}

/// Convert our ClientCapabilities to SDK ClientCapabilities
#[instrument(skip(caps), level = "debug")]
fn convert_capabilities_to_sdk(caps: &ClientCapabilities) -> Result<sdk::ClientCapabilities> {
    debug!("Converting ClientCapabilities to SDK format");
    
    // Implement the detailed mapping between capability structures
    // This would be expanded based on the actual fields in both structures
    Ok(sdk::ClientCapabilities {
        progress: caps.progress.clone(),
        // Map other fields from our capabilities to SDK capabilities
        // For example:
        // workspace: caps.workspace.as_ref().map(convert_workspace_caps_to_sdk),
        // text_document: caps.text_document.as_ref().map(convert_text_document_caps_to_sdk),
    })
}

/// Convert SDK ServerCapabilities to our ServerCapabilities
#[instrument(skip(caps), level = "debug")]
fn convert_capabilities_from_sdk(caps: &sdk::ServerCapabilities) -> Result<ServerCapabilities> {
    debug!("Converting SDK ServerCapabilities to our format");
    
    // Implement the detailed mapping between capability structures
    // This would be expanded based on the actual fields in both structures
    Ok(ServerCapabilities {
        tool_provider: caps.tool_provider.clone(),
        progress_provider: caps.progress_provider.clone(),
        // Map other fields from SDK capabilities to our capabilities
    })
}

/// Convert SDK Tool to our ToolInfo
#[instrument(skip(tool), level = "debug")]
fn convert_tool_info_from_sdk(tool: sdk::Tool) -> Result<ToolInfo> {
    debug!("Converting SDK Tool '{}' to our ToolInfo", tool.name);
    
    Ok(ToolInfo {
        name: tool.name,
        description: tool.description,
        input_schema: tool.schema,
        annotations: None, // SDK Tool doesn't have annotations in the example
    })
}

/// Convert SDK ToolContent to our ToolResponseContent
#[instrument(skip(content), level = "debug")]
fn convert_tool_response_content_from_sdk(content: sdk::ToolContent) -> Result<ToolResponseContent> {
    debug!("Converting SDK ToolContent to our ToolResponseContent");
    
    Ok(ToolResponseContent {
        type_: content.type_,
        text: content.text,
        annotations: None, // Add if SDK has annotations
    })
}

/// Convert SDK Error to our JsonRpcError
#[instrument(skip(error), level = "debug")]
fn convert_error_from_sdk(error: &SdkError) -> JsonRpcError {
    debug!("Converting SDK Error (code {}) to our JsonRpcError", error.code.0);
    
    JsonRpcError {
        code: convert_error_code_from_sdk(error.code),
        message: error.message.clone(),
        data: error.data.clone(),
    }
}

/// Convert SDK ErrorCode to our error code (i64)
#[instrument(level = "debug")]
fn convert_error_code_from_sdk(code: SdkErrorCode) -> i64 {
    match code {
        // Standard JSON-RPC codes
        SdkErrorCode::PARSE_ERROR => crate::PARSE_ERROR,
        SdkErrorCode::INVALID_REQUEST => crate::INVALID_REQUEST,
        SdkErrorCode::METHOD_NOT_FOUND => crate::METHOD_NOT_FOUND,
        SdkErrorCode::INVALID_PARAMS => crate::INVALID_PARAMS,
        SdkErrorCode::INTERNAL_ERROR => crate::INTERNAL_ERROR,
        
        // LSP and RMCP specific codes
        SdkErrorCode::SERVER_NOT_INITIALIZED => SERVER_NOT_INITIALIZED,
        SdkErrorCode::RESOURCE_NOT_FOUND => -32002, // Use literal if no constant defined
        
        // Protocol version mismatch (if SDK defines it)
        _ if code.0 == -32099 => PROTOCOL_VERSION_MISMATCH,
        
        // Default fallback for unmapped SDK codes
        _ => {
            warn!("Unmapped SDK error code received: {}. Falling back to INTERNAL_ERROR.", code.0);
            crate::INTERNAL_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_id_conversion() {
        // Test number ID conversion
        let number_id = json!(42);
        let sdk_id = convert_id_to_sdk(&number_id).unwrap();
        assert!(matches!(sdk_id, NumberOrString::Number(42)));
        let roundtrip_id = convert_id_from_sdk(&sdk_id);
        assert_eq!(roundtrip_id, number_id);
        
        // Test string ID conversion
        let string_id = json!("request-1");
        let sdk_id = convert_id_to_sdk(&string_id).unwrap();
        assert!(matches!(sdk_id, NumberOrString::String(s) if s.as_ref() == "request-1"));
        let roundtrip_id = convert_id_from_sdk(&sdk_id);
        assert_eq!(roundtrip_id, string_id);
        
        // Test large number ID conversion
        let large_number_id = json!(4294967296u64); // u32::MAX + 1
        let sdk_id = convert_id_to_sdk(&large_number_id).unwrap();
        assert!(matches!(sdk_id, NumberOrString::String(s) if s.as_ref() == "4294967296"));
    }
    
    #[test]
    fn test_protocol_version_check() {
        // Test matching version
        let result = RmcpProtocolAdapter::check_protocol_version(CURRENT_PROTOCOL_VERSION);
        assert!(result.is_ok());
        
        // Test mismatched version
        let result = RmcpProtocolAdapter::check_protocol_version("0.9");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Protocol version mismatch"));
    }
    
    // Additional tests would be implemented for other conversion functions
}
