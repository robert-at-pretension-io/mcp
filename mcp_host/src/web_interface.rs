use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::{get, post, Router},
    http::StatusCode,
    Json,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::Value;
use uuid::Uuid;
use anyhow::Result;
use futures::StreamExt;
use crate::{
    ai_client::StreamEvent,
    conversation_state::ConversationState,
    MCPHost,
};
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
  <title>WebSocket + AI Demo</title>
  <!-- Markdown library from a CDN -->
  <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
</head>
<body>
  <h1>WebSocket Demo</h1>

  <!-- Container for chat messages -->
  <div id="chatContainer" style="border:1px solid #ccc; width:600px; height:300px; overflow:auto; padding:5px;"></div>

  <!-- Input field -->
  <input id="userInput" type="text" placeholder="Type a message..." style="width:600px;"/>
  <button id="sendBtn">Send</button>

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

    // We display messages in #chatContainer, each in its own <div>
    // We'll parse text as markdown (via "marked") to convert to HTML.
    const chatContainer = document.getElementById("chatContainer");
    function addMessage(text, from='assistant') {
      const msgDiv = document.createElement('div');
      // Use marked to parse the string as markdown
      msgDiv.innerHTML = marked.parse(text);
      msgDiv.style.borderTop = "1px solid #ccc";
      msgDiv.style.margin = "4px 0";
      chatContainer.appendChild(msgDiv);
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }

    // On receiving server messages, we parse them as JSON
    ws.onmessage = (evt) => {
      try {
        const msg = JSON.parse(evt.data);
        if(msg.type === "token") {
          // We'll treat each token chunk as part of a single response
          addMessage(msg.data, 'assistant');
        } else if(msg.type === "done") {
          // End of message
          addMessage('[Done]', 'assistant');
        } else if(msg.type === "error") {
          addMessage('[ERROR] ' + msg.data, 'assistant');
        }
      } catch(e) {
        console.error('Invalid JSON from server:', evt.data);
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
      addMessage(text, 'user');
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

    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            let parsed: WsRequest = match serde_json::from_str(&text) {
                Ok(req) => req,
                Err(e) => {
                    log::error!("[WS] Could not parse JSON request: {}", e);
                    continue;
                }
            };

            let session_id = resolve_session_id(parsed.session_id, &app_state).await;
            let user_input = parsed.user_input.trim().to_string();

            {
                let mut sessions = app_state.sessions.lock().await;
                let convo = sessions
                    .entry(session_id)
                    .or_insert_with(|| ConversationState::new("Welcome!".to_string(), vec![]));
                convo.add_user_message(&user_input);
            }

            // Stream from AI client
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
                        Role::System => {
                            builder = builder.system(m.content.clone());
                        }
                        Role::User => {
                            builder = builder.user(m.content.clone());
                        }
                        Role::Assistant => {
                            builder = builder.assistant(m.content.clone());
                        }
                    }
                }
                builder.execute_streaming().await
            };

            match stream_result {
                Ok(mut s) => {
                    while let Some(chunk_res) = s.next().await {
                        match chunk_res {
                            Ok(event) => {
                                match event {
                                    StreamEvent::ContentDelta{ text, .. } => {
                                        let json_msg = serde_json::json!({
                                            "type": "token",
                                            "data": text
                                        });
                                        if socket.send(Message::Text(json_msg.to_string())).await.is_err() {
                                            break;
                                        }
                                    }
                                    StreamEvent::MessageStop => {
                                        let done_msg = serde_json::json!({"type":"done"});
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
