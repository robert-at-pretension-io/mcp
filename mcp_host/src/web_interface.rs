use axum::{
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::{Html, IntoResponse},
    http::StatusCode,
    Json,
    routing::{post, get, Router},
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::Mutex;
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
    let html = r#"
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>WebSocket + AI Demo</title>
</head>
<body>
  <h1>WebSocket Demo</h1>
  <textarea id="chatLog" cols="80" rows="15" readonly></textarea><br>
  <input id="userInput" type="text" placeholder="Type a message..." />
  <button onclick="sendMsg()">Send</button>

  <script>
    // Setup logs
    console.log('[INFO] Starting WebSocket demo');

    const ws = new WebSocket(`ws://${location.host}/ws`);
    ws.onopen = () => {
      console.log('WebSocket connected');
    };
    ws.onmessage = (evt) => {
      try {
        const msg = JSON.parse(evt.data);
        if(msg.type === "token") {
          chatLog.value += msg.data;
        } else if(msg.type === "done") {
          chatLog.value += "\n[Done]\n";
        } else if(msg.type === "error") {
          chatLog.value += "\n[ERROR] " + msg.data + "\n";
        }
      } catch(e) {
        console.error('Invalid JSON from server:', evt.data);
      }
    };
    ws.onclose = () => {
      console.log('WebSocket closed');
    };

    const chatLog = document.getElementById("chatLog");

    let sessionId = null; // We can generate if needed
    function sendMsg() {
      const input = document.getElementById("userInput");
      const text = input.value.trim();
      if(!text) return;

      if(!sessionId) sessionId = crypto.randomUUID();
      const payload = {
        session_id: sessionId,
        user_input: text
      };
      ws.send(JSON.stringify(payload));
      input.value = "";
    }
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