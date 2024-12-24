use crate::ai_client::StreamEvent;
use anyhow::{ Result, anyhow };
use futures::{ Stream, future };
use serde::Deserialize;
use serde_json::Value;
use std::pin::Pin;
use futures::stream::StreamExt;

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
    where S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static
{
    log::debug!("[SSE] Starting SSE stream parsing");

    Box::pin(
        stream.filter_map(move |line_result| {
            futures::future::ready(match line_result {
                Ok(bytes) => {
                    match String::from_utf8(bytes.to_vec()) {
                        Ok(line) => {
                            log::debug!("[SSE] Received raw line: {}", line);

                            // Handle event type lines
                            if line.starts_with("event: ") {
                                log::debug!(
                                    "[SSE] Received event type: {}",
                                    line.trim_start_matches("event: ")
                                );
                                None
                            } else if
                                // Handle data lines
                                line.starts_with("data: ")
                            {
                                let data = line.trim_start_matches("data: ");
                                log::debug!("[SSE] Parsing data: {}", data);

                                // Parse all messages uniformly

                                match serde_json::from_str::<StreamingMessage>(data) {
                                    Ok(msg) => {
                                        log::debug!("[SSE] Successfully parsed message: {:?}", msg);
                                        Some(parse_streaming_message(msg))
                                    }
                                    Err(e) => {
                                        log::error!("Failed to parse SSE message: {}", e);
                                        Some(Err(anyhow!("Failed to parse SSE message: {}", e)))
                                    }
                                }
                            } else {
                                // For other event types, still try to parse but log the type
                                log::debug!(
                                    "[SSE] Processing non-content-delta event: {}",
                                    current_event_type
                                );
                                match serde_json::from_str::<StreamingMessage>(data) {
                                    Ok(msg) => {
                                        log::debug!("[SSE] Successfully parsed message: {:?}", msg);
                                        Some(parse_streaming_message(msg))
                                    }
                                    Err(e) => {
                                        log::error!("Failed to parse SSE message: {}", e);
                                        Some(Err(anyhow!("Failed to parse SSE message: {}", e)))
                                    }
                                }
                            }
                        }
                        Err(e) => Some(Err(anyhow!("Invalid UTF-8 in SSE stream: {}", e))),
                    }
                }
                Err(e) => Some(Err(anyhow::anyhow!(e))),
            });
        })
    );
}

fn parse_streaming_message(msg: StreamingMessage) -> Result<StreamEvent> {
    match msg.message_type.as_str() {
        "message_start" => {
            let message_id = msg.message.ok_or_else(|| anyhow!("Missing message content"))?.id;
            Ok(StreamEvent::MessageStart { message_id })
        }
        "content_block_start" => {
            let index = msg.index.ok_or_else(|| anyhow!("Missing content block index"))?;
            Ok(StreamEvent::ContentBlockStart { index })
        }
        "content_block_delta" => {
            let index = msg.index.ok_or_else(|| anyhow!("Missing content delta index"))?;
            let text = msg.delta.and_then(|d| d.text).unwrap_or_default();
            Ok(StreamEvent::ContentDelta { index, text })
        }
        "content_block_stop" => {
            let index = msg.index.ok_or_else(|| anyhow!("Missing content block stop index"))?;
            Ok(StreamEvent::ContentBlockStop { index })
        }
        "message_delta" => {
            let delta = msg.delta.unwrap_or_else(|| DeltaContent {
                delta_type: None,
                text: None,
                stop_reason: None,
                usage: None,
            });
            Ok(StreamEvent::MessageDelta {
                stop_reason: delta.stop_reason,
                usage: delta.usage,
            })
        }
        "message_stop" => Ok(StreamEvent::MessageStop),
        "error" => {
            let error = msg.error.ok_or_else(|| anyhow!("Missing error content"))?;
            Ok(StreamEvent::Error {
                error_type: error.error_type,
                message: error.message,
            })
        }
        _ => Err(anyhow!("Unknown message type: {}", msg.message_type)),
    }
}
