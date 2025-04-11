use crate::{
    JsonRpcRequest, JsonRpcResponse, JsonRpcNotification, JsonRpcError,
    InitializeParams, InitializeResult, ToolInfo, ListToolsParams, ListToolsResult, CallToolParams, CallToolResult,
    ClientInfo, Implementation, ClientCapabilities, ServerCapabilities, ToolResponseContent, // Added missing types
    ProgressParams, // Added for progress notifications
    INVALID_PARAMS, METHOD_NOT_FOUND, INTERNAL_ERROR, // Error codes
};
use rmcp::model::{
    self as sdk, ClientJsonRpcMessage, ServerJsonRpcMessage, Notification, Id as SdkId, ErrorCode as SdkErrorCode,
    Request as SdkRequest, Response as SdkResponse, Error as SdkError, // Added missing SDK types
};
use serde_json::{Value, json};
use anyhow::{anyhow, Result}; // Use anyhow for conversion errors

/// Adapter that handles conversion between our protocol objects and SDK objects.
/// Note: This implementation assumes specific structures and might need adjustments
/// based on the exact definitions in both `crate` and `rmcp::model`.
pub struct RmcpProtocolAdapter;

impl RmcpProtocolAdapter {
    /// Convert our JsonRpcRequest to SDK ClientJsonRpcMessage.
    /// Returns Result to handle potential conversion errors (e.g., invalid params).
    pub fn to_sdk_request(request: &JsonRpcRequest) -> Result<ClientJsonRpcMessage> {
        let sdk_id = convert_id_to_sdk(&request.id)?;
        let params = request.params.clone().unwrap_or(Value::Null);

        match request.method.as_str() {
            "initialize" => {
                let our_params: InitializeParams = serde_json::from_value(params)
                    .map_err(|e| anyhow!("Failed to parse InitializeParams: {}", e))?;
                Ok(ClientJsonRpcMessage::Initialize(sdk::Initialize {
                    id: sdk_id,
                    protocol_version: our_params.protocol_version, // Assuming direct mapping
                    capabilities: convert_capabilities_to_sdk(&our_params.capabilities)?,
                    client_info: convert_client_info_to_sdk(&our_params.client_info),
                }))
            },
            "tools/list" => {
                // Our ListToolsParams might have a cursor, SDK might not, or vice-versa. Adjust as needed.
                let our_params: Option<ListToolsParams> = serde_json::from_value(params).ok(); // Optional params
                Ok(ClientJsonRpcMessage::ListTools(sdk::ListTools {
                    id: sdk_id,
                    cursor: our_params.and_then(|p| p.cursor), // Pass cursor if present
                    // Add other fields if the SDK ListTools request has them
                }))
            },
            "tools/call" => {
                 let our_params: CallToolParams = serde_json::from_value(params)
                    .map_err(|e| anyhow!("Failed to parse CallToolParams: {}", e))?;
                 Ok(ClientJsonRpcMessage::CallTool(sdk::CallTool {
                     id: sdk_id,
                     name: our_params.name,
                     arguments: our_params.arguments, // Assuming direct mapping of Value
                 }))
            }
            // Handle other specific methods if necessary...
            "shutdown" => Ok(ClientJsonRpcMessage::Shutdown), // Assuming SDK has a parameterless Shutdown variant
            "exit" => Ok(ClientJsonRpcMessage::Exit), // Assuming SDK has a parameterless Exit variant

            // Generic request for methods not specifically handled
            _ => {
                Ok(ClientJsonRpcMessage::Request(SdkRequest {
                    id: sdk_id,
                    method: request.method.clone(),
                    params: request.params.clone().unwrap_or(Value::Null), // Pass params as Value
                }))
            }
        }
    }

