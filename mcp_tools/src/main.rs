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
use tracing::{debug, error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use shared_protocol_objects::{
    CallToolParams, JsonRpcError, ListToolsResult, ToolInfo,
    success_response, error_response, JsonRpcResponse,
    INTERNAL_ERROR, INVALID_PARAMS,
};

use mcp_tools::graph_database::{graph_tool_info, handle_graph_tool_call, GraphManager, DEFAULT_GRAPH_DIR};
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

#[async_trait]
impl Tool for GraphTool {
    fn info(&self) -> ToolInfo {
        graph_tool_info()
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
        search_tool_info()
    }

    async fn execute(&self, params: CallToolParams) -> Result<JsonRpcResponse> {
        // Implementation here - will be added later
        todo!()
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
        scraping_tool_info()
    }

    async fn execute(&self, params: CallToolParams) -> Result<JsonRpcResponse> {
        // Implementation here - will be added later
        todo!()
    }
}

// Application State
#[derive(Clone)]
pub struct AppState {
    tool_registry: ToolRegistry,
}

// Request/Response structures
#[derive(Deserialize, Debug)]
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
    debug!("Incoming tool call: {:?}", payload);

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

async fn index_page() -> Html<&'static str> {
    Html(INDEX_HTML)
}

// Initialize tools and create app state
fn initialize_tools() -> Result<ToolRegistry> {
    // Load configuration and create tools
    let graph_dir = std::env::var("KNOWLEDGE_GRAPH_DIR")
        .unwrap_or_else(|_| DEFAULT_GRAPH_DIR.to_string());
    let graph_path = std::path::PathBuf::from(&graph_dir)
        .join("knowledge_graph.json");
    
    let graph_manager = Arc::new(Mutex::new(
        GraphManager::new(graph_path.to_str().unwrap().to_string())
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
    <div id="function-calls">
        <h2>Function Call History</h2>
        <pre id="call-history"></pre>
    </div>
    <button id="btn-start">Start RTC</button>
    <script>
    const toolsList = document.getElementById('tools-list');
    const callHistory = document.getElementById('call-history');
    const btn = document.getElementById('btn-start');

    // Function to display tools info
    function displayTools(tools) {
        const toolsInfo = tools.map(tool => 
            `Tool: ${tool.name}\nDescription: ${tool.description}\nParameters: ${JSON.stringify(tool.parameters, null, 2)}\n`
        ).join('\n---\n');
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
        const tools = toolsData.result.tools.map(tool => ({
            type: "function",
            name: tool.name,
            description: tool.description,
            parameters: tool.input_schema
        }));

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
                // Initial configuration with system prompt
                const configEvent = {
                    type: "session.update",
                    session: {
                        tools,
                        tool_choice: "auto",
                        modalities: ["text"],
                        system_prompt: `You are a proactive assistant with access to powerful tools. FOLLOW TOOL DESCRIPTIONS EXACTLY.

CORE RESPONSIBILITIES:
1. CONTINUOUSLY CREATE KNOWLEDGE GRAPH NODES for:
   - Every piece of user information shared
   - All preferences mentioned
   - Any problems discussed
   - Goals and aspirations
   - Experiences described
   - Skills and expertise
   - Relationships mentioned
   - Decisions made
   - Questions asked
   - Opinions expressed

2. PROACTIVELY USE TOOLS:
   - Search for relevant information before responding
   - Suggest related resources and information
   - Offer to explore topics deeper using available tools
   - Connect new information to existing knowledge
   - Keep the knowledge graph current and interconnected

3. BE SUGGESTIVE AND HELPFUL:
   - "I could search for more information about X..."
   - "Would you like me to find resources related to Y?"
   - "I notice you're interested in Z, I can explore that topic further..."
   - "Based on your previous interests in A, you might want to learn about B..."
   - "I can help you discover more about this topic using our search tools..."

MANDATORY BEHAVIOR:
1. ALWAYS create nodes for new information
2. ALWAYS connect new nodes to related existing ones
3. ALWAYS suggest relevant tool usage to users
4. ALWAYS check existing knowledge before asking questions
5. NEVER wait for user permission to maintain the knowledge graph
6. CONTINUOUSLY offer to use tools proactively

CONVERSATION FLOW:
1. Start by checking existing knowledge
2. Proactively suggest relevant information
3. Create nodes for all new information
4. Connect information across nodes
5. Offer to explore topics deeper
6. Suggest related areas to explore

REMEMBER:
- Create nodes for EVERYTHING the user shares
- Be proactive with tool suggestions
- Continuously offer relevant information
- Keep the knowledge graph updated
- Make connections between topics
- Suggest related explorations

When you get information, don't mention it. Just use it to subtly inform the conversation going forward. It's jarring for the user when you info dump on them stuff they already know about their lives.`
                    }
                };
                dc.send(JSON.stringify(configEvent));

                // Initial response.create
                const responseCreate = {
                    type: "response.create",
                    response: {
                        modalities: ["text"],
                        instructions: "I'm ready to help you. What would you like to do?"
                    }
                };
                dc.send(JSON.stringify(responseCreate));
            };

            dc.onmessage = async (e) => {
                const data = JSON.parse(e.data);
                console.log("Message from model:", data);
                
                if (data.type === "function.call") {
                    const toolRequest = {
                        jsonrpc: "2.0",
                        id: 1,
                        method: "tools/call",
                        params: {
                            name: data.function.name,
                            arguments: data.function.arguments
                        }
                    };
                    
                    try {
                        const response = await fetch('/tools/call', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify(toolRequest)
                        });
                        
                        const result = await response.json();
                        console.log("Tool response:", result);
                        
                        // Add to call history
                        addToCallHistory(
                            data.function_call.name,
                            data.function_call.arguments,
                            result
                        );
                        
                        // Send tool result back to the model
                        // Send tool result back to the model
                        dc.send(JSON.stringify({
                            type: "function.response",
                            function: {
                                name: data.function.name,
                                content: result.result?.content?.[0]?.text || "Error executing tool",
                                status: result.error ? "error" : "success"
                            }
                        }));
                    } catch(err) {
                        console.error("Tool call error:", err);
                    }
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
        }));

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
