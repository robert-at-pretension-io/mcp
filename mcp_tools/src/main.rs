use futures::StreamExt;
use std::future::Future;
use std::pin::Pin;
use mcp_tools::aider::{handle_aider_tool_call, AiderParams};
use mcp_tools::bash::{handle_quick_bash, BashExecutor, BashParams, QuickBashParams};
use mcp_tools::brave_search::BraveSearchClient;
use mcp_tools::email_validator::handle_neverbounce_tool_call;
use mcp_tools::git_integration::handle_git_tool_call;
use mcp_tools::gmail_integration::handle_gmail_tool_call;
use mcp_tools::long_running_task::{handle_long_running_tool_call, LongRunningTaskManager};
use mcp_tools::oracle_tool::handle_oracle_select_tool_call;
use mcp_tools::process_html::extract_text_from_html;
use mcp_tools::regex_replace::handle_regex_replace_tool_call;
use mcp_tools::scraping_bee::{ScrapingBeeClient, ScrapingBeeResponse};
use mcp_tools::tool_impls::{create_tools, LongRunningTaskTool};
use mcp_tools::tool_trait::{Tool, standard_error_response};
use serde_json::{json, Value};
use shared_protocol_objects::{
    create_notification, error_response, success_response, CallToolParams, CallToolResult, 
    ClientCapabilities, Implementation, InitializeResult, JsonRpcRequest, JsonRpcResponse, 
    ListResourcesResult, ListToolsResult, PromptsCapability, ReadResourceParams, ReadResourceResult, 
    ResourceContent, ResourceInfo, ResourcesCapability, ServerCapabilities, ToolInfo, 
    ToolResponseContent, ToolsCapability, INTERNAL_ERROR, INVALID_PARAMS, LATEST_PROTOCOL_VERSION, 
    PARSE_ERROR, SUPPORTED_PROTOCOL_VERSIONS,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{stdout, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tokio::{io, task};
use tokio_stream::wrappers::LinesStream;
use tracing::{debug, error, info, warn, Level};
use tracing_appender;
use tracing_subscriber::{self, EnvFilter};

#[tokio::main]
async fn main() {
    // Set up file appender
    let log_dir = std::env::var("LOG_DIR")
        .unwrap_or_else(|_| format!("{}/Developer/mcp/logs", dirs::home_dir().unwrap().display()));
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::NEVER)
        .filename_prefix("mcp-server")
        .filename_suffix("log")
        .build(log_dir)
        .expect("Failed to create log directory");

    // Initialize the tracing subscriber with both stdout and file output
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(Level::DEBUG.into())
                .add_directive("mcp_tools=debug".parse().unwrap()),
        )
        .with_writer(non_blocking)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .init();

    info!("Starting MCP server...");

    // Create a new manager with a persistence filename
    let my_manager = LongRunningTaskManager::new("tasks.json".to_string());
    // If you like, load any persisted tasks now
    if let Err(err) = my_manager.load_persistent_tasks().await {
        error!("Failed to load tasks: {}", err);
    }

    // Create tool implementations
    let mut tool_impls = match create_tools().await {
        Ok(tools) => tools,
        Err(e) => {
            error!("Failed to create tools: {}", e);
            Vec::new()
        }
    };
    
    // Add LongRunningTaskTool which needs the manager
    let manager_arc = Arc::new(Mutex::new(my_manager.clone()));
    tool_impls.push(Box::new(LongRunningTaskTool::new(manager_arc)));
    
    // Extract tool info for registration
    let tool_infos = tool_impls.iter().map(|t| t.info()).collect();

    let state = Arc::new(Mutex::new(MCPServerState {
        resources: vec![], // No sample resources
        tools: tool_infos,
        tool_impls,
        client_capabilities: None,
        client_info: None,
        long_running_manager: my_manager,
    }));

    let (tx_out, mut rx_out) = mpsc::unbounded_channel::<JsonRpcResponse>();

    let printer_handle = tokio::spawn(async move {
        let mut out = stdout();
        while let Some(resp) = rx_out.recv().await {
            let serialized = serde_json::to_string(&resp).unwrap();
            debug!("Sending response: {}", serialized);
            let _ = out.write_all(serialized.as_bytes()).await;
            let _ = out.write_all(b"\n").await;
            let _ = out.flush().await;
        }
    });

    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let lines = reader.lines();
    let mut lines = LinesStream::new(lines);

    while let Some(Ok(line)) = lines.next().await {
        if line.trim().is_empty() {
            continue;
        }

        debug!("Received input: {}", line);
        let parsed: Result<JsonRpcRequest, _> = serde_json::from_str(&line);
        let req = match parsed {
            Ok(req) => {
                debug!("Parsed request: {:?}", req);
                req
            }
            Err(e) => {
                error!("Failed to parse request: {}", e);
                // Try parsing as raw JSON first
                if let Ok(raw_json) = serde_json::from_str::<Value>(&line) {
                    // Check if this looks like an attempted tool call
                    if let Some(intended_tool) = detect_intended_tool_call(&raw_json) {
                        let error_msg = format!(
                            "It looks like you were trying to use the '{}' tool, but the request wasn't properly formatted.\n\
                            Tool calls must use this format:\n\
                            {{\n  \
                              \"jsonrpc\": \"2.0\",\n  \
                              \"id\": 1,\n  \
                              \"method\": \"tools/call\",\n  \
                              \"params\": {{\n    \
                                \"name\": \"{}\",\n    \
                                \"arguments\": {}\n  \
                              }}\n\
                            }}",
                            intended_tool,
                            intended_tool,
                            raw_json
                        );
                        let resp = error_response(
                            Some(Value::Number((1).into())),
                            PARSE_ERROR,
                            &error_msg,
                        );
                        let _ = tx_out.send(resp);
                        continue;
                    }
                }
                let resp =
                    error_response(Some(Value::Number((1).into())), PARSE_ERROR, "Parse error");
                let _ = tx_out.send(resp);
                continue;
            }
        };

        let state = Arc::clone(&state);
        let tx_out_clone = tx_out.clone();

        task::spawn(async move {
            debug!("Handling request: {:?}", req);
            let resp = handle_request(req, &state, tx_out_clone.clone()).await;
            if let Some(resp) = resp {
                debug!("Got response: {:?}", resp);
                let _ = tx_out_clone.send(resp);
            } else {
                warn!("No response generated for request");
            }
        });
    }

    drop(tx_out);
    let _ = printer_handle.await;
}