    /// Convert SDK ServerJsonRpcMessage to our JsonRpcResponse.
    /// Returns Result to handle potential conversion errors.
    pub fn from_sdk_response(response: ServerJsonRpcMessage) -> Result<JsonRpcResponse> {
        match response {
            ServerJsonRpcMessage::InitializeResult(res) => {
                let our_id = convert_id_from_sdk(&res.id)?;
                let our_result = InitializeResult {
                    protocol_version: res.protocol_version, // Assuming direct mapping
                    capabilities: convert_capabilities_from_sdk(&res.capabilities)?,
                    server_info: convert_implementation_from_sdk(&res.server_info),
                    instructions: res.instructions, // Assuming direct mapping
                };
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: Some(serde_json::to_value(our_result)?),
                    error: None,
                })
            },
            ServerJsonRpcMessage::ListToolsResult(res) => {
                let our_id = convert_id_from_sdk(&res.id)?;
                let our_result = ListToolsResult {
                    tools: res.tools.into_iter()
                        .map(convert_tool_info_from_sdk) // Use helper function
                        .collect::<Result<Vec<_>>>()?, // Collect results, propagating errors
                    next_cursor: res.cursor, // Assuming direct mapping
                };
                 Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: Some(serde_json::to_value(our_result)?),
                    error: None,
                })
            },
             ServerJsonRpcMessage::CallToolResult(res) => {
                let our_id = convert_id_from_sdk(&res.id)?;
                let our_result = CallToolResult {
                    content: res.content.into_iter()
                        .map(convert_tool_response_content_from_sdk) // Use helper function
                        .collect::<Result<Vec<_>>>()?, // Collect results
                };
                 Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: our_id,
                    result: Some(serde_json::to_value(our_result)?),
                    error: None,
                })
            },
            ServerJsonRpcMessage::Response(res) => {
                 // Generic response handling
                 let our_id = convert_id_from_sdk(&res.id)?;
                 Ok(JsonRpcResponse {
                     jsonrpc: "2.0".to_string(),
                     id: our_id,
                     result: Some(res.result), // Pass through Value
                     error: None,
                 })
            },
            ServerJsonRpcMessage::Error(err_res) => {
                 let our_id = convert_id_from_sdk(&err_res.id)?;
                 let our_error = convert_error_from_sdk(&err_res.error);
                 Ok(JsonRpcResponse {
                     jsonrpc: "2.0".to_string(),
                     id: our_id,
                     result: None,
                     error: Some(our_error),
                 })
            },
            // Handle other potential SDK response types if necessary
            _ => Err(anyhow!("Unsupported SDK response type received: {:?}", response)),
        }
    }

    /// Convert SDK Notification to our JsonRpcNotification.
    /// Returns Result for potential conversion errors.
    pub fn from_sdk_notification(notification: Notification) -> JsonRpcNotification {
        // This requires knowing the specific notification types in the SDK
        match notification {
            Notification::Progress(params) => {
                // Assuming SDK Progress params map directly or require conversion
                let our_params = ProgressParams {
                    token: params.token, // Assuming field names match
                    value: params.value, // Assuming field names match
                };
                JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: "$/progress".to_string(), // Standard LSP progress notification method
                    params: Some(serde_json::to_value(our_params).unwrap_or(Value::Null)),
                }
            },
            // Handle other notification types like textDocument/publishDiagnostics if applicable
            Notification::Generic { method, params } => {
                 JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method,
                    params: Some(params),
                }
            }
            // _ => {
            //     // Log or handle unknown notifications
            //     tracing::warn!("Received unknown SDK notification type: {:?}", notification);
            //     // Create a generic notification or ignore, depending on requirements
            //     JsonRpcNotification {
            //         jsonrpc: "2.0".to_string(),
            //         method: "unknown/notification".to_string(),
            //         params: None,
            //     }
            // }
        }
    }

    /// Convert our JsonRpcNotification to SDK ClientJsonRpcMessage (Notification variant).
    /// Returns Result for potential conversion errors.
    pub fn to_sdk_notification(notification: &JsonRpcNotification) -> Result<ClientJsonRpcMessage> {
        let params = notification.params.clone().unwrap_or(Value::Null);

        match notification.method.as_str() {
            "$/progress" => {
                let our_params: ProgressParams = serde_json::from_value(params)
                    .map_err(|e| anyhow!("Failed to parse ProgressParams for notification: {}", e))?;
                // Assuming SDK has a matching Progress variant in ClientJsonRpcMessage::Notification
                // If not, adapt to use ClientJsonRpcMessage::Notification(sdk::Notification::Generic { ... })
                 Ok(ClientJsonRpcMessage::Notification(sdk::Notification::Progress(sdk::ProgressParams {
                     token: our_params.token,
                     value: our_params.value,
                 })))
            },
            "exit" => Ok(ClientJsonRpcMessage::Exit), // Map to SDK Exit if it's parameterless
            // Handle other specific notification methods...
            _ => {
                // Generic notification
                Ok(ClientJsonRpcMessage::Notification(sdk::Notification::Generic {
                    method: notification.method.clone(),
                    params: params,
                }))
            }
        }
    }
}

// --- Helper Conversion Functions ---

// ID Conversion
fn convert_id_to_sdk(id: &Value) -> Result<SdkId> {
    match id {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(SdkId::Num(i))
            } else {
                // Handle potential precision loss or invalid number for SDK ID type
                Err(anyhow!("Numeric ID cannot be represented as i64: {}", n))
            }
        },
        Value::String(s) => Ok(SdkId::Str(s.clone())),
        Value::Null => Ok(SdkId::Null), // Assuming SDK supports Null ID, otherwise error
        _ => Err(anyhow!("Unsupported JSON-RPC ID type for SDK conversion: {:?}", id)),
    }
}

fn convert_id_from_sdk(id: &SdkId) -> Result<Value> {
    match id {
        SdkId::Num(i) => Ok(json!(i)),
        SdkId::Str(s) => Ok(json!(s)),
        SdkId::Null => Ok(Value::Null),
        // Handle potential future SDK ID types if necessary
    }
}

