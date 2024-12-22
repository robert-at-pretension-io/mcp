use axum::{
    extract::Query,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod graph_manager;
mod graph_tool_impl;

#[derive(Clone)]
pub struct AppState {
    graph_manager: Arc<Mutex<graph_manager::GraphManager>>,
}

#[derive(Deserialize)]
struct SessionQuery {
    model: Option<String>,
}

async fn get_ephemeral_token(
    Query(q): Query<SessionQuery>,
    state: Arc<Mutex<AppState>>,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct OpenAiFunctionCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ToolCallRequest {
    pub model: Option<String>,
    pub function_call: OpenAiFunctionCall,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ToolCallResponse {
    pub name: String,
    pub result: serde_json::Value,
}

async fn handle_tools_call(
    Json(payload): Json<ToolCallRequest>,
    state: Arc<Mutex<AppState>>,
) -> impl IntoResponse {
    debug!("Incoming function call: {:?}", payload);

    if payload.function_call.name != "graph_tool" {
        return Json(json!({"error": "Unsupported function name"}));
    }

    let call_params = match serde_json::from_value::<graph_tool_impl::CallToolParams>(
        payload.function_call.arguments,
    ) {
        Ok(cp) => cp,
        Err(e) => {
            error!("Parsing error: {e}");
            return Json(json!({"error": format!("Could not parse arguments: {e}")}));
        }
    };

    let mut app_state = state.lock().await;
    let mut gm = app_state.graph_manager.lock().await;

    match graph_tool_impl::handle_graph_tool_call(call_params, &mut gm, None).await {
        Ok(response) => {
            Json(json!({
                "name": payload.function_call.name,
                "result": response.result
            }))
        }
        Err(e) => {
            error!("Graph tool call error: {e}");
            Json(json!({"error": e.to_string()}))
        }
    }
}

async fn index_page() -> Html<&'static str> {
    Html(INDEX_HTML)
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8" />
  <title>Realtime Voice + Tools Demo</title>
</head>
<body>
  <h1>Realtime Voice + Tools Demo</h1>
  <button id="btn-start">Start RTC</button>
  <script>
  const btn = document.getElementById('btn-start');
  btn.addEventListener('click', async () => {
    const model = "gpt-4o-realtime-preview-2024-12-17";
    try {
      const sessionRes = await fetch(`/session?model=${model}`);
      const sessionData = await sessionRes.json();
      console.log('Session data:', sessionData);
      
      const ephemeralKey = sessionData?.client_secret?.value;
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
        const configEvent = {
          type: "session.update",
          session: {
            tools: [{
              type: "function",
              name: "graph_tool",
              description: "Full graph tool usage",
              parameters: {
                type: "object",
                properties: {
                  action: { type: "string" },
                  params: { type: "object" }
                },
                required: ["action","params"]
              }
            }],
            tool_choice: "auto"
          }
        };
        dc.send(JSON.stringify(configEvent));
      };
      dc.onmessage = e => {
        console.log("Message from model:", e.data);
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
</html>
"#;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry().with(tracing_subscriber::fmt::layer()).init();

    let gm = graph_manager::GraphManager::new("my_graph.json".to_string());
    let state = Arc::new(Mutex::new(AppState {
        graph_manager: Arc::new(Mutex::new(gm)),
    }));

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

    let addr = "0.0.0.0:3000";
    info!("Server running on {}", addr);
    axum::Server::bind(&addr.parse()?)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
