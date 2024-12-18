use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use anyhow::{Result, Context};
use dotenv::dotenv;
use reqwest::Client;
use std::env;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing_subscriber::fmt::init as tracing_init;


#[derive(Deserialize, Serialize)]
struct EphemeralKeyResponse {
    client_secret: ClientSecret,
}

#[derive(Deserialize, Serialize)]
struct ClientSecret {
    value: String,
    expires_at: i64,
}

#[derive(Clone)]
struct AppState {
    openai_api_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_init();

    dotenv().ok(); // Load .env file if present
    
    let openai_api_key = env::var("OPENAI_API_KEY")
        .context("OPENAI_API_KEY environment variable not set")?;
        
    let state = AppState {
        openai_api_key,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/session", get(session))
        .with_state(state);

    let addr = SocketAddr::from(([127,0,0,1], 3000));
    println!("Server running at http://{}/", addr);
    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app
    ).await?;

    Ok(())
}

async fn index() -> impl IntoResponse {
    // A simple HTML page that:
    // 1. On load, fetches "/session" for ephemeral key.
    // 2. Uses ephemeral key to set up WebRTC with OpenAI Realtime API.
    // 3. Shows a button to trigger function calling scenario.

    let html = r#"
<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8" />
<title>OpenAI Realtime WebRTC Example</title>
</head>
<body>
<h1>OpenAI Realtime WebRTC</h1>
<div id="status">Loading...</div>
<button id="callFunctionBtn">Call Function</button>
<script>
(async () => {
  const statusEl = document.getElementById('status');
  const btn = document.getElementById('callFunctionBtn');
  // 1. Get ephemeral key
  const resp = await fetch('/session');
  const data = await resp.json();
  const EPHEMERAL_KEY = data.client_secret.value;
  console.log('Ephemeral key:', EPHEMERAL_KEY);

  const pc = new RTCPeerConnection();

  // Show remote audio
  const audioEl = document.createElement('audio');
  audioEl.autoplay = true;
  pc.ontrack = e => {
    audioEl.srcObject = e.streams[0];
  };
  document.body.appendChild(audioEl);

  // Add local audio track if you want. For simplicity, skip local audio.
  // Or if needed:
  // const ms = await navigator.mediaDevices.getUserMedia({audio:true});
  // pc.addTrack(ms.getTracks()[0], ms);

  const dc = pc.createDataChannel('oai-events');
  dc.addEventListener('open', () => {
    console.log('Data channel open');
    statusEl.textContent = 'Connected!';
  });
  dc.addEventListener('message', e => {
    console.log('Received event:', e.data);
    // Handle events from server: function_call, response, etc.
    const evt = JSON.parse(e.data);
    if(evt.type === 'response.function_call_arguments.done') {
      // Model has called a function, we have arguments in evt.arguments
      const args = JSON.parse(evt.arguments);
      console.log('Function call arguments:', args);
      // Here we'd execute the function and send results back:
      const result = { sum: (args.a + args.b) };
      // Return result to model:
      const fn_output_event = {
        type: 'conversation.item.create',
        item: {
          type: 'function_call_output',
          call_id: evt.call_id,
          output: JSON.stringify(result)
        }
      };
      dc.send(JSON.stringify(fn_output_event));
      // Then ask model to respond:
      dc.send(JSON.stringify({type:'response.create'}));
    }
  });

  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);

  const baseUrl = "https://api.openai.com/v1/realtime";
  const model = "gpt-4o-realtime-preview-2024-12-17";
  const sdpResponse = await fetch(`${baseUrl}?model=${model}`, {
    method: "POST",
    body: offer.sdp,
    headers: {
      "Authorization": `Bearer ${EPHEMERAL_KEY}`,
      "Content-Type": "application/sdp"
    }
  });
  const answerSdp = await sdpResponse.text();
  const answer = {
    type: "answer",
    sdp: answerSdp,
  };
  await pc.setRemoteDescription(answer);
  console.log('Connection established!');

  // On button click, send a response.create event with a tool (function)
  btn.addEventListener('click', () => {
    // Define a function tool
    const event = {
      type: 'response.create',
      response: {
        modalities: ["text"], // just text for simplicity
        instructions: "Ask the model to call the `calculate_sum` function.",
        tools: [
          {
            type: "function",
            name: "calculate_sum",
            description: "Calculate the sum of two numbers",
            parameters: {
              type: "object",
              properties: {
                a: {type: "number"},
                b: {type: "number"}
              },
              required: ["a","b"]
            }
          }
        ],
        tool_choice: "auto",
        // Provide instructions so model calls the function
        input: [
          {type:"message", role:"user", content:[{type:"input_text", text:"What is 2+3?"}]}
        ]
      }
    };
    dc.send(JSON.stringify(event));
  });
})();
</script>
</body>
</html>
"#;

    Html(html)
}

async fn session(State(state): State<AppState>) -> impl IntoResponse {
    let ephem = match get_ephemeral_key(&state.openai_api_key).await {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error getting ephemeral key: {:?}", e);
            return Json(serde_json::json!({"error":"failed_to_get_key"}));
        }
    };
    Json(serde_json::to_value(ephem).unwrap())
}

async fn get_ephemeral_key(std_api_key: &str) -> Result<EphemeralKeyResponse> {
    let req_body = serde_json::json!({
        "model": "gpt-4o-realtime-preview-2024-12-17",
        "voice": "verse"
    });

    let client = Client::new();
    let res = client.post("https://api.openai.com/v1/realtime/sessions")
        .bearer_auth(std_api_key)
        .json(&req_body)
        .send()
        .await?
        .error_for_status()?;
    let ephem: EphemeralKeyResponse = res.json().await?;
    Ok(ephem)
}
