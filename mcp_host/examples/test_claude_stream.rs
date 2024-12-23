use anyhow::Result;
use futures::StreamExt;
use reqwest::Client;
use serde_json::json;
use std::env;
use crate::streaming::parse_sse_stream;
use crate::ai_client::StreamEvent;

use mcp_host as crate;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();
    let api_key = env::var("ANTHROPIC_API_KEY")?;
    let client = Client::new();

    // Create the request
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [{"role": "user", "content": "Tell me a short story about a robot learning to paint"}],
            "max_tokens": 1024,
            "stream": true
        }))
        .send()
        .await?;

    // Convert response to event stream
    let event_stream = response.bytes_stream();
    let mut stream = parse_sse_stream(event_stream);

    // Process the stream
    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => match event {
                StreamEvent::ContentDelta { text, .. } => {
                    print!("{}", text);
                    std::io::Write::flush(&mut std::io::stdout())?;
                }
                StreamEvent::Error { error_type, message } => {
                    eprintln!("Error {}: {}", error_type, message);
                    break;
                }
                StreamEvent::MessageStop => {
                    println!("\n[Message Complete]");
                    break;
                }
                _ => {} // Handle other events as needed
            },
            Err(e) => {
                eprintln!("Stream error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
