use crate::ai_client::StreamEvent;
use anyhow::{ Result, anyhow };
use futures::{ Stream, StreamExt };
use serde::Deserialize;
use serde_json::Value;
use std::pin::Pin;

#[derive(Debug, Deserialize)]
struct StreamingMessage {
    #[serde(rename = "type")]
    message_type: String,
    message: Option<MessageContent>,
    delta: Option<DeltaContent>,
    index: Option<usize>,
    error: Option<ErrorContent>,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DeltaContent {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    stop_reason: Option<String>,
    usage: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ErrorContent {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

use std::sync::Arc;
use tokio::sync::Mutex;

pub fn parse_sse_stream<S>(stream: S) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static
{
    // Buffer for incomplete messages
    let buffer = Arc::new(Mutex::new(String::new()));
    
    Box::pin(stream.then(move |chunk_result| {
        let buffer = buffer.clone();
        async move {
            let mut results = Vec::new();
            match chunk_result {
                Ok(chunk) => {
                    match String::from_utf8(chunk.to_vec()) {
                        Ok(chunk_str) => {
                            let mut buffer = buffer.lock().await;
                            buffer.push_str(&chunk_str);

                            // Process complete messages
                            while let Some((message, remaining)) = extract_complete_message(&buffer) {
                                *buffer = remaining;
                                
                                if let Some(data) = message.strip_prefix("data: ") {
                                    let data = data.trim();
                                    if !data.is_empty() {
                                        match serde_json::from_str::<StreamingMessage>(data) {
                                            Ok(msg) => {
                                                if let Err(e) = handle_sse_message(&msg, &mut results) {
                                                    results.push(Err(e));
                                                }
                                            }
                                            Err(e) => {
                                                results.push(Err(anyhow!(
                                                    "Failed to parse SSE JSON: {} (data was: '{}')",
                                                    e, data
                                                )));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            results.push(Err(anyhow!("Invalid UTF-8 in SSE stream: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    results.push(Err(anyhow!(e)));
                }
            }
            futures::stream::iter(results)
        }
    }).flatten_stream())
}

fn extract_complete_message(buffer: &str) -> Option<(String, String)> {
    if let Some(newline_pos) = buffer.find('\n') {
        let (message, rest) = buffer.split_at(newline_pos + 1);
        Some((message.trim().to_string(), rest.to_string()))
    } else {
        None
    }
}

fn handle_sse_message(msg: &StreamingMessage, out: &mut Vec<Result<StreamEvent>>) -> Result<()> {
    match msg.message_type.as_str() {
        "message_start" => {
            let message_id = msg
                .message
                .as_ref()
                .ok_or_else(|| anyhow!("Missing message in 'message_start'"))?
                .id
                .clone();
            out.push(Ok(StreamEvent::MessageStart { message_id }));
        }
        "content_block_start" => {
            let index = msg.index.ok_or_else(|| anyhow!("Missing index"))?;
            out.push(Ok(StreamEvent::ContentBlockStart { index }));
        }
        "content_block_delta" => {
            let index = msg.index.ok_or_else(|| anyhow!("Missing index"))?;
            let text = msg.delta.as_ref().and_then(|d| d.text.clone()).unwrap_or_default();
            out.push(Ok(StreamEvent::ContentDelta { index, text }));
        }
        "content_block_stop" => {
            let index = msg.index.ok_or_else(|| anyhow!("Missing index"))?;
            out.push(Ok(StreamEvent::ContentBlockStop { index }));
        }
        "message_delta" => {
            let delta = msg.delta.as_ref().ok_or_else(|| anyhow!("Missing delta"))?;
            out.push(Ok(StreamEvent::MessageDelta {
                stop_reason: delta.stop_reason.clone(),
                usage: delta.usage.clone(),
            }));
        }
        "message_stop" => {
            out.push(Ok(StreamEvent::MessageStop));
        }
        "error" => {
            let err = msg.error.as_ref().ok_or_else(|| anyhow!("Missing error content"))?;
            out.push(Ok(StreamEvent::Error {
                error_type: err.error_type.clone(),
                message: err.message.clone(),
            }));
        }
        // Safely ignore unknown message types
        other => {
            out.push(Err(anyhow!("Unknown SSE message type: {}", other)));
        }
    }
    Ok(())
}
