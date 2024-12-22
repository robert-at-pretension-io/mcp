use futures::StreamExt;
use serde_json::{json, Value};
use shared_protocol_objects::{
    error_response, success_response, CallToolParams, CallToolResult, ClientCapabilities,
    Implementation, InitializeResult, JsonRpcRequest, JsonRpcResponse, ListResourcesResult,
    ListToolsResult, PromptsCapability, ReadResourceParams, ReadResourceResult, ResourceContent,
    ResourceInfo, ResourcesCapability, ServerCapabilities, ToolInfo, ToolResponseContent,
    ToolsCapability, INTERNAL_ERROR, INVALID_PARAMS, LATEST_PROTOCOL_VERSION, PARSE_ERROR,
    SUPPORTED_PROTOCOL_VERSIONS,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{stdout, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tokio::{io, task};
use tokio_stream::wrappers::LinesStream;
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::{self, EnvFilter};
use tracing_appender;
use mcp_tools::bash::{bash_tool_info, BashExecutor, BashParams};
use mcp_tools::brave_search::{search_tool_info, BraveSearchClient};
use mcp_tools::git_integration::{git_tool_info, handle_git_tool_call};
use mcp_tools::{graph_database, memory, sequential_thinking, task_planning};
use mcp_tools::graph_database::{graph_tool_info, handle_graph_tool_call, GraphManager};
use mcp_tools::process_html::extract_text_from_html;
use mcp_tools::regex_replace::{handle_regex_replace_tool_call, regex_replace_tool_info};
use mcp_tools::scraping_bee::{scraping_tool_info, ScrapingBeeClient, ScrapingBeeResponse};
use mcp_tools::oracle_tool::{oracle_select_tool_info, handle_oracle_select_tool_call};

#[tokio::main]
async fn main() {
    // Set up file appender
    let log_dir = std::env::var("LOG_DIR").unwrap_or_else(|_| {
        format!("{}/Developer/mcp/logs", dirs::home_dir().unwrap().display())
    });
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
                .add_directive("mcp_tools=debug".parse().unwrap())
        )
        .with_writer(non_blocking)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .init();

    info!("Starting MCP server...");

    // Verify required environment variables are present
    let _scrapingbee_key = std::env::var("SCRAPINGBEE_API_KEY")
        .expect("SCRAPINGBEE_API_KEY environment variable must be set");
    let _brave_key = std::env::var("BRAVE_API_KEY")
        .expect("BRAVE_API_KEY environment variable must be set");

    debug!("Environment variables loaded successfully");

    let _ = std::env::var("KNOWLEDGE_GRAPH_DIR").unwrap_or_else(|_| {
        println!(
            "KNOWLEDGE_GRAPH_DIR not set, using default: {}",
            graph_database::DEFAULT_GRAPH_DIR
        );
        graph_database::DEFAULT_GRAPH_DIR.to_string()
    });

    let state = Arc::new(Mutex::new(MCPServerState {
        resources: vec![ResourceInfo {
            uri: "file:///example.txt".into(),
            name: "Example Text File".into(),
            mime_type: Some("text/plain".into()),
            description: Some("An example text resource".into()),
        }],
        tools: vec![
            git_tool_info(),
            bash_tool_info(),
            scraping_tool_info(),
            search_tool_info(),
            graph_tool_info(),
            regex_replace_tool_info(),
            oracle_select_tool_info(),
            // sequential_thinking::sequential_thinking_tool_info(),
            // memory::memory_tool_info(),
            //task_planning::task_planning_tool_info(),
        ],
        client_capabilities: None,
        client_info: None,
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
                        let resp = error_response(None, PARSE_ERROR, &error_msg);
                        let _ = tx_out.send(resp);
                        continue;
                    }
                }
                let resp = error_response(None, PARSE_ERROR, "Parse error");
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
    client_capabilities: Option<ClientCapabilities>,
    client_info: Option<Implementation>,
}

async fn handle_request(
    req: JsonRpcRequest,
    state: &Arc<Mutex<MCPServerState>>,
    tx_out: mpsc::UnboundedSender<JsonRpcResponse>,
) -> Option<JsonRpcResponse> {
    let id = if req.id.is_null() {
        None
    } else {
        Some(req.id.clone())
    };

    match req.method.as_str() {
        "initialize" => {
            let params = match req.params {
                Some(p) => p,
                None => return Some(error_response(id, -32602, "Missing params")),
            };

            let protocol_version = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or(LATEST_PROTOCOL_VERSION);

            if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version) {
                return Some(error_response(id, -32602, "Unsupported protocol version"));
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
                        list_changed: false,
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
                id,
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
                        -32602,
                        &format!("Invalid params: {}", e),
                    ))
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
                    return Some(error_response(
                        id,
                        -32602,
                        &format!("Invalid params: {}", e),
                    ))
                }
            };

            let tool = {
                let guard = state.lock().await;
                guard.tools.iter().find(|t| t.name == params.name).cloned()
            };

            match tool {
                Some(t) => {
                    if t.name == "scrape_url" {
                        // Handle scrape_url tool
                        let url = params
                            .arguments
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if url.is_empty() {
                            return Some(error_response(
                                id,
                                -32602,
                                "Missing required argument: url",
                            ));
                        }

                        let scrapingbee_api_key = std::env::var("SCRAPINGBEE_API_KEY")
                            .expect("SCRAPINGBEE_API_KEY environment variable must be set");
                        let mut client = ScrapingBeeClient::new(scrapingbee_api_key.clone());
                        client.url(&url).render_js(true);

                        let result = client.execute().await;

                        match result {
                            Ok(ScrapingBeeResponse::Text(body)) => {
                                // Send progress notification if requested
                                let meta = if let Some(params) = &req.params {
                                    params.get("_meta").cloned()
                                } else {
                                    None
                                };
                                if let Some(meta) = meta {
                                    if let Some(token) = meta.get("progressToken") {
                                        let progress_notification = JsonRpcResponse {
                                            jsonrpc: "2.0".into(),
                                            id: None,
                                            result: Some(json!({
                                                "method": "notifications/progress",
                                                "params": {
                                                    "progressToken": token,
                                                    "progress": 50,
                                                    "total": 100
                                                }
                                            })),
                                            error: None,
                                        };
                                        let _ = tx_out.send(progress_notification);
                                    }
                                }

                                let tool_res = CallToolResult {
                                    content: vec![ToolResponseContent {
                                        type_: "text".into(),
                                        text: extract_text_from_html(&body, Some(&url)),
                                        annotations: None,
                                    }],
                                    is_error: None,
                                    _meta: None,
                                    progress: None,
                                    total: None,
                                };
                                Some(success_response(id, json!(tool_res)))
                            }
                            Ok(ScrapingBeeResponse::Binary(bytes)) => {
                                // Save screenshot
                                return Some(error_response(
                                    id,
                                    -32603,
                                    &format!("Can't read binary scrapes"),
                                ));
                            }
                            Err(e) => {
                                let tool_res = CallToolResult {
                                    content: vec![ToolResponseContent {
                                        type_: "text".into(),
                                        text: format!("Error: {}", e),
                                        annotations: None,
                                    }],
                                    is_error: Some(true),
                                    _meta: None,
                                    progress: None,
                                    total: None,
                                };
                                Some(success_response(id, json!(tool_res)))
                            }
                        }
                    } else if t.name == "bash" {
                        // Parse bash params
                        let bash_params: BashParams = match serde_json::from_value(params.arguments)
                        {
                            Ok(p) => p,
                            Err(e) => {
                                return Some(error_response(id, INVALID_PARAMS, &e.to_string()))
                            }
                        };

                        let executor = BashExecutor::new();

                        // Execute command
                        match executor.execute(bash_params).await {
                            Ok(result) => {
                                let tool_res = CallToolResult {
                                    content: vec![ToolResponseContent {
                                        type_: "text".into(),
                                        text: format!(
                                            "Command completed with status {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                                            result.status,
                                            result.stdout,
                                            result.stderr
                                        ),
                                        annotations: None,
                                    }],
                                    is_error: Some(!result.success),
                                    _meta: None,
                                    progress: None,
                                    total: None,
                                };
                                Some(success_response(id, json!(tool_res)))
                            }
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else if t.name == "git" {
                        match handle_git_tool_call(params, id.clone()).await {
                            Ok(resp) => Some(resp),
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else if t.name == "brave_search" {
                        let query = match params.arguments.get("query").and_then(Value::as_str) {
                            Some(q) => q.to_string(),
                            None => {
                                return Some(error_response(
                                    id,
                                    -32602,
                                    "Missing required argument: query",
                                ))
                            }
                        };

                        let count = params
                            .arguments
                            .get("count")
                            .and_then(Value::as_u64)
                            .unwrap_or(10)
                            .min(20) as u8;

                        let brave_search_api_key = std::env::var("BRAVE_API_KEY")
                            .expect("BRAVE_API_KEY environment variable must be set");
                        let client = BraveSearchClient::new(brave_search_api_key);
                        match client.search(&query).await {
                            Ok(response) => {
                                let results = match response.web {
                                    Some(web) => web
                                        .results
                                        .iter()
                                        .take(count as usize)
                                        .map(|result| {
                                            format!(
                                                "Title: {}\nURL: {}\nDescription: {}\n\n",
                                                result.title,
                                                result.url,
                                                result
                                                    .description
                                                    .as_deref()
                                                    .unwrap_or("No description available")
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join("---\n"),
                                    None => "No web results found".to_string(),
                                };

                                let tool_res = CallToolResult {
                                    content: vec![ToolResponseContent {
                                        type_: "text".into(),
                                        text: results,
                                        annotations: None,
                                    }],
                                    is_error: None,
                                    _meta: None,
                                    progress: None,
                                    total: None,
                                };
                                Some(success_response(id, json!(tool_res)))
                            }
                            Err(e) => {
                                let tool_res = CallToolResult {
                                    content: vec![ToolResponseContent {
                                        type_: "text".into(),
                                        text: format!("Search error: {}", e),
                                        annotations: None,
                                    }],
                                    is_error: Some(true),
                                    _meta: None,
                                    progress: None,
                                    total: None,
                                };
                                Some(success_response(id, json!(tool_res)))
                            }
                        }
                    } else if t.name == "graph_tool" {
                        // Initialize with just the filename - path will be determined from env var
                        let mut graph_manager =
                            GraphManager::new("knowledge_graph.json".to_string());
                        match handle_graph_tool_call(params, &mut graph_manager, id.clone()).await {
                            Ok(resp) => Some(resp),
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else if t.name == "regex_replace" {
                        match handle_regex_replace_tool_call(params, id.clone()).await {
                            Ok(resp) => Some(resp),
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else if t.name == "sequential_thinking" {
                        match sequential_thinking::handle_sequential_thinking_tool_call(
                            params,
                            id.clone(),
                        )
                        .await
                        {
                            Ok(resp) => Some(resp),
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else if t.name == "memory" {
                        match memory::handle_memory_tool_call(params, id.clone()).await {
                            Ok(resp) => Some(resp),
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else if t.name == "task_planning" {
                        match task_planning::handle_task_planning_tool_call(params, id.clone())
                            .await
                        {
                            Ok(resp) => Some(resp),
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else if t.name == "oracle_select" {
                        match handle_oracle_select_tool_call(params, id.clone()).await {
                            Ok(resp) => Some(resp),
                            Err(e) => Some(error_response(id, INTERNAL_ERROR, &e.to_string())),
                        }
                    } else {
                        Some(error_response(id, -32601, "Tool not implemented"))
                    }
                }
                None => Some(error_response(id, -32601, "Tool not found")),
            }
        }

        _ => Some(error_response(id, -32601, "Method not found")),
    }
}

fn detect_intended_tool_call(json: &Value) -> Option<String> {
    // Common tool parameter names and their likely tools
    let tool_hints = [
        (vec!["file_path", "pattern", "replacement"], "regex_replace"),
        (vec!["command"], "bash"),
        (vec!["url"], "scrape_url"),
        (vec!["query"], "brave_search"),
        (vec!["action", "repo_path", "files", "message"], "git"),
        (
            vec!["action", "params", "name", "description", "content"],
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
