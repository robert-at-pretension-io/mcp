use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use anyhow::{Result, Context};
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

    let openai_api_key = env::var("OPENAI_API_KEY")
        .context("OPENAI_API_KEY environment variable not set. Please set this environment variable.")?;
        
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

    let html = include_str!("index.html");

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
