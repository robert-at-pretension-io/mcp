use axum::{
    extract::Query,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use shared_protocol_objects::{
    CallToolParams, ListToolsResult, ToolInfo,
    success_response, error_response, JsonRpcResponse,
    INTERNAL_ERROR, INVALID_PARAMS,
};

use mcp_tools::graph_database::{graph_tool_info, handle_graph_tool_call, GraphManager};
use mcp_tools::brave_search::{search_tool_info, BraveSearchClient};
use mcp_tools::scraping_bee::{scraping_tool_info, ScrapingBeeClient};

// Tool trait defining the interface for all tools
#[async_trait]
pub trait Tool: Send + Sync {
    fn info(&self) -> ToolInfo;
    async fn execute(&self, params: CallToolParams) -> Result<JsonRpcResponse>;
}

// Registry to manage all available tools
#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(HashMap::new()),
        }
    }

    pub fn with_tools(tools: Vec<Arc<dyn Tool>>) -> Self {
        let mut tool_map = HashMap::new();
        for tool in tools {
            tool_map.insert(tool.info().name.clone(), tool);
        }
        Self {
            tools: Arc::new(tool_map),
        }
    }

    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools.values()
            .map(|tool| tool.info())
            .collect()
    }
}

// Graph Tool Implementation
pub struct GraphTool {
    graph_manager: Arc<Mutex<GraphManager>>,
}

impl GraphTool {
    pub fn new(graph_manager: Arc<Mutex<GraphManager>>) -> Self {
        Self { graph_manager }
    }
}

pub const MAX_DESCRIPTION_LENGTH: usize = 1024;

fn validate_tool_description(name: &str, description: &Option<String>) {
    if let Some(desc) = description {
        if desc.len() > MAX_DESCRIPTION_LENGTH {
            panic!(
                "Tool '{}' description exceeds maximum length of {} characters (current length: {})",
                name, MAX_DESCRIPTION_LENGTH, desc.len()
            );
        }
    }
}




#[async_trait]
impl Tool for GraphTool {
    fn info(&self) -> ToolInfo {
        let info = graph_tool_info();
        validate_tool_description(&info.name, &info.description);
        info
    }

    async fn execute(&self, params: CallToolParams) -> Result<JsonRpcResponse> {
        let mut graph_manager = self.graph_manager.lock().await;
        handle_graph_tool_call(params, &mut graph_manager, None).await
    }
}

// Brave Search Tool Implementation
pub struct BraveSearchTool {
    client: Arc<BraveSearchClient>,
}

impl BraveSearchTool {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Arc::new(BraveSearchClient::new(api_key)),
        }
    }
}

#[async_trait]
impl Tool for BraveSearchTool {
    fn info(&self) -> ToolInfo {
        let info = search_tool_info();
        validate_tool_description(&info.name, &info.description);
        info
    }

    async fn execute(&self, params: CallToolParams) -> Result<JsonRpcResponse> {
        let query = params.arguments.get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'query' parameter"))?;

        let search_response = self.client.search(query).await
            .map_err(|e| anyhow::anyhow!("Search request failed: {}", e))?;
        
        let formatted_results = if let Some(web) = search_response.web {
            if web.results.is_empty() {
                "No results found for this query.".to_string()
            } else {
                web.results.iter()
                    .map(|result| {
                        format!(
                            "Title: {}\nURL: {}\nDescription: {}\nLanguage: {}\nAge: {}\n",
                            result.title,
                            result.url,
                            result.description.as_deref().unwrap_or("No description available"),
                            result.language.as_deref().unwrap_or("unknown"),
                            result.page_age.as_deref().unwrap_or("unknown")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n---\n")
            }
        } else {
            "No web results found".to_string()
        };

        let tool_res = CallToolResult {
            content: vec![ToolResponseContent {
                type_: "text".into(),
                text: formatted_results,
                annotations: None,
            }],
            is_error: Some(false),
            _meta: None,
            progress: None,
            total: None,
        };

        Ok(success_response(None, serde_json::to_value(tool_res)?))
    }
}

// ScrapingBee Tool Implementation
pub struct ScrapingBeeTool {
    client: Arc<ScrapingBeeClient>,
}

impl ScrapingBeeTool {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Arc::new(ScrapingBeeClient::new(api_key)),
        }
    }
}

#[async_trait]
impl Tool for ScrapingBeeTool {
    fn info(&self) -> ToolInfo {
        let info = scraping_tool_info();
        validate_tool_description(&info.name, &info.description);
        info
    }

