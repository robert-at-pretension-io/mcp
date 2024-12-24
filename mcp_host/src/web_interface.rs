use axum::{
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::{Html, IntoResponse},
    http::StatusCode,
    Json,
    routing::{post, Router, get},
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    convert::Infallible,
};
use tokio::sync::Mutex;
use uuid::Uuid;
use anyhow::Result;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use crate::{
    ai_client::StreamResult,
    conversation_state::ConversationState,
    MCPHost,
    conversation_service::handle_assistant_response,
};

use shared_protocol_objects::Role;

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




pub fn create_router(app_state: WebAppState) -> Router {
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

pub async fn root() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>HTMX + AI Streaming Demo</title>
  <script src="https://cdn.jsdelivr.net/npm/htmx.org@1.9.2"></script>
  <script>
    // Override console logging
    (function() {
        const originalConsole = {
            log: console.log,
            debug: console.debug,
            info: console.info,
            warn: console.warn,
            error: console.error
        };

        function sendToBackend(level, args) {
            const message = Array.from(args).map(arg => 
                typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
            ).join(' ');

            fetch('/frontend-log', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ level, message })
            }).catch(err => originalConsole.error('Failed to send log to backend:', err));
        }

        console.log = function(...args) {
            originalConsole.log.apply(console, args);
            sendToBackend('info', args);
        };

        console.debug = function(...args) {
            originalConsole.debug.apply(console, args);
            sendToBackend('debug', args);
        };

        console.info = function(...args) {
            originalConsole.info.apply(console, args);
            sendToBackend('info', args);
        };

        console.warn = function(...args) {
            originalConsole.warn.apply(console, args);
            sendToBackend('warn', args);
        };

        console.error = function(...args) {
            originalConsole.error.apply(console, args);
            sendToBackend('error', args);
        };
    })();
  </script>
</head>
<body>
  <h1>HTMX + AI Streaming Demo</h1>
  <div>
    <label>Session ID:
      <input type="text" id="sessionId" value="" placeholder="(auto-generated on first submit)">
    </label>
  </div>

  <form id="askForm"
        hx-post="/ask"
        hx-trigger="submit"
        hx-swap="none"
        style="margin-top: 1em;">
    <input type="text" name="user_input" placeholder="Ask me something..." />
    <input type="hidden" name="session_id" />
    <button type="submit">Send</button>
  </form>

  <div id="streamArea" style="border: 1px solid #ccc; padding: 1em; margin-top: 1em;">
  </div>

<script>
// Initialize logging
console.log('[DEBUG] Initializing web interface...');

