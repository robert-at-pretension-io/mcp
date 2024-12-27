use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::{get, post, Router},
    http::StatusCode,
    Json,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
};
use crate::{
    ai_client::StreamEvent,
    conversation_state::ConversationState,
    MCPHost,
    conversation_service::{self, parse_tool_call},
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::Value;
use uuid::Uuid;
use anyhow::Result;
use futures::StreamExt;

use shared_protocol_objects::Role;



// ---------------------------------------------------------------------------
// No changes in the WebAppState or WsRequest structures
// ---------------------------------------------------------------------------
#[derive(Clone)]
pub struct WebAppState {
    pub sessions: Arc<Mutex<HashMap<Uuid, ConversationState>>>,
    pub host: Arc<MCPHost>,
}

impl WebAppState {
    pub fn new(host: Arc<MCPHost>) -> Self {
        WebAppState {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            host,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct WsRequest {
    session_id: Option<String>,
    user_input: String,
}

// ---------------------------------------------------------------------------
// CHANGED: Create router without SSE-based endpoints
// ---------------------------------------------------------------------------
pub fn create_router(app_state: WebAppState) -> Router {
    // Removed any .route("/ask") or SSE routes
    Router::new()
        .route("/", get(root))
        .route("/ws", get(ws_handler))
        .route("/frontend-log", post(receive_frontend_log))
        .with_state(app_state)
}

async fn receive_frontend_log(Json(payload): Json<Value>) -> impl IntoResponse {
    if let Some(level) = payload.get("level").and_then(|v| v.as_str()) {
        if let Some(msg) = payload.get("message").and_then(|v| v.as_str()) {
            match level {
                "debug" => log::debug!("[Frontend] {}", msg),
                "info" => log::info!("[Frontend] {}", msg),
                "warn" => log::warn!("[Frontend] {}", msg),
                "error" => log::error!("[Frontend] {}", msg),
                _ => log::info!("[Frontend] {}", msg),
            }
        }
    }
    StatusCode::OK
}

// ---------------------------------------------------------------------------
// CHANGED: root() now includes simple WebSocket-based HTML
// ---------------------------------------------------------------------------
pub async fn root() -> impl IntoResponse {
    // We use a <div> (#chatContainer) to hold messages, each in its own <div>.
    // We include "marked" from CDN to parse markdown for each chunk.
    // Pressing Enter sends the message. No custom CSS is used, just basic HTML.
    let html = r#"
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AI Chat Interface</title>
  <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/picocss/2.0.6/pico.classless.min.css">
  <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
  <style>
    #chatContainer {
      height: 70vh;
      overflow-y: auto;
      border: 1px solid var(--pico-muted-border-color);
      border-radius: var(--pico-border-radius);
      padding: 1rem;
      margin-bottom: 1rem;
      background: var(--pico-background-color);
    }
    .input-group {
      display: flex;
      gap: 0.5rem;
    }
    .input-group input {
      flex: 1;
      margin-bottom: 0;
    }
  </style>
</head>
<body>
  <main class="container">
    <h1>AI Chat Interface</h1>
    
    <div id="chatContainer"></div>

    <div class="input-group">
      <input type="text" id="userInput" placeholder="Type your message..."/>
      <button id="sendBtn">Send</button>
    </div>

  <script>
    console.log('[INFO] Starting WebSocket demo');

    // Connect to WebSocket
    const ws = new WebSocket(`ws://${location.host}/ws`);

    // Logging
    ws.onopen = () => {
      console.log('WebSocket connected');
    };
    ws.onclose = () => {
      console.log('WebSocket closed');
    };
    ws.onerror = (err) => {
      console.error('WebSocket error:', err);
    };

    // Store the entire assistant's partial message as it streams in
    let assistantMarkdown = "";
    let currentAssistantDiv = null;

    function startNewAssistantMessage() {
      assistantMarkdown = ""; // reset
      currentAssistantDiv = document.createElement('div');
      currentAssistantDiv.style.padding = "0.5rem";
      currentAssistantDiv.style.margin = "0.5rem 0";
      currentAssistantDiv.style.background = "var(--pico-card-background-color)";
      currentAssistantDiv.style.borderRadius = "var(--pico-border-radius)";
      chatContainer.appendChild(currentAssistantDiv);
    }

    function appendToAssistantMessage(textChunk) {
      // Add chunk to the ongoing buffer
      assistantMarkdown += textChunk;
      // Then do a full parse of the entire buffer
      const html = marked.parse(assistantMarkdown);
      
      // Re-render the entire message in the same div
      currentAssistantDiv.innerHTML = html;
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }

    function showLoadingIndicator(toolName) {
      const indicator = document.createElement('div');
      indicator.id = `loading-indicator-${toolName}`;
      indicator.style.padding = "0.5rem";
      indicator.style.margin = "0.5rem 0";
      indicator.style.background = "var(--pico-card-background-color)";
      indicator.style.borderRadius = "var(--pico-border-radius)";
      indicator.style.color = "var(--pico-muted-color)";
      indicator.innerHTML = `<span style="display: inline-block; animation: spin 1s linear infinite">⚙️</span> Running ${toolName}...`;
      chatContainer.appendChild(indicator);
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }

    function hideLoadingIndicator(toolName) {
      const indicator = document.getElementById(`loading-indicator-${toolName}`);
      if (indicator) {
        indicator.remove();
      }
    }

    // Add spinning animation
    const style = document.createElement('style');
    style.textContent = `
      @keyframes spin {
        from { transform: rotate(0deg); }
        to { transform: rotate(360deg); }
      }
    `;
    document.head.appendChild(style);

    // Add user messages in a different style
    function addUserMessage(text) {
      const msgDiv = document.createElement('div');
      msgDiv.innerHTML = marked.parse(text);
      msgDiv.style.padding = "0.5rem";
      msgDiv.style.margin = "0.5rem 0";
      msgDiv.style.background = "var(--pico-form-element-background-color)";
      msgDiv.style.borderRadius = "var(--pico-border-radius)";
      chatContainer.appendChild(msgDiv);
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }

    const chatContainer = document.getElementById("chatContainer");
    ws.onmessage = (evt) => {
      try {
        const msg = JSON.parse(evt.data);
        if (msg.type === "token") {
          if (!currentAssistantDiv) {
            startNewAssistantMessage();
          }
          appendToAssistantMessage(msg.data);
        } else if (msg.type === "done") {
          // Optionally finalize
          appendToAssistantMessage("\n[Done]");
          currentAssistantDiv = null;
        } else if (msg.type === "error") {
          startNewAssistantMessage();
          appendToAssistantMessage("[ERROR] " + msg.data);
          currentAssistantDiv = null;
        } else if (msg.type === "tool_call_start") {
          showLoadingIndicator(msg.tool_name);
        } else if (msg.type === "tool_call_end") {
          hideLoadingIndicator(msg.tool_name);
        }
      } catch(e) {
        console.error("Invalid JSON from server:", evt.data);
      }
    };

    // Pressing Enter also sends the message
    const inputField = document.getElementById("userInput");
    const sendBtn = document.getElementById("sendBtn");

    function sendMsg() {
      const text = inputField.value.trim();
      if(!text) return;
      // We'll let the server generate a session if needed
      if(!window.sessionId) window.sessionId = crypto.randomUUID();
      const payload = { session_id: window.sessionId, user_input: text };

      // Add user message to chat
      addUserMessage(text);
      inputField.value = "";

      // Send to WebSocket
      ws.send(JSON.stringify(payload));
    }

    // Press Enter to send
    inputField.addEventListener("keydown", function(e) {
      if(e.key === "Enter") {
        e.preventDefault();
        sendMsg();
      }
    });

    // Clicking button also sends
    sendBtn.onclick = () => {
      sendMsg();
    };
  </script>
</body>
</html>
"#;

    Html(html)
}

// ---------------------------------------------------------------------------
// WebSocket route (unchanged except references to SSE are gone)
// ---------------------------------------------------------------------------
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(app_state): State<WebAppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move {
        if let Err(e) = handle_ws(socket, app_state).await {
            log::error!("[ws_handler] WebSocket error: {:?}", e);
        }
    })
}

async fn handle_ws(mut socket: WebSocket, app_state: WebAppState) -> Result<()> {
    log::info!("[WS] New WebSocket connection");

    let mut accumulated_message = String::new();

    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            let parsed: WsRequest = match serde_json::from_str(&text) {
                Ok(req) => req,
                Err(e) => {
                    log::error!("[WS] Could not parse JSON request: {}", e);
                    let err_msg = serde_json::json!({
                        "type": "error",
                        "data": format!("Invalid request: {}", e)
                    });
                    socket.send(Message::Text(err_msg.to_string())).await?;
                    continue;
                }
            };

            let session_id = resolve_session_id(parsed.session_id, &app_state).await;
            let user_input = parsed.user_input.trim().to_string();

            // Initialize new conversation if needed
            let new_convo = {
                let mut sessions = app_state.sessions.lock().await;
                if !sessions.contains_key(&session_id) {
                    drop(sessions);
                    match app_state.host.enter_chat_mode("api").await {
                        Ok(state) => Some(state),
                        Err(e) => {
                            log::warn!("Error calling enter_chat_mode: {}", e);
                            Some(ConversationState::new("Welcome!".to_string(), vec![]))
                        }
                    }
                } else {
                    None
                }
            };

            if let Some(convo) = new_convo {
                let mut sessions = app_state.sessions.lock().await;
                sessions.insert(session_id, convo);
            }

            // Add user message
            {
                let mut sessions = app_state.sessions.lock().await;
                let convo = sessions.get_mut(&session_id).unwrap();
                convo.add_user_message(&user_input);
            }

            // Get AI response
            let stream_result = {
                let sessions = app_state.sessions.lock().await;
                let convo = sessions.get(&session_id).unwrap();
                let client = app_state
                    .host
                    .ai_client
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("No AI client configured"))?;

                let mut builder = client.raw_builder().streaming(true);
                for m in &convo.messages {
                    match m.role {
                        Role::System => builder = builder.system(m.content.clone()),
                        Role::User => builder = builder.user(m.content.clone()),
                        Role::Assistant => builder = builder.assistant(m.content.clone()),
                    }
                }
                builder.execute_streaming().await
            };

            match stream_result {
                Ok(mut s) => {
                    accumulated_message.clear();
                    
                    while let Some(chunk_res) = s.next().await {
                        match chunk_res {
                            Ok(event) => {
                                match event {
                                    StreamEvent::ContentDelta{ text, .. } => {
                                        accumulated_message.push_str(&text);
                                        
                                        let json_msg = serde_json::json!({
                                            "type": "token",
                                            "data": text
                                        });
                                        if socket.send(Message::Text(json_msg.to_string())).await.is_err() {
                                            break;
                                        }
                                    }
                                    StreamEvent::MessageStop => {
                                        // Handle tool calls with the complete accumulated message
                                        if let Err(e) = do_multi_tool_loop(
                                            &app_state,
                                            session_id,
                                            &mut accumulated_message,
                                            &mut socket
                                        ).await {
                                            log::error!("Tool handling error: {}", e);
                                            let err_msg = serde_json::json!({
                                                "type": "error",
                                                "data": format!("Tool handling error: {}", e)
                                            });
                                            let _ = socket.send(Message::Text(err_msg.to_string())).await;
                                        }

                                        // Notify completion
                                        let done_msg = serde_json::json!({"type": "done"});
                                        let _ = socket.send(Message::Text(done_msg.to_string())).await;
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            Err(e) => {
                                log::error!("Error in streaming: {}", e);
                                let err_msg = serde_json::json!({
                                    "type": "error",
                                    "data": e.to_string()
                                });
                                let _ = socket.send(Message::Text(err_msg.to_string())).await;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("AI client error: {}", e);
                    let err_msg = serde_json::json!({
                        "type": "error",
                        "data": e.to_string()
                    });
                    let _ = socket.send(Message::Text(err_msg.to_string())).await;
                }
            }
        }
    }

    log::info!("[WS] WebSocket closed");
    Ok(())
}

async fn resolve_session_id(
    provided: Option<String>,
    _app_state: &WebAppState,
) -> Uuid {
    if let Some(sid) = provided {
        if let Ok(parsed) = Uuid::parse_str(&sid) {
            return parsed;
        }
    }
    let new_id = Uuid::new_v4();
    log::info!("[WS] Generating new session_id: {}", new_id);
    new_id
}

async fn run_single_stream_pass(
    app_state: &WebAppState,
    session_id: Uuid,
) -> Result<String> {
    let sessions = app_state.sessions.lock().await;
    let convo = sessions.get(&session_id)
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

    let client = app_state.host.ai_client.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No AI client configured"))?;

    let mut builder = client.raw_builder();
    for msg in &convo.messages {
        match msg.role {
            Role::System => builder = builder.system(msg.content.clone()),
            Role::User => builder = builder.user(msg.content.clone()),
            Role::Assistant => builder = builder.assistant(msg.content.clone()),
        }
    }

    builder.execute().await
}

async fn do_multi_tool_loop(
    app_state: &WebAppState,
    session_id: Uuid,
    partial_response: &mut String,
    socket: &mut WebSocket,
) -> Result<()> {
    // Get the conversation state
    let mut sessions = app_state.sessions.lock().await;
    let convo = sessions.get_mut(&session_id)
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
    
    // Get the AI client
    let client = app_state.host.ai_client.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No AI client configured"))?;

    // Use handle_assistant_response directly
    match crate::conversation_service::handle_assistant_response(
        &app_state.host,
        partial_response,
        "api",
        convo,
        client,
        socket
    ).await {
        Ok(()) => {
            log::debug!("Successfully handled assistant response");
            Ok(())
        },
        Err(e) => {
            log::error!("Error handling assistant response: {}", e);
            // Send error directly through WebSocket
            let err_msg = serde_json::json!({
                "type": "error",
                "data": format!("Error processing response: {}", e)
            });
            socket.send(Message::Text(err_msg.to_string())).await?;
            Err(anyhow::anyhow!(e))
        }
    }
}