// ClientInfo / Implementation Conversion
fn convert_client_info_to_sdk(info: &ClientInfo) -> sdk::ClientInfo {
    sdk::ClientInfo {
        name: info.name.clone(),
        version: info.version.clone(),
        // Map other fields if they exist in both structs
    }
}

fn convert_implementation_from_sdk(info: &sdk::ServerInfo) -> Implementation {
    Implementation {
        name: info.name.clone(),
        version: info.version.clone(),
        // Map other fields
    }
}


// Capabilities Conversion (Requires detailed knowledge of both capability structures)
fn convert_capabilities_to_sdk(caps: &ClientCapabilities) -> Result<sdk::ClientCapabilities> {
    // Example: Map fields directly if they match. Add logic for differences.
    Ok(sdk::ClientCapabilities {
        // workspace: caps.workspace.as_ref().map(convert_workspace_caps_to_sdk),
        // text_document: caps.text_document.as_ref().map(convert_text_document_caps_to_sdk),
        // experimental: caps.experimental.clone(), // Pass through if Value or similar
        // ... other capability fields
        // This needs to be implemented based on actual capability fields in both structs.
        // Return Err(...) if conversion is not possible or ambiguous.
        progress: caps.progress.clone(), // Assuming direct mapping for simplicity
    })
}

fn convert_capabilities_from_sdk(caps: &sdk::ServerCapabilities) -> Result<ServerCapabilities> {
    // Example: Map fields directly if they match. Add logic for differences.
    Ok(ServerCapabilities {
        // text_document_sync: caps.text_document_sync.map(convert_sync_options_from_sdk),
        // completion_provider: caps.completion_provider.map(convert_completion_options_from_sdk),
        // experimental: caps.experimental.clone(),
        // ... other capability fields
        // This needs to be implemented based on actual capability fields.
        // Return Err(...) if conversion is not possible.
        tool_provider: caps.tool_provider.clone(), // Assuming direct mapping
        progress_provider: caps.progress_provider.clone(), // Assuming direct mapping
    })
}

// ToolInfo Conversion
fn convert_tool_info_from_sdk(tool: sdk::Tool) -> Result<ToolInfo> {
    Ok(ToolInfo {
        name: tool.name,
        description: tool.description, // Assuming Option<String> matches
        input_schema: tool.schema, // Assuming schema type (Value) matches
        annotations: None, // SDK Tool doesn't seem to have annotations in the plan example
    })
}

// ToolResponseContent Conversion
fn convert_tool_response_content_from_sdk(content: sdk::ToolContent) -> Result<ToolResponseContent> {
    // This depends heavily on how sdk::ToolContent is defined.
    // Assuming it has fields like `type_` and `text` similar to ours.
    Ok(ToolResponseContent {
         type_: content.type_, // Adjust field name if different in SDK
         text: content.text,   // Adjust field name if different in SDK
         annotations: None, // Handle if SDK has annotations
    })
    // If sdk::ToolContent is an enum, match on its variants.
    // Example:
    // match content {
    //     sdk::ToolContent::Text { text } => Ok(ToolResponseContent { type_: "text".to_string(), text, annotations: None }),
    //     sdk::ToolContent::Json { data } => Ok(ToolResponseContent { type_: "json".to_string(), text: serde_json::to_string(&data)?, annotations: None }),
    //     // ... other variants
    // }
}


// Error Conversion
fn convert_error_from_sdk(error: &SdkError) -> JsonRpcError {
    JsonRpcError {
        code: convert_error_code_from_sdk(error.code),
        message: error.message.clone(),
        data: error.data.clone(), // Pass through data if present
    }
}

fn convert_error_code_from_sdk(code: SdkErrorCode) -> i64 {
    // Map SDK error codes to our JSON-RPC error codes
    match code {
        SdkErrorCode::ParseError => crate::PARSE_ERROR,
        SdkErrorCode::InvalidRequest => crate::INVALID_REQUEST,
        SdkErrorCode::MethodNotFound => crate::METHOD_NOT_FOUND,
        SdkErrorCode::InvalidParams => crate::INVALID_PARAMS,
        SdkErrorCode::InternalError => crate::INTERNAL_ERROR,
        SdkErrorCode::ServerError(_) => crate::INTERNAL_ERROR, // Map generic server errors
        SdkErrorCode::RequestCancelled => crate::REQUEST_CANCELLED,
        SdkErrorCode::ContentModified => crate::CONTENT_MODIFIED,
        // Add mappings for any other specific codes in the SDK
        _ => crate::INTERNAL_ERROR, // Default fallback
    }
}

// Note: Conversion functions for complex nested types within capabilities
// (like WorkspaceCapabilities, TextDocumentCapabilities, etc.) would need
// similar helper functions if their structures differ significantly.
