use axum::{
    extract::{Form, State},
    response::{Html, IntoResponse, Sse},
    routing::{get, post},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tokio_stream::StreamExt;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;
use futures::stream::BoxStream;
use anyhow::Result;
use crate::{
    ai_client::{AIClient, AIRequestBuilder, StreamResult},
    conversation_state::{ConversationState, Message},
    shared_protocol_objects::Role,
};
use axum::Router;

#[derive(Clone)]
pub struct WebAppState {
    pub sessions: Arc<Mutex<HashMap<Uuid, ConversationState>>>,
    pub ai_client: Arc<dyn AIClient + Send + Sync>,
}

impl WebAppState {
    pub fn new(ai_client: Arc<dyn AIClient + Send + Sync>) -> Self {
        WebAppState {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            ai_client,
        }
    }
}

#[derive(Deserialize)]
pub struct UserQuery {
    user_input: String,
    session_id: String,
}

pub async fn root() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>HTMX + AI Streaming Demo</title>
  <script src="https://cdn.jsdelivr.net/npm/htmx.org@1.9.2"></script>
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
document.getElementById('askForm').addEventListener('submit', function(evt) {
  evt.preventDefault();

  let form = evt.target;
  let user_input = form.user_input.value;
  if (!user_input.trim()) return;

  let sessionElem = document.getElementById('sessionId');
  if (!sessionElem.value) {
    sessionElem.value = crypto.randomUUID();
  }

  form.session_id.value = sessionElem.value;

  fetch('/ask', {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams(new FormData(form))
  }).then(response => {
    if (!response.ok) {
      alert("Error from server: " + response.status);
      return;
    }
    return response.json();
  }).then(data => {
    if (!data || !data.ok) {
      alert("No valid SSE path returned");
      return;
    }

    let eventSource = new EventSource(data.sse_url);
    let streamArea = document.getElementById('streamArea');
    streamArea.innerHTML = "";

    eventSource.onmessage = function(e) {
      if (e.data === "[DONE]") {
        eventSource.close();
        return;
      }
      streamArea.innerHTML += e.data;
    };

    eventSource.onerror = function(e) {
      streamArea.innerHTML += "<br><strong style='color:red;'>[Stream error occurred]</strong>";
      eventSource.close();
    };
  }).catch(err => {
    alert("Request error: " + err);
  });
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
    let user_input = query.user_input.trim().to_string();
    let session_id_str = query.session_id.clone();

    let session_id = if session_id_str.is_empty() {
        Uuid::new_v4()
    } else {
        match Uuid::parse_str(&session_id_str) {
            Ok(id) => id,
            Err(_) => Uuid::new_v4(),
        }
    };

    {
        let mut sessions = app_state.sessions.lock().unwrap();
        let entry = sessions.entry(session_id).or_insert_with(|| {
            ConversationState::new("Welcome to the HTMX + AI Demo!".to_string(), vec![])
        });
        entry.add_user_message(&user_input);
    }

    let sse_url = format!("/sse/{}", session_id);
    let result = serde_json::json!({
        "ok": true,
        "sse_url": sse_url,
    });

    Json(result)
}

pub async fn sse_handler(
    State(app_state): State<WebAppState>,
    axum::extract::Path(session_id): axum::extract::Path<Uuid>,
) -> Result<Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, (StatusCode, String)> {
    let mut session_guard = app_state.sessions.lock().unwrap();
    let conversation = match session_guard.get_mut(&session_id) {
        Some(conv) => conv.clone(),
        None => {
            return Err((StatusCode::BAD_REQUEST, "Session not found".to_string()));
        }
    };

    let mut builder = app_state.ai_client.raw_builder().streaming(true);

    for msg in &conversation.messages {
        match msg.role {
            Role::System => { builder = builder.system(msg.content.clone()); },
            Role::User => { builder = builder.user(msg.content.clone()); },
            Role::Assistant => { builder = builder.assistant(msg.content.clone()); },
        }
    }

    match builder.execute_streaming().await {
        Ok(stream_result) => {
            let sse_stream = stream_result_to_sse(stream_result);
            Ok(Sse::new(sse_stream))
        }
        Err(err) => {
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("AI error: {}", err)))
        }
    }
}

fn stream_result_to_sse(
    mut stream_result: StreamResult
) -> impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>> {
    StreamExt::map(
        Box::pin(&mut stream_result),
        |chunk_result| {
            match chunk_result {
                Ok(event) => {
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
