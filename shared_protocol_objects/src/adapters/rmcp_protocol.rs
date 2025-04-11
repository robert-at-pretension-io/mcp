use crate::{
    JsonRpcRequest, JsonRpcResponse, JsonRpcNotification, JsonRpcError,
    InitializeParams, InitializeResult, ToolInfo, ListToolsParams, ListToolsResult, CallToolParams, CallToolResult,
    ClientInfo, Implementation, ClientCapabilities, ServerCapabilities, ToolResponseContent, // Added missing types
    ProgressParams, // Added for progress notifications
    INVALID_PARAMS, METHOD_NOT_FOUND, INTERNAL_ERROR, // Error codes
};
use rmcp::model::{
    self as sdk, ClientJsonRpcMessage, ServerJsonRpcMessage, Notification, RequestId as SdkId, ErrorCode as SdkErrorCode,
    Request as SdkRequest, Response as SdkResponse, Error as SdkError, NumberOrString, ProgressParams as SdkProgressParams,
    Initialize as SdkInitialize, ListTools as SdkListTools, CallTool as SdkCallTool, // Import specific request/result types
    InitializeResult as SdkInitializeResult, ListToolsResult as SdkListToolsResult, CallToolResult as SdkCallToolResult,
    Tool as SdkTool, ToolContent as SdkToolContent, ClientInfo as SdkClientInfo, ServerInfo as SdkServerInfo,
    ClientCapabilities as SdkClientCapabilities, ServerCapabilities as SdkServerCapabilities,
};
use serde_json::{Value, json};
use anyhow::{anyhow, Result}; // Use anyhow for conversion errors
use std::sync::Arc; // For SDK ID String variant

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
                Ok(ClientJsonRpcMessage::Initialize(SdkInitialize { // Use SDK type directly
                    id: sdk_id,
                    protocol_version: our_params.protocol_version, // Assuming direct mapping
                    capabilities: convert_capabilities_to_sdk(&our_params.capabilities)?,
                    client_info: convert_client_info_to_sdk(&our_params.client_info),
                    // Map other fields like process_id, root_uri if they exist in our_params and SdkInitialize
                }))
            },
            "tools/list" => {
                // Guide example shows parameterless ListTools request
                let our_params: Option<ListToolsParams> = serde_json::from_value(params).ok(); // Still parse ours if needed
                Ok(ClientJsonRpcMessage::ListTools(SdkListTools { // Use SDK type directly
                    id: sdk_id,
                    cursor: our_params.and_then(|p| p.cursor), // Pass cursor if present in ours and SDK supports it
                    // Add other fields if the SDK ListTools request has them
                }))
            },
            "tools/call" => {
                 let our_params: CallToolParams = serde_json::from_value(params)
                    .map_err(|e| anyhow!("Failed to parse CallToolParams: {}", e))?;
                 Ok(ClientJsonRpcMessage::CallTool(SdkCallTool { // Use SDK type directly
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
            ServerJsonRpcMessage::InitializeResult(res) => { // res is SdkInitializeResult
                let our_id = convert_id_from_sdk(&res.id); // Use updated converter
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
            ServerJsonRpcMessage::ListToolsResult(res) => { // res is SdkListToolsResult
                let our_id = convert_id_from_sdk(&res.id); // Use updated converter
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
             ServerJsonRpcMessage::CallToolResult(res) => { // res is SdkCallToolResult
                let our_id = convert_id_from_sdk(&res.id); // Use updated converter
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
            ServerJsonRpcMessage::Response(res) => { // res is SdkResponse
                 // Generic response handling
                 let our_id = convert_id_from_sdk(&res.id); // Use updated converter
                 Ok(JsonRpcResponse {
                     jsonrpc: "2.0".to_string(),
                     id: our_id,
                     result: Some(res.result), // Pass through Value
                     error: None,
                 })
            },
            ServerJsonRpcMessage::Error(err_res) => { // err_res is SdkErrorResponse
                 let our_id = convert_id_from_sdk(&err_res.id); // Use updated converter
                 let our_error = convert_error_from_sdk(&err_res.error); // Use updated converter
                 Ok(JsonRpcResponse {
                     jsonrpc: "2.0".to_string(),
                     id: our_id,
                     result: None,
                     error: Some(our_error),
                 })
            },
            // Handle other potential SDK response types if necessary
             _ => Err(anyhow!("Unsupported SDK ServerJsonRpcMessage variant received: {:?}", response)),
        }
    }

    /// Convert SDK Notification to our JsonRpcNotification. (Based on Section 2.4)
    /// Returns Result for potential conversion errors.
    pub fn from_sdk_notification(notification: sdk::Notification) -> Result<JsonRpcNotification> {
        // This requires knowing the specific notification types in the SDK
        match notification {
            sdk::Notification::Progress(params) => { // params is SdkProgressParams
                // Map SDK ProgressParams fields to our ProgressParams fields
                let our_params = ProgressParams {
                    // Adjust field names based on actual struct definitions
                    token: params.token, // Assuming 'token' exists in SdkProgressParams
                    value: params.value, // Assuming 'value' exists in SdkProgressParams
                };
                Ok(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method: "$/progress".to_string(), // Standard LSP progress notification method
                    params: Some(serde_json::to_value(our_params)?), // Use ? for serialization result
                })
            },
            // Handle other specific notification types defined in sdk::Notification enum...
            // e.g., sdk::Notification::LogMessage(params) => { ... }
            // e.g., sdk::Notification::ShowMessage(params) => { ... }

            // Handle generic notifications if the SDK uses them
            sdk::Notification::Generic { method, params } => {
                 Ok(JsonRpcNotification {
                    jsonrpc: "2.0".to_string(),
                    method, // Pass method name through
                    params: Some(params), // Pass params Value through
                })
            }
             // Catch-all for unhandled specific notification variants from the SDK
             _ => Err(anyhow!("Unsupported SDK Notification variant received: {:?}", notification)),
        }
    }

    /// Convert our JsonRpcNotification to SDK ClientJsonRpcMessage (Notification variant). (Based on Section 2.4)
    /// Returns Result for potential conversion errors.
    pub fn to_sdk_notification(notification: &JsonRpcNotification) -> Result<ClientJsonRpcMessage> {
        let params = notification.params.clone().unwrap_or(Value::Null);

        match notification.method.as_str() {
             // Handle specific notifications mentioned in the guide
             "notifications/initialized" => {
                 // Guide maps this to a specific SDK variant, not a generic notification
                 Ok(ClientJsonRpcMessage::NotificationsInitialized)
             },
            "$/progress" => {
                let our_params: ProgressParams = serde_json::from_value(params)
                    .map_err(|e| anyhow!("Failed to parse ProgressParams for notification: {}", e))?;
                // Map our ProgressParams fields to SDK SdkProgressParams fields
                let sdk_params = SdkProgressParams {
                    // Adjust field names based on actual struct definitions
                    token: our_params.token, // Assuming 'token' maps
                    value: our_params.value, // Assuming 'value' maps
                    // Map other fields if they exist and differ...
                };
                 // Wrap in the SDK's Notification::Progress variant
                 Ok(ClientJsonRpcMessage::Notification(sdk::Notification::Progress(sdk_params)))
            },
            "exit" => Ok(ClientJsonRpcMessage::Exit), // Map to SDK Exit if it's parameterless
            // Handle other specific notification methods if needed...

            // Default to generic notification for unhandled methods
            _ => {
                Ok(ClientJsonRpcMessage::Notification(sdk::Notification::Generic {
                    method: notification.method.clone(),
                    params: params,
                    // extensions: Default::default(), // Add if sdk::Notification::Generic has extensions
                }))
            }
        }
    }
}

// --- Helper Conversion Functions ---

// ID Conversion (Based on Section 2.1 of the new guide)
fn convert_id_to_sdk(id: &Value) -> Result<SdkId> {
    match id {
        Value::Number(n) => {
            if let Some(i) = n.as_u64() {
                // SDK uses u32 for number IDs according to guide's NumberOrString
                if i <= u32::MAX as u64 {
                    Ok(NumberOrString::Number(i as u32))
                } else {
                    // Convert large numbers to string ID as per guide
                    Ok(NumberOrString::String(i.to_string().into()))
                }
            } else if let Some(f) = n.as_f64() {
                 // Handle potential floats if necessary, maybe convert to string or error
                 Err(anyhow!("Numeric float ID cannot be represented as u32: {}", f))
            }
            else {
                Err(anyhow!("Numeric ID cannot be represented as u32: {}", n))
            }
        },
        Value::String(s) => Ok(NumberOrString::String(s.clone().into())), // Use Arc<str> via .into()
        Value::Null => Err(anyhow!("SDK does not support null IDs")), // Guide explicitly states no null ID support
        _ => Err(anyhow!("Unsupported JSON-RPC ID type for SDK conversion: {:?}", id)),
    }
}

fn convert_id_from_sdk(id: &SdkId) -> Value { // Result not needed if conversion always succeeds
    match id {
        NumberOrString::Number(n) => json!(n), // Convert u32 to JSON number
        NumberOrString::String(s) => json!(s.as_ref()), // Convert Arc<str> to JSON string
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

fn convert_error_code_from_sdk(code: SdkErrorCode) -> i64 { // Return i64 for our JsonRpcError
    // Map SDK error codes (constants from SdkErrorCode struct) to our JSON-RPC error codes (i64 constants)
    // Based on Section 1 of the new guide
    match code {
        // Standard JSON-RPC codes
        SdkErrorCode::PARSE_ERROR => crate::PARSE_ERROR,
        SdkErrorCode::INVALID_REQUEST => crate::INVALID_REQUEST,
        SdkErrorCode::METHOD_NOT_FOUND => crate::METHOD_NOT_FOUND,
        SdkErrorCode::INVALID_PARAMS => crate::INVALID_PARAMS,
        SdkErrorCode::INTERNAL_ERROR => crate::INTERNAL_ERROR,

        // LSP specific codes (Map if SDK uses them and we have equivalents)
        // SdkErrorCode::SERVER_ERROR_START..=SdkErrorCode::SERVER_ERROR_END => crate::INTERNAL_ERROR, // Example range mapping
        // SdkErrorCode::SERVER_NOT_INITIALIZED => crate::SERVER_NOT_INITIALIZED, // Add if defined
        // SdkErrorCode::UNKNOWN_ERROR_CODE => crate::INTERNAL_ERROR, // Add if defined

        // RMCP specific codes (Map if SDK uses them and we have equivalents)
        SdkErrorCode::RESOURCE_NOT_FOUND => -32002, // Use literal if no constant defined in crate
        // SdkErrorCode::REQUEST_CANCELLED => crate::REQUEST_CANCELLED, // Add if defined
        // SdkErrorCode::CONTENT_MODIFIED => crate::CONTENT_MODIFIED, // Add if defined

        // Default fallback for unmapped SDK codes
        _ => {
            tracing::warn!("Unmapped SDK error code received: {}. Falling back to INTERNAL_ERROR.", code.0);
            crate::INTERNAL_ERROR
        }
    }
}

// Note: Conversion functions for complex nested types within capabilities
// (like WorkspaceCapabilities, TextDocumentCapabilities, etc.) would need
// similar helper functions if their structures differ significantly.
