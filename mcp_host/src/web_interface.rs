use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Sse, sse::Event},
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

#[derive(Deserialize)]
pub struct UserQuery {
    user_input: String,
    session_id: String,
}

// New endpoint to receive frontend logs
async fn receive_frontend_log(
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
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

pub fn create_router(app_state: WebAppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/ask", post(ask))
        .route("/sse/:session_id", get(sse_handler))
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
      reconnectAttempts++;
      
      // Log detailed error information
      if (e.target.readyState === EventSource.CLOSED) {
        console.error('SSE connection closed unexpectedly');
      } else if (e.target.readyState === EventSource.CONNECTING) {
        console.error('SSE connection attempting to reconnect');
      }
      
      // Close the existing connection
      eventSource.close();
      console.log('Closed SSE connection due to error');
      
      if (reconnectAttempts <= MAX_RECONNECT_ATTEMPTS) {
        // Show reconnection attempt message
        streamArea.innerHTML += `<br><strong style='color:orange;'>[Connection interrupted. Reconnection attempt ${reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS}...]</strong>`;
        
        // Attempt to reconnect with exponential backoff
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
        // Max reconnection attempts reached
        streamArea.innerHTML += "<br><strong style='color:red;'>[Maximum reconnection attempts reached. Please refresh the page.]</strong>";
        console.error('Maximum reconnection attempts reached');
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

pub async fn ask(
    State(app_state): State<WebAppState>,
    Form(query): Form<UserQuery>,
) -> impl IntoResponse {
    log::info!("[ask] Received user_input: {:?}, session_id: {:?}", query.user_input, query.session_id);
    
    let user_input = query.user_input.trim().to_string();
    let session_id_str = query.session_id.clone();
    
    log::debug!("[ask] user_input after trim => '{}'", user_input);

    let session_id = if session_id_str.is_empty() {
        let generated = Uuid::new_v4();
        log::debug!(
            "[ask] session_id was empty; generating new UUID => {}",
            generated
        );
        generated
    } else {
        match Uuid::parse_str(&session_id_str) {
            Ok(id) => {
                log::debug!("[ask] using existing session_id => {}", id);
                id
            },
            Err(e) => {
                log::warn!(
                    "[ask] provided session_id was invalid UUID ({}), generating new ID",
                    e
                );
                Uuid::new_v4()
            },
        }
    };

    {
        let mut sessions = app_state.sessions.lock().await;
        let conversation = sessions.entry(session_id).or_insert_with(|| {
            log::debug!("[ask] Creating new ConversationState for session_id {}", session_id);
            ConversationState::new("Welcome to the HTMX + AI Demo!".to_string(), vec![])
        });
        log::debug!("[ask] Adding user message '{}' to session {}", user_input, session_id);
        conversation.add_user_message(&user_input);
    }

    let sse_url = format!("/sse/{}", session_id);
    log::info!(
        "[ask] Returning SSE URL => '{}' for session {}",
        sse_url,
        session_id
    );

    let result = serde_json::json!({
        "ok": true,
        "sse_url": sse_url,
    });

    Json(result)
}

#[axum::debug_handler]
pub async fn sse_handler(
    State(app_state): State<WebAppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    log::info!(
        "[sse_handler] SSE connection established for session: {}",
        session_id
    );
    let mut sessions = app_state.sessions.lock().await;
    let state = match sessions.get_mut(&session_id) {
        Some(conv) => {
            log::debug!("[sse_handler] Found conversation with {} messages", conv.messages.len());
            conv
        },
        None => {
            log::error!("[sse_handler] Session {} not found in sessions map", session_id);
            return Err((StatusCode::BAD_REQUEST, "Session not found".to_string()))
        },
    };

    // Get the last user message from the conversation

    // Process the message using the host's unified logic
    // Get the AI client
    let Some(client) = &app_state.host.ai_client else {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "No AI client configured".to_string()));
    };

    // Build the request with streaming enabled
    let mut builder = client.raw_builder();
    
    // Add system messages
    for msg in state.messages.iter().filter(|m| matches!(m.role, Role::System)) {
        builder = builder.system(msg.content.clone());
    }
    
    // Add conversation messages
    for msg in state.messages.iter().filter(|m| !matches!(m.role, Role::System)) {
        match msg.role {
            Role::User => builder = builder.user(msg.content.clone()),
            Role::Assistant => builder = builder.assistant(msg.content.clone()),
            _ => {}
        }
    }

    // Enable streaming
    builder = builder.streaming(true);

    // Execute streaming request
    match builder.execute_streaming().await {
        Ok(stream_result) => {
            log::info!("Started streaming response for session: {}", session_id);
            let event_stream = stream_result_to_sse(stream_result, state, &app_state);
            
            // After streaming completes, process any tool calls
            // Get the last message content before mutable borrow
            let last_msg_content = state.messages.last()
                .map(|m| m.content.clone())
                .unwrap_or_default();

            if let Some(client) = &app_state.host.ai_client {
                if let Err(e) = handle_assistant_response(
                    &app_state.host,
                    &last_msg_content,
                    "default",
                    state,
                    client
                ).await {
                    log::error!("Error in handle_assistant_response: {}", e);
                }
            }
            
            Ok(Sse::new(event_stream))
        }
        Err(e) => {
            log::error!("Failed to start streaming for session {}: {}", session_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("AI error: {}", e)))
        }
    }
}

fn stream_result_to_sse(
    stream_result: StreamResult,
    state: &mut ConversationState,
    app_state: &WebAppState,
) -> impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>> {
    log::debug!("[stream_result_to_sse] Converting stream result to SSE events");
    StreamExt::map(
        stream_result,
        |chunk_result| {
            match chunk_result {
                Ok(event) => {
                    log::debug!("Processing stream event: {:?}", event);
                    use crate::ai_client::StreamEvent;
                    match event {
                        StreamEvent::MessageStart { .. } => {
                            Ok(axum::response::sse::Event::default().data(""))
                        },
                        StreamEvent::ContentBlockStart { .. } => {
                            Ok(axum::response::sse::Event::default().data(""))
                        },
                        StreamEvent::ContentDelta { text, .. } => {
                            Ok(axum::response::sse::Event::default().data(text))
                        },
                        StreamEvent::ContentBlockStop { .. } => {
                            Ok(axum::response::sse::Event::default().data(""))
                        },
                        StreamEvent::MessageDelta { stop_reason, .. } => {
                            if let Some(reason) = stop_reason {
                                if reason.to_uppercase().contains("STOP") {
                                    return Ok(axum::response::sse::Event::default().data("[DONE]"));
                                }
                            }
                            Ok(axum::response::sse::Event::default().data(""))
                        },
                        StreamEvent::MessageStop => {
                            Ok(axum::response::sse::Event::default().data("[DONE]"))
                        },
                        StreamEvent::Error { message, .. } => {
                            let msg = format!("[ERROR] {}", message);
                            Ok(axum::response::sse::Event::default().data(msg))
                        },
                    }
                }
                Err(e) => {
                    let msg = format!("[ERROR] {}", e);
                    Ok(axum::response::sse::Event::default().data(msg))
                }
            }
        }
    )
}