    async fn execute(&self, params: CallToolParams) -> Result<JsonRpcResponse> {
        let url = params.arguments.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'url' parameter"))?;

        let scraping_response = self.client.clone()
            .url(url)
            .render_js(true)
            .execute()
            .await
            .map_err(|e| anyhow::anyhow!("Scraping request failed: {}", e))?;

        let content = match scraping_response {
            ScrapingBeeResponse::Text(text) => {
                if text.trim().is_empty() {
                    return Err(anyhow::anyhow!("Received empty response from URL"));
                }
                // Truncate very long responses
                if text.len() > 50000 {
                    format!("{}... (truncated {} characters)", &text[..50000], text.len() - 50000)
                } else {
                    text
                }
            },
            ScrapingBeeResponse::Binary(_) => return Err(anyhow::anyhow!("Received binary response, expected text")),
        };

        let tool_res = CallToolResult {
            content: vec![ToolResponseContent {
                type_: "text".into(),
                text: content,
                annotations: None,
            }],
            is_error: Some(false),
            _meta: None,
            progress: None,
            total: None,
        };

        Ok(success_response(None, serde_json::to_value(tool_res)?))
    }
}

// Application State
#[derive(Clone)]
pub struct AppState {
    tool_registry: ToolRegistry,
}

// Request/Response structures
#[derive(Serialize,Deserialize, Debug)]
pub struct ToolCallRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<CallToolParams>,
}

#[derive(Deserialize)]
struct SessionQuery {
    model: Option<String>,
}

// Handler functions
async fn handle_tools_call(
    Json(payload): Json<ToolCallRequest>,
    state: Arc<AppState>,
) -> impl IntoResponse {
    debug!("Incoming tool call: {}", serde_json::to_string_pretty(&payload).unwrap_or_default());

    let response = match payload.method.as_str() {
        "tools/call" => {
            if let Some(params) = payload.params {
                if let Some(tool) = state.tool_registry.get_tool(&params.name) {
                    match tool.execute(params).await {
                        Ok(resp) => resp,
                        Err(e) => error_response(payload.id, INTERNAL_ERROR, &e.to_string()),
                    }
                } else {
                    error_response(payload.id, -32601, "Tool not found")
                }
            } else {
                error_response(payload.id, INVALID_PARAMS, "Missing params")
            }
        },
        "tools/list" => {
            let result = ListToolsResult {
                tools: state.tool_registry.list_tools(),
                _meta: None,
            };
            success_response(payload.id, json!(result))
        },
        _ => error_response(payload.id, -32601, "Method not found"),
    };

    Json(response)
}

async fn get_ephemeral_token(
    Query(q): Query<SessionQuery>,
    state: Arc<AppState>,
) -> impl IntoResponse {
    let model = q.model.unwrap_or("gpt-4o-realtime-preview-2024-12-17".to_string());
    let openai_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-REAL_KEY".into());

    let result = match reqwest::Client::new()
        .post("https://api.openai.com/v1/realtime/sessions")
        .header("Authorization", format!("Bearer {openai_key}"))
        .json(&json!({"model": model, "voice": "verse"}))
        .send()
        .await
    {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(val) => val,
            Err(e) => json!({"error": format!("Invalid response: {e}")}),
        },
        Err(e) => json!({"error": format!("Request failure: {e}")}),
    };

    Json(result)
}

// Handler for client-side logs
async fn handle_log(Json(payload): Json<Value>) -> impl IntoResponse {
    if let Some(msg) = payload.get("message") {
        info!("Client log: {}", msg);
    }
    Json(json!({"status": "ok"}))
}

async fn index_page() -> Html<&'static str> {
    Html(INDEX_HTML)
}

