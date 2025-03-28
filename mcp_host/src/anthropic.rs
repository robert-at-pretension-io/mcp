use anyhow::{Result, Context};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, StreamResult};
use crate::streaming::parse_sse_stream;

use shared_protocol_objects::Role;

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    api_key: String,
    model: String,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

#[async_trait]
impl AIClient for AnthropicClient {
    fn model_name(&self) -> String {
        self.model.clone()
    }

    fn builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(AnthropicCompletionBuilder {
            client: self.clone(),
            messages: Vec::new(),
            config: None,
            stream: false,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder> {
        self.builder()
    }
}

#[derive(Debug, Clone)]
pub struct AnthropicCompletionBuilder {
    client: AnthropicClient,
    messages: Vec<(Role, String)>,
    config: Option<GenerationConfig>,
    stream: bool,
}

#[async_trait]
impl AIRequestBuilder for AnthropicCompletionBuilder {
    fn streaming(mut self: Box<Self>, enabled: bool) -> Box<dyn AIRequestBuilder> {
        self.stream = enabled;
        self
    }

    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::System, content));
        self
    }

    fn user(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::User, content));
        self
    }

    fn user_with_image(self: Box<Self>, text: String, _image_path: &std::path::Path) -> Result<Box<dyn AIRequestBuilder>> {
        let mut s = self;
        s.messages.push((Role::User, text));
        Ok(s)
    }

    fn user_with_image_url(self: Box<Self>, text: String, _image_url: String) -> Box<dyn AIRequestBuilder> {
        let mut s = self;
        s.messages.push((Role::User, text));
        s
    }

    fn assistant(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::Assistant, content));
        self
    }

    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder> {
        self.config = Some(config);
        self
    }

    async fn execute_streaming(self: Box<Self>) -> Result<StreamResult> {
        log::debug!("[Anthropic] Starting streaming execution");
        
        // Extract system message if present
        let (system_message, other_messages): (Vec<_>, Vec<_>) = self.messages.iter()
            .partition(|(role, _)| matches!(role, Role::System));
        
        log::debug!("[Anthropic] System messages: {:?}", system_message);
        log::debug!("[Anthropic] Other messages: {:?}", other_messages);

        // Build the payload
        let mut payload = json!({
            "model": self.client.model,
            "messages": other_messages.iter().map(|(role, content)| {
                json!({
                    "role": match role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        _ => unreachable!()
                    },
                    "content": content
                })
            }).collect::<Vec<_>>(),
            "stream": true,
            "max_tokens": 1024
        });

        // Add system message if present
        if let Some((_, system_content)) = system_message.first() {
            payload.as_object_mut().unwrap()
                .insert("system".to_string(), json!(system_content));
        }

        if let Some(cfg) = &self.config {
            if let Some(max_tokens) = cfg.max_tokens {
                payload.as_object_mut().unwrap().insert("max_tokens".to_string(), json!(max_tokens));
            }
        }

        log::debug!("[Anthropic] Sending streaming request with payload: {:?}", payload);
        
        let client = Client::new();
        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.client.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("accept", "text/event-stream")
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        log::debug!("[Anthropic] Got response with status: {}", response.status());

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow::anyhow!("Anthropic API error: {}", error));
        }

        let stream = response.bytes_stream();
        Ok(Box::pin(parse_sse_stream(stream)))
    }

    async fn execute(self: Box<Self>) -> Result<String> {
        // Extract system message if present
        let (system_message, other_messages): (Vec<_>, Vec<_>) = self.messages.iter()
            .partition(|(role, _)| matches!(role, Role::System));

        // Build the payload
        let mut payload = json!({
            "model": self.client.model,
            "messages": other_messages.iter().map(|(role, content)| {
                json!({
                    "role": match role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        _ => unreachable!()
                    },
                    "content": content
                })
            }).collect::<Vec<_>>(),
            "stream": self.stream,
            "max_tokens": 1024
        });

        // Add system message if present
        if let Some((_, system_content)) = system_message.first() {
            payload.as_object_mut().unwrap()
                .insert("system".to_string(), json!(system_content));
        }

        if let Some(cfg) = &self.config {
            if let Some(max_tokens) = cfg.max_tokens {
                payload.as_object_mut().unwrap().insert("max_tokens".to_string(), json!(max_tokens));
            }
        }

        let client = Client::new();
        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.client.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow::anyhow!("Anthropic API error: {}", error));
        }

        let response_json = response.json::<serde_json::Value>().await?;
        let content = response_json["content"][0]["text"]
            .as_str()
            .context("Failed to get response text")?;

        Ok(content.to_string())
    }
}
