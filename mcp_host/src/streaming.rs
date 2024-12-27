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
    log::debug!("[SSE] Creating new SSE stream parser");
    
    // Buffer for incomplete messages
    let buffer = Arc::new(Mutex::new(String::new()));
    
    Box::pin(stream.then(move |chunk_result| {
        let buffer = buffer.clone();
        async move {
            let mut results = Vec::new();
            match chunk_result {
                Ok(chunk) => {
                    // log::debug!("[SSE] Received raw chunk of {} bytes", chunk.len());
                    
                    match String::from_utf8(chunk.to_vec()) {
                        Ok(chunk_str) => {
                            // log::debug!("[SSE] Decoded chunk: {}", chunk_str);
                            let mut buffer = buffer.lock().await;
                            buffer.push_str(&chunk_str);
                            // log::debug!("[SSE] Current buffer size: {} bytes", buffer.len());

                            // Process complete messages
                            while let Some((message, remaining)) = extract_complete_message(&buffer) {
                                // log::debug!("[SSE] Extracted complete message: {}", message);
                                *buffer = remaining;
                                // log::debug!("[SSE] Remaining buffer size: {} bytes", buffer.len());
                                
                                if let Some(data) = message.strip_prefix("data: ") {
                                    let data = data.trim();
                                    if !data.is_empty() {
                                        // log::debug!("[SSE] Processing data payload: {}", data);
                                        
                                        match serde_json::from_str::<StreamingMessage>(data) {
                                            Ok(msg) => {
                                                log::debug!("[SSE] Parsed streaming message: {:?}", msg);
                                                if let Err(e) = handle_sse_message(&msg, &mut results) {
                                                    log::error!("[SSE] Error handling message: {}", e);
                                                    results.push(Err(e));
                                                }
                                            }
                                            Err(e) => {
                                                log::error!("[SSE] JSON parse error: {} (data was: '{}')", e, data);
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
                            log::error!("[SSE] UTF-8 decode error: {}", e);
                            results.push(Err(anyhow!("Invalid UTF-8 in SSE stream: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    log::error!("[SSE] Stream error: {}", e);
                    results.push(Err(anyhow!(e)));
                }
            }
            log::debug!("[SSE] Returning {} results: {:?}", results.len(), results);
            futures::stream::iter(results)
        }
    }).flatten())
}

fn extract_complete_message(buffer: &str) -> Option<(String, String)> {
    if let Some(newline_pos) = buffer.find('\n') {
        let (message, rest) = buffer.split_at(newline_pos + 1);
        log::debug!("[SSE] Extracted message of {} bytes, {} bytes remaining", 
            message.len(), rest.len());
        log::debug!("[SSE] Message content: {}", message);
        Some((message.trim().to_string(), rest.to_string()))
    } else {
        log::debug!("[SSE] No complete message found in buffer of {} bytes", buffer.len());
        if !buffer.is_empty() {
            log::debug!("[SSE] Buffer content: {}", buffer);
        }
        None
    }
}

fn handle_sse_message(msg: &StreamingMessage, out: &mut Vec<Result<StreamEvent>>) -> Result<()> {
    log::debug!("[SSE] Handling message type: {}", msg.message_type);
    
    match msg.message_type.as_str() {
        "message_start" => {
            let message_id = msg
                .message
                .as_ref()
                .ok_or_else(|| {
                    log::error!("[SSE] Missing message in 'message_start'");
                    anyhow!("Missing message in 'message_start'")
                })?
                .id
                .clone();
            log::debug!("[SSE] Starting new message with ID: {}", message_id);
            out.push(Ok(StreamEvent::MessageStart { message_id }));
        }
        "content_block_start" => {
            let index = msg.index.ok_or_else(|| {
                log::error!("[SSE] Missing index in 'content_block_start'");
                anyhow!("Missing index")
            })?;
            log::debug!("[SSE] Starting content block {}", index);
            out.push(Ok(StreamEvent::ContentBlockStart { index }));
        }
        "content_block_delta" => {
            let index = msg.index.ok_or_else(|| {
                log::error!("[SSE] Missing index in 'content_block_delta'");
                anyhow!("Missing index")
            })?;
            let text = msg.delta.as_ref().and_then(|d| d.text.clone()).unwrap_or_default();
            log::debug!("[SSE] Content delta for block {}: {:?}", index, text);
            out.push(Ok(StreamEvent::ContentDelta { index, text }));
        }
        "content_block_stop" => {
            let index = msg.index.ok_or_else(|| {
                log::error!("[SSE] Missing index in 'content_block_stop'");
                anyhow!("Missing index")
            })?;
            log::debug!("[SSE] Stopping content block {}", index);
            out.push(Ok(StreamEvent::ContentBlockStop { index }));
        }
        "message_delta" => {
            let delta = msg.delta.as_ref().ok_or_else(|| {
                log::error!("[SSE] Missing delta in 'message_delta'");
                anyhow!("Missing delta")
            })?;
            log::debug!("[SSE] Message delta - stop reason: {:?}, usage: {:?}", 
                delta.stop_reason, delta.usage);
            out.push(Ok(StreamEvent::MessageDelta {
                stop_reason: delta.stop_reason.clone(),
                usage: delta.usage.clone(),
            }));
        }
        "message_stop" => {
            log::debug!("[SSE] Message complete");
            out.push(Ok(StreamEvent::MessageStop));
        }
        "error" => {
            let err = msg.error.as_ref().ok_or_else(|| {
                log::error!("[SSE] Missing error content in 'error' message");
                anyhow!("Missing error content")
            })?;
            log::error!("[SSE] Error event - type: {}, message: {}", 
                err.error_type, err.message);
            out.push(Ok(StreamEvent::Error {
                error_type: err.error_type.clone(),
                message: err.message.clone(),
            }));
        }
        other => {
            log::debug!("[SSE] Ignoring unknown message type: {}", other);
        }
    }
    Ok(())
}