// Initialize tools and create app state
fn initialize_tools() -> Result<ToolRegistry> {
    // Load configuration and create tools
    let home_dir = dirs::home_dir()
        .expect("Could not find home directory");
    let graph_path = home_dir.join("knowledge_graph.json");
    
    debug!("Using knowledge graph at: {}", graph_path.display());
    
    let graph_manager = Arc::new(Mutex::new(
        GraphManager::new(graph_path.to_str()
            .expect("Invalid path to knowledge graph")
            .to_string())
    ));
    
    let brave_api_key = std::env::var("BRAVE_API_KEY")?;
    let scrapingbee_api_key = std::env::var("SCRAPINGBEE_API_KEY")?;
    
    // Create tool instances
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(GraphTool::new(graph_manager)),
        Arc::new(BraveSearchTool::new(brave_api_key)),
        Arc::new(ScrapingBeeTool::new(scrapingbee_api_key)),
        // Add more tools here
    ];

    Ok(ToolRegistry::with_tools(tools))
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8" />
    <title>Realtime Voice + Tools Demo</title>
</head>
<body>
    <h1>Realtime Voice + Tools Demo</h1>
    <div id="tools-info">
        <h2>Available Tools</h2>
        <pre id="tools-list">Loading tools...</pre>
    </div>
    <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
    <style>
        .filter-container {
            margin: 20px 0;
            padding: 10px;
            background: #f8f9fa;
            border: 1px solid #dee2e6;
            border-radius: 4px;
        }
        .filter-input {
            width: 100%;
            padding: 8px;
            border: 1px solid #ced4da;
            border-radius: 4px;
            margin-top: 5px;
        }
        .response-container {
            margin: 10px 0;
        }
        .response-type {
            color: #0366d6;
            font-weight: bold;
            margin-bottom: 5px;
        }
        /* Style for markdown content */
        .markdown-content {
            background: #f8f9fa;
            border: 1px solid #dee2e6;
            border-radius: 4px;
            padding: 15px;
        }
        /* Override markdown code block styling */
        .markdown-content pre {
            background: #f1f1f1;
            padding: 10px;
            border-radius: 3px;
        }
    </style>
    <div id="function-calls">
        <h2>Function Call History</h2>
        <pre id="call-history"></pre>
    </div>
    <div id="responses">
        <h2>Assistant Responses</h2>
        <div class="filter-container">
            <label for="filter-text">Filter out responses containing text (comma-separated, default filters applied):</label>
            <input type="text" id="filter-text" class="filter-input" value="delta,updated,created,speech,part" placeholder="Enter terms to filter out, separated by commas">
        </div>
        <div id="response-history"></div>
    </div>
    <button id="btn-start">Start RTC</button>
    <script>
    // Override console.log to send prettier logs to server
    const originalLog = console.log;
    console.log = function(...args) {
        // Call original console.log
        originalLog.apply(console, args);
        
        // Format message for server
        let message;
        if (args.length === 1 && typeof args[0] === 'object') {
            // For single objects, pretty print
            message = JSON.stringify(args[0], null, 2);
        } else if (args.length === 2 && args[0] && typeof args[0] === 'string' && typeof args[1] === 'object') {
            // For label + object pattern, format nicely
            message = `${args[0]}\n${JSON.stringify(args[1], null, 2)}`;
        } else {
            // For other cases, join with spaces
            message = args.map(arg => 
                typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
            ).join(' ');
        }
        
        // Send to server
        fetch('/log', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ message })
        }).catch(err => originalLog('Error sending log:', err));
    };

    const toolsList = document.getElementById('tools-list');
    const callHistory = document.getElementById('call-history');
    const responseHistory = document.getElementById('response-history');
    const btn = document.getElementById('btn-start');

    function addResponse(data) {
        const filterInput = document.getElementById('filter-text');
        const filterTerms = filterInput.value.split(',').map(term => term.trim().toLowerCase()).filter(Boolean);
        
        // Convert data to string for filtering
        const dataString = JSON.stringify(data).toLowerCase();
        
        // Check if any filter term is present in the data
        const shouldFilter = filterTerms.some(term => dataString.includes(term));
        
        if (!shouldFilter) {
            const container = document.createElement('div');
            container.className = 'response-container';
            
            const type = document.createElement('div');
            type.className = 'response-type';
            type.textContent = data.type;
            
            const content = document.createElement('div');
            content.className = 'markdown-content';
            
            // Convert the JSON to a markdown code block
            const markdownContent = '```json\n' + JSON.stringify(data, null, 2) + '\n```';
            content.innerHTML = marked.parse(markdownContent);
            
            container.appendChild(type);
            container.appendChild(content);
            responseHistory.insertBefore(container, responseHistory.firstChild);
        }
    }

    // Function to display tools info
    function displayTools(tools) {
        console.log('DisplayTools received:', JSON.stringify(tools, null, 2));
        
        const toolsInfo = tools.map(tool => {
            console.log('Processing display for tool:', tool.name);
            console.log('Tool parameters:', JSON.stringify(tool.parameters, null, 2));
            
            const schema = tool.parameters || {};
            console.log('Schema to process:', JSON.stringify(schema, null, 2));
            
            let paramList = '';
            
            if (schema.properties) {
                console.log('Processing properties schema');
                paramList = Object.entries(schema.properties)
                    .map(([key, value]) => {
                        console.log('Processing property:', key, value);
                        const required = schema.required?.includes(key) ? '*' : '';
                        const desc = value.description ? ` - ${value.description}` : '';
                        return `  ${key}${required}: ${value.type}${desc}`;
                    })
                    .join('\n');
            } else if (schema.oneOf) {
                console.log('Processing oneOf schema');
                paramList = schema.oneOf
                    .map((option, index) => {
                        console.log('Processing option:', index, option);
                        const props = Object.entries(option.properties || {})
                            .map(([key, value]) => {
                                console.log('Processing oneOf property:', key, value);
                                const required = option.required?.includes(key) ? '*' : '';
                                const desc = value.description ? ` - ${value.description}` : '';
                                return `    ${key}${required}: ${value.type}${desc}`;
                            })
                            .join('\n');
                        return `  Option ${index + 1}:\n${props}`;
                    })
                    .join('\n');
            } else {
                console.log('No recognized schema structure found');
            }
            
            const result = `Tool: ${tool.name}\nDescription: ${tool.description}\nParameters:\n${paramList}\n`;
            console.log('Generated display:', result);
            return result;
        }).join('\n---\n');
        
        console.log('Final toolsInfo:', toolsInfo);
        toolsList.textContent = toolsInfo;
    }

    // Function to add call to history
    function addToCallHistory(functionName, params, result) {
        const timestamp = new Date().toISOString();
        const callInfo = `[${timestamp}] Called: ${functionName}\nParams: ${JSON.stringify(params, null, 2)}\nResult: ${JSON.stringify(result, null, 2)}\n---\n`;
        callHistory.textContent = callInfo + callHistory.textContent;
    }

    btn.addEventListener('click', async () => {
        // First fetch available tools
        const toolsResponse = await fetch('/tools/call', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                jsonrpc: "2.0",
                id: 1,
                method: "tools/list"
            })
        });
        const toolsData = await toolsResponse.json();
        console.log('Raw tools response:', JSON.stringify(toolsData, null, 2));
        console.log('Tools before transformation:', JSON.stringify(toolsData.result.tools, null, 2));
        
        const tools = toolsData.result.tools.map(tool => {
            console.log('Processing tool:', tool.name);
            console.log('Tool inputSchema:', JSON.stringify(tool.inputSchema, null, 2));
            
            const transformed = {
                type: "function",
                name: tool.name,
                description: tool.description || '',
                parameters: tool.inputSchema
            };
            
            console.log('Transformed tool:', JSON.stringify(transformed, null, 2));
            return transformed;
        });

        // Display available tools
        displayTools(tools);

        const model = "gpt-4o-realtime-preview-2024-12-17";
        try {
            const sessionRes = await fetch(`/session?model=${model}`);
            const sessionData = await sessionRes.json();
            
            if (!sessionData?.client_secret?.value) {
                console.error("No ephemeral key found in /session response:", sessionData);
                return;
            }
            const ephemeralKey = sessionData.client_secret.value;
            if(!ephemeralKey) {
                console.error("No ephemeral key found in /session response.");
                return;
            }

            const pc = new RTCPeerConnection();
            const audioEl = document.createElement("audio");
            audioEl.autoplay = true;
            document.body.appendChild(audioEl);
            pc.ontrack = e => audioEl.srcObject = e.streams[0];

            const ms = await navigator.mediaDevices.getUserMedia({audio:true});
            pc.addTrack(ms.getTracks()[0]);

            const dc = pc.createDataChannel("oai-events");
            dc.onopen = () => {
                console.log('Data channel open');
                console.log('Sending initial configuration with tools');
                // Initial configuration with tools and system prompt
                const configEvent = {
                    type: "session.update",
                    session: {
                        tools: tools,
                        tool_choice: "auto",
                        modalities: ["text", "audio"],
                        instructions: `You are a proactive assistant with access to tools. Your primary responsibility is to maintain a comprehensive knowledge graph about the user and their context. For every interaction:

1. ALWAYS create graph nodes for:
   - Personal information shared
   - Preferences expressed
   - Problems or challenges mentioned
   - Goals and aspirations
   - Experiences described
   - Skills and knowledge
   - Relationships and connections
   - Decisions and choices made
   - Emotional states or reactions
   - Questions asked and answers given

2. PROACTIVELY:
   - Search for and connect relevant information
   - Link new nodes to existing knowledge
   - Update outdated information
   - Add metadata and tags for better organization
   - Create relationships between related concepts
   - Maintain temporal context with date metadata

3. BEFORE RESPONDING:
   - Search the graph for relevant context
   - Check for contradictions with existing knowledge
   - Look for opportunities to gather more information
   - Consider related topics that could be explored

4. WHILE RESPONDING:
   - Seamlessly incorporate graph knowledge without explicitly mentioning it
   - Use the context to personalize responses
   - Identify gaps in knowledge to explore
   - Look for opportunities to verify or update existing information

Remember: Every interaction is an opportunity to enrich the knowledge graph. Be thorough but natural in gathering and storing information.`
                    }
                };
                dc.send(JSON.stringify(configEvent));

                // Initial response.create
                const responseCreate = {
                    type: "response.create",
                    response: {
                        modalities: ["text", "audio"],
                        instructions: "I'm ready to help you. What would you like to do?"
                    }
                };
                dc.send(JSON.stringify(responseCreate));
            };

            dc.onmessage = async (e) => {
                const data = JSON.parse(e.data);
                
                // Add response to UI
                addResponse(data);
                
                // Log events except filtered ones
                const defaultFilters = ['delta', 'updated', 'created', 'speech', 'part'];
                if (!defaultFilters.some(term => data.type.toLowerCase().includes(term))) {
                    console.log('Event received:', {
                        type: data.type,
                        data: JSON.stringify(data, null, 2)
                    });
                }
                
                switch (data.type) {
                    case "response.done":
                        if (data.response?.output?.[0]?.type === "function_call") {
                            console.log('Function call requested:', {
                                name: data.response.output[0].name,
                                call_id: data.response.output[0].call_id,
                                arguments: JSON.parse(data.response.output[0].arguments)
                            });
                        }
                        break;
                        
                    case "response.function_call_arguments.done":
                        console.log('Function call event received:', JSON.stringify(data, null, 2));
                        console.log('Function arguments:', data.arguments);
                        console.log('Executing function:', {
                            name: data.name,
                            call_id: data.call_id
                        });
                        
                        const toolRequest = {
                            jsonrpc: "2.0", 
                            id: 1,
                            method: "tools/call",
                            params: {
                                name: data.name,
                                arguments: JSON.parse(data.arguments)
                            }
                        };
                    
                        try {
                            const response = await fetch('/tools/call', {
                                method: 'POST',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify(toolRequest)
                            });
                        
                            const result = await response.json();
                            
                            // Log the full result for debugging
                            console.log('Tool call result:', JSON.stringify(result, null, 2));
                            
                            // Add to call history UI
                            addToCallHistory(
                                data.name,
                                JSON.parse(data.arguments),
                                result
                            );

                            let outputText;
                            if (result.error) {
                                outputText = `Error: ${result.error.message}`;
                                console.error('Tool call failed:', outputText);
                            } else {
                                outputText = result.result?.content?.[0]?.text || "No output received";
                                console.log('Tool call succeeded:', {
                                    name: data.name,
                                    call_id: data.call_id,
                                    output: outputText
                                });
                            }
                        
                            // Parse and format the output text if it's JSON
                            let formattedOutput = outputText;
                            try {
                                const outputJson = JSON.parse(outputText);
                                if (Array.isArray(outputJson)) {
                                    formattedOutput = outputJson.map(node => {
                                        return `Node: ${node.name}\nDescription: ${node.description}\nTags: [${node.tags.join(", ")}]\nCreated: ${node.date_created}\nModified: ${node.date_modified}\nContent: ${node.content}`;
                                    }).join("\n---\n");
                                }
                            } catch (e) {
                                // If not valid JSON, use as-is
                                console.log("Output is not JSON, using raw text");
                            }

                            // Send function result back to model
                            dc.send(JSON.stringify({
                                type: "conversation.item.create",
                                item: {
                                    type: "function_call_output",
                                    call_id: data.call_id,
                                    output: formattedOutput
                                }
                            }));

                            // Create a text input with the formatted result for the AI to respond to
                            dc.send(JSON.stringify({
                                type: "conversation.item.create",
                                item: {
                                    type: "message",
                                    role: "user",
                                    content: [{
                                        type: "input_text",
                                        text: formattedOutput
                                    }]
                                }
                            }));

                            // Request a response from the AI with instructions to run more functions if needed
                            dc.send(JSON.stringify({
                                type: "response.create",
                                response: {
                                    modalities: ["text", "audio"],
                                    instructions: `CRITICAL: You MUST follow this process with the function call result:

1. ALWAYS run multiple additional search functions to gather comprehensive context:
   - search_nodes for related content
   - get_recent_nodes to check latest context
   - get_nodes_by_tag for thematic connections
   - get_most_connected to find key relationships

2. For each piece of information discovered:
   - Run more targeted searches to deep-dive into specific aspects
   - Look for connections to other topics
   - Verify consistency with existing knowledge
   - Identify gaps that need exploration

3. Only after exhaustively gathering context through multiple rounds of function calls:
   - Synthesize all discovered information
   - Identify any remaining gaps
   - Make final function calls if needed
   - Provide comprehensive response

NEVER give a final response without first making multiple rounds of function calls to gather extensive context.`
                                }
                            }));
                        } catch(err) {
                            console.error('Function failed:', {
                                name: data.function.name,
                                call_id: data.function.call_id,
                                error: err.message
                            });
                            
                            // Send error response
                            dc.send(JSON.stringify({
                                type: "conversation.item.create",
                                item: {
                                    type: "function_call_output",
                                    call_id: data.function.call_id,
                                    output: `Error: ${err.message}`
                                }
                            }));
                        }
                        break;
                        
                    case "session.update":
                        // Only log tool registration
                        if (data.session?.tools) {
                            console.log('Tools registered:', data.session.tools.map(t => t.name));
                        }
                        break;
                }
            };

            const offer = await pc.createOffer();
            await pc.setLocalDescription(offer);
            const baseUrl = "https://api.openai.com/v1/realtime";
            const sdpResponse = await fetch(`${baseUrl}?model=${model}`, {
                method: "POST",
                body: offer.sdp,
                headers: {
                    "Authorization": `Bearer ${ephemeralKey}`,
                    "Content-Type": "application/sdp"
                }
            });
            if(!sdpResponse.ok) {
                console.error("SDP request failed:", await sdpResponse.text());
                return;
            }
            const answerSdp = await sdpResponse.text();
            await pc.setRemoteDescription({ type:"answer", sdp: answerSdp });
            console.log("WebRTC connected successfully.");
        } catch(err) {
            console.error("Error starting session:", err);
        }
    });
    </script>
</body>
</html>"#;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize tools and create app state
    let tool_registry = initialize_tools()?;
    let state = Arc::new(AppState { tool_registry });

    // Create router with all routes
    let app = Router::new()
        .route("/", get(index_page))
        .route("/session", get({
            let st = state.clone();
            move |q| get_ephemeral_token(q, st)
        }))
        .route("/tools/call", post({
            let st = state.clone();
            move |body| handle_tools_call(body, st)
        }))
        .route("/log", post(handle_log));

    // Start server
    let addr = "0.0.0.0:3000";
    info!("Server running on {}", addr);
    let addr: SocketAddr = addr.parse()?;
    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app.into_make_service(),
    )
    .await?;
    Ok(())
}