#[derive(Debug)]
struct MCPServerState {
    resources: Vec<ResourceInfo>,
    tools: Vec<ToolInfo>,
    tool_impls: Vec<Box<dyn Tool>>,
    client_capabilities: Option<ClientCapabilities>,
    client_info: Option<Implementation>,
    long_running_manager: LongRunningTaskManager,
}

// Helper function to create standardized error responses
fn create_error_response(id: Option<Value>, code: i64, message: &str) -> JsonRpcResponse {
    error_response(id, code, message)
}

// Helper function to create standardized success responses
fn create_success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    success_response(id, result)
}

// This function is not needed as we're using the Tool trait implementations directly
// Removing it to avoid type mismatches

// Removing this type alias as we're using the Tool trait implementations directly

async fn handle_request(
    req: JsonRpcRequest,
    state: &Arc<Mutex<MCPServerState>>,
    tx_out: mpsc::UnboundedSender<JsonRpcResponse>,
) -> Option<JsonRpcResponse> {
    // Keep the id (even if null) to ensure proper JSON-RPC format
    let id = Some(req.id.clone());

    match req.method.as_str() {
        "prompts/list" => {
            // Return an empty list of prompts
            let result = json!({
                "prompts": []
            });
            return Some(success_response(id, result));
        }

        "prompts/get" => {
            // Always return "prompt not found" error
            return Some(error_response(id, -32601, "Prompt not found"));
        }

        "initialize" => {
            let params = match req.params {
                Some(p) => p,
                None => {
                    return Some(error_response(
                        Some(id.unwrap_or(Value::Number((1).into()))),
                        INVALID_PARAMS,
                        "Missing params",
                    ));
                }
            };

            let protocol_version = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or(LATEST_PROTOCOL_VERSION);

            if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version) {
                return Some(error_response(
                    Some(id.unwrap_or(Value::Number((1).into()))),
                    INVALID_PARAMS,
                    "Unsupported protocol version",
                ));
            }

            // Store client info and capabilities
            if let Some(client_info) = params.get("clientInfo") {
                if let Ok(info) = serde_json::from_value(client_info.clone()) {
                    let mut guard = state.lock().await;
                    guard.client_info = Some(info);
                }
            }

            if let Some(capabilities) = params.get("capabilities") {
                if let Ok(caps) = serde_json::from_value(capabilities.clone()) {
                    let mut guard = state.lock().await;
                    guard.client_capabilities = Some(caps);
                }
            }

            let result = InitializeResult {
                protocol_version: protocol_version.to_string(),
                capabilities: ServerCapabilities {
                    experimental: Some(HashMap::new()),
                    logging: Some(json!({})),
                    prompts: Some(PromptsCapability {
                        list_changed: true, // We support prompts and prompt updates
                    }),
                    resources: Some(ResourcesCapability {
                        subscribe: false,
                        list_changed: true,
                    }),
                    tools: Some(ToolsCapability { list_changed: true }),
                },
                server_info: Implementation {
                    name: "rust-mcp-server".into(),
                    version: "1.0.0".into(),
                },
                _meta: None,
            };

            Some(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: id.unwrap_or(Value::Null),
                result: Some(json!({
                    "protocolVersion": result.protocol_version,
                    "serverInfo": result.server_info,
                    "capabilities": {
                        "experimental": result.capabilities.experimental,
                        "logging": result.capabilities.logging,
                        "prompts": result.capabilities.prompts,
                        "resources": result.capabilities.resources,
                        "tools": result.capabilities.tools
                    }
                })),
                error: None,
            })
        }

        "resources/list" => {
            let guard = state.lock().await;
            let result = ListResourcesResult {
                resources: guard.resources.clone(),
                _meta: None,
            };
            Some(success_response(id, json!(result)))
        }

        "resources/read" => {
            let params_res: Result<ReadResourceParams, _> =
                serde_json::from_value(req.params.unwrap_or(Value::Null));
            let params = match params_res {
                Ok(p) => p,
                Err(e) => {
                    return Some(error_response(
                        id,
                        INVALID_PARAMS,
                        &format!("Invalid params: {}", e),
                    ));
                }
            };

            let guard = state.lock().await;
            let res = guard.resources.iter().find(|r| r.uri == params.uri);
            match res {
                Some(r) => {
                    let content = ResourceContent {
                        uri: r.uri.clone(),
                        mime_type: r.mime_type.clone(),
                        text: Some("Example file contents.\n".into()),
                        blob: None,
                    };
                    let result = ReadResourceResult {
                        contents: vec![content],
                        _meta: None,
                    };
                    Some(success_response(id, json!(result)))
                }
                None => Some(error_response(id, -32601, "Resource not found")),
            }
        }

        "tools/list" => {
            let guard = state.lock().await;
            let result = ListToolsResult {
                tools: guard.tools.clone(),
                _meta: None,
            };
            Some(success_response(id, json!(result)))
        }

        "tools/call" => {
            let params_res: Result<CallToolParams, _> =
                serde_json::from_value(req.params.clone().unwrap_or(Value::Null));
            let params = match params_res {
                Ok(p) => p,
                Err(e) => {
                    return Some(standard_error_response(
                        id,
                        INVALID_PARAMS,
                        &format!("Invalid params: {}", e),
                    ));
                }
            };

            // Find the tool implementation by name and execute it directly
            let result = {
                let guard = state.lock().await;
                if let Some(tool) = guard.tool_impls.iter().find(|t| t.name() == params.name) {
                    // Execute the tool while holding the lock
                    debug!("Executing tool: {}", tool.name());
                    Some(tool.execute(params.clone(), id.clone()))
                } else {
                    None
                }
            };
            
            match result {
                Some(future) => {
                    // Await the future outside the lock
                    match future.await {
                        Ok(response) => Some(response),
                        Err(e) => {
                            error!("Tool execution error: {}", e);
                            Some(standard_error_response(
                                id,
                                INTERNAL_ERROR,
                                &format!("Tool execution failed: {}", e)
                            ))
                        }
                    }
                }
                None => {
                    warn!("Tool not found: {}", params.name);
                    Some(standard_error_response(
                        id,
                        -32601, // Method not found
                        &format!("Tool not found: {}", params.name)
                    ))
                }
            }

        }

        _ => Some(error_response(id, -32601, "Method not found")), // -32601 is standard code for method not found
    }
}

// We're removing all the individual tool handler functions since we're using the Tool trait implementations directly

fn detect_intended_tool_call(json: &Value) -> Option<String> {
    // Common tool parameter names and their likely tools
    let tool_hints = [
        (vec!["file_path", "pattern", "replacement"], "regex_replace"),
        (vec!["command"], "bash"),
        (vec!["url"], "scrape_url"),
        (vec!["query"], "brave_search"),
        (vec!["action", "repo_path", "files", "message"], "git"),
        (
            vec![
                "command",
                "name",
                "description",
                "content",
                "parent",
                "relation",
                "tags",
            ],
            "graph_tool",
        ),
        (vec!["action", "params"], "sequential_thinking"),
        (vec!["action", "params"], "memory"),
        (vec!["action", "params"], "task_planning"),
    ];

    // If it's an object, check its fields
    if let Some(obj) = json.as_object() {
        for (params, tool_name) in tool_hints.iter() {
            // Count how many of the hint parameters are present
            let matches = params
                .iter()
                .filter(|&param| obj.contains_key(*param))
                .count();

            // If we find more than half of the expected parameters, this is probably the intended tool
            if matches >= (params.len() + 1) / 2 {
                return Some((*tool_name).to_string());
            }
        }
    }
    None
}