document.getElementById('askForm').addEventListener('submit', function(evt) {
  console.log('[DEBUG] askForm submit event triggered');
  evt.preventDefault();

  let form = evt.target;
  let user_input = form.user_input.value.trim();
  console.log('[DEBUG] user_input:', user_input);
  
  if (!user_input) {
    console.log('[DEBUG] user_input is empty, not sending request.');
    return;
  }

  let sessionElem = document.getElementById('sessionId');
  if (!sessionElem.value) {
    console.log('[DEBUG] No sessionId found, generating...');
    sessionElem.value = crypto.randomUUID();
    console.log('[DEBUG] Generated new sessionId:', sessionElem.value);
  } else {
    console.log('[DEBUG] Using existing sessionId:', sessionElem.value);
  }

  form.session_id.value = sessionElem.value;
  console.log(`[DEBUG] Submitting /ask with user_input='${user_input}', session_id='${sessionElem.value}'`);

  fetch('/ask', {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams(new FormData(form))
  }).then(response => {
    console.log('[DEBUG] /ask response status:', response.status);
    if (!response.ok) {
      console.error('[ERROR] /ask returned non-OK status:', response.status);
      alert("Error from server: " + response.status);
      return;
    }
    return response.json();
  }).then(data => {
    console.log('[DEBUG] /ask JSON response:', data);
    if (!data || !data.ok) {
      console.warn('[DEBUG] Data was not ok or missing:', data);
      alert("No valid SSE path returned from /ask");
      return;
    }
    
    let sseUrl = data.sse_url;
    console.log('[DEBUG] Creating EventSource for SSE at', sseUrl);
    let eventSource = new EventSource(sseUrl);
    let streamArea = document.getElementById('streamArea');
    streamArea.innerHTML = "";
    console.log('[DEBUG] Cleared streamArea');

    eventSource.onopen = (e) => {
      console.log('[DEBUG] SSE onopen => readyState:', eventSource.readyState);
      console.log('[DEBUG] SSE connection details:', {
        url: eventSource.url,
        readyState: eventSource.readyState,
        withCredentials: eventSource.withCredentials
      });
      streamArea.innerHTML = "<em style='color:green;'>Connected successfully...</em><br>";
    };

    eventSource.onmessage = (e) => {
      console.log('[DEBUG] SSE onmessage => data:', e.data);
      if (e.data === "[DONE]") {
        console.log('[DEBUG] SSE [DONE] signal received, closing eventSource');
        eventSource.close();
        console.log('[DEBUG] EventSource closed, final readyState:', eventSource.readyState);
        return;
      }
      streamArea.innerHTML += e.data;
      console.log('[DEBUG] Updated streamArea, content length:', streamArea.innerHTML.length);
    };

    // Add onclose handler
    eventSource.onclose = (e) => {
      console.log('[DEBUG] SSE onclose event:', e);
      console.log('[DEBUG] SSE final state:', {
        readyState: eventSource.readyState,
        reconnection: false
      });
    };

    let reconnectAttempts = 0;
    const MAX_RECONNECT_ATTEMPTS = 3;
    const RECONNECT_DELAY = 2000;

    eventSource.onerror = function(e) {
      console.error('SSE error occurred:', e);
      
      // Check if the connection is already closed
      if (eventSource.readyState === EventSource.CLOSED) {
        console.log('SSE connection already closed, not reconnecting');
        return;
      }
      
      // Close the existing connection
      eventSource.close();
      console.log('Closed SSE connection due to error');
      
      // Only attempt reconnect for network-related errors
      if (e.target.readyState === EventSource.CONNECTING) {
        reconnectAttempts++;
        if (reconnectAttempts <= MAX_RECONNECT_ATTEMPTS) {
          console.log(`Attempting reconnect ${reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS}`);
          streamArea.innerHTML += `<br><strong style='color:orange;'>[Connection interrupted. Reconnection attempt ${reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS}...]</strong>`;
          
          setTimeout(() => {
            console.log(`Attempting to reconnect (${reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS})...`);
            fetch('/ask', {
              method: 'POST',
              headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
              body: new URLSearchParams(new FormData(form))
            }).then(response => {
              if (!response.ok) throw new Error(`Server error: ${response.status}`);
              return response.json();
            }).then(data => {
              if (!data || !data.ok) throw new Error('Invalid response data');
              eventSource = new EventSource(data.sse_url);
              console.log('Reconnected to new EventSource:', data.sse_url);
              streamArea.innerHTML += "<br><em style='color:green;'>Reconnected successfully.</em>";
            }).catch(err => {
              console.error('Reconnection failed:', err);
              streamArea.innerHTML += "<br><strong style='color:red;'>[Reconnection failed. Please refresh the page.]</strong>";
            });
          }, RECONNECT_DELAY * Math.pow(2, reconnectAttempts - 1));
        } else {
          streamArea.innerHTML += "<br><strong style='color:red;'>[Maximum reconnection attempts reached. Please refresh the page.]</strong>";
          console.error('Maximum reconnection attempts reached');
        }
      } else {
        console.log('Non-recoverable error, not attempting reconnect');
        streamArea.innerHTML += "<br><strong style='color:red;'>[Connection terminated. Please refresh the page.]</strong>";
      }
    };
  }).catch(err => {
    console.error('Request failed:', err);
    alert("Request error: " + err);
  });
  
  console.log('Form submission handler completed');
});

// Log when the page is fully loaded
window.addEventListener('load', function() {
  console.log('Page fully loaded');
  console.log('Session ID element:', document.getElementById('sessionId'));
  console.log('Stream area element:', document.getElementById('streamArea'));
});
</script>
</body>
</html>
"#;

    Html(html)
}

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
    app_state: &WebAppState,
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

