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

pub fn parse_sse_stream<S>(stream: S) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static
{
    Box::pin(stream.flat_map(|chunk_result| {
        let mut out = Vec::new();
        match chunk_result {
            Ok(chunk) => {
                // Convert raw bytes to string
                match String::from_utf8(chunk.to_vec()) {
                    Ok(chunk_str) => {
                        // Split by newlines; each line might be `event: ...` or `data: ...`
                        for line in chunk_str.lines() {
                            // We only parse lines starting with `data: `
                            if let Some(data) = line.strip_prefix("data: ") {
                                let data = data.trim();
                                // Ignore empty data lines
                                if data.is_empty() {
                                    continue;
                                }
                                match serde_json::from_str::<StreamingMessage>(data) {
                                    Ok(msg) => {
                                        if let Err(e) = handle_sse_message(&msg, &mut out) {
                                            out.push(Err(e));
                                        }
                                    }
                                    Err(e) => {
                                        out.push(Err(anyhow!(
                                            "Failed to parse SSE JSON: {} (line was: '{}')",
                                            e, line
                                        )));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        out.push(Err(anyhow!("Invalid UTF-8 in SSE stream: {}", e)));
                    }
                }
            }
            Err(e) => {
                out.push(Err(anyhow!(e)));
            }
        }
        futures::stream::iter(out)
    }))
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
