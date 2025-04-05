use anyhow::{Result, Context};
use async_trait::async_trait;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, Content, ModelCapabilities};
use shared_protocol_objects::Role;
use serde_json::{json, Value};
use log::{debug, error, info};
use std::path::Path;
use reqwest;


#[derive(Debug, Clone)]
pub struct OpenAIClient {
    api_key: String,
    endpoint: String,
    speech_endpoint: String,
    transcription_endpoint: String,
}

impl OpenAIClient {
    pub fn new(api_key: String, _model: String) -> Self {
        info!("Creating new OpenAIClient with provided API key");
        // For chat completions, the standard endpoint is "https://api.openai.com/v1/chat/completions"
        // The 'model' field is provided in the JSON payload itself, not in the endpoint URL.
        OpenAIClient {
            api_key,
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            speech_endpoint: "https://api.openai.com/v1/audio/speech".to_string(),
            transcription_endpoint: "https://api.openai.com/v1/audio/transcriptions".to_string(),
        }
    }

    fn role_to_string(role: &Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    fn content_to_string(content: &Content) -> String {
        match content {
            Content::Text(s) => s.clone(),
            Content::Image { path: _, alt_text: _ } => "[Image content not supported]".to_string(),
            Content::ImageUrl { url: _, alt_text: _ } => "[Image URL not supported]".to_string(),
        }
    }
}

#[async_trait]
impl AIClient for OpenAIClient {
    fn model_name(&self) -> String {
        // Return the model name as a string. For example, "gpt-4o-mini"
        "gpt-4o-mini".to_string()
    }

    fn builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(OpenAICompletionBuilder {
            client: self.clone(),
            messages: Vec::new(),
            config: None,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(OpenAICompletionBuilder {
            client: self.clone(),
            messages: Vec::new(),
            config: None,
        })
    }
    
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            supports_images: false,
            supports_system_messages: true,
            supports_function_calling: true,
            supports_vision: false,
            max_tokens: Some(4096),
            supports_json_mode: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenAICompletionBuilder {
    client: OpenAIClient,
    messages: Vec<(Role, String)>,
    config: Option<GenerationConfig>,
}

#[async_trait]
impl AIRequestBuilder for OpenAICompletionBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::System, content));
        self
    }

    fn user(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::User, content));
        self
    }

    fn user_with_image(self: Box<Self>, text: String, _image_path: &Path) -> Result<Box<dyn AIRequestBuilder>> {
        // If needed, handle image. For now, just treat as text.
        let mut s = self;
        s.messages.push((Role::User, format!("{} [Image omitted]", text)));
        Ok(s)
    }

    fn user_with_image_url(self: Box<Self>, text: String, _image_url: String) -> Box<dyn AIRequestBuilder> {
        // If needed, handle image URL. For now, just treat as text.
        let mut s = self;
        s.messages.push((Role::User, format!("{} [Image URL omitted]", text)));
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

    async fn execute(self: Box<Self>) -> Result<String> {
        let model = "gpt-4o-mini"; // Hard-coded model name
        let mut payload_messages = Vec::new();
        for (role, content) in &self.messages {
            payload_messages.push(json!({
                "role": OpenAIClient::role_to_string(role),
                "content": content,
            }));
        }

        let mut payload = json!({
            "model": model,
            "messages": payload_messages,
            "response_format": { "type": "json_object" }
        });

        if let Some(cfg) = &self.config {
            if let Some(temp) = cfg.temperature {
                payload.as_object_mut().unwrap().insert("temperature".to_string(), json!(temp));
            }
            if let Some(max_tokens) = cfg.max_tokens {
                payload.as_object_mut().unwrap().insert("max_tokens".to_string(), json!(max_tokens));
            }
            if let Some(top_p) = cfg.top_p {
                payload.as_object_mut().unwrap().insert("top_p".to_string(), json!(top_p));
            }
            // frequency_penalty and presence_penalty not shown in snippet, but can be added if needed.
        }

        debug!("Sending request to OpenAI API");
        let client = reqwest::Client::new();
        let response = client
            .post(&self.client.endpoint)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.client.api_key))
            .json(&payload)
            .send()
            .await?;

        debug!("Response received, status: {}", response.status());
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.context("Failed to read error response")?;
            error!("API error response: {}", error_text);
            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_text));
        }

        let response_text = response.text().await?;
        debug!("Full API response: {}", response_text);

        let response_json: Value = serde_json::from_str(&response_text)
            .context("Failed to parse API response as JSON")?;

        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .context("Failed to extract text content from API response")?;

        Ok(content.to_string())
    }
}
