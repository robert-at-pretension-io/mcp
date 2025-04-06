use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::ai_client::{
    AIClient, AIRequestBuilder, Content, GenerationConfig, Message, 
    ModelCapabilities
};
use std::path::Path;
use reqwest::Client;
use std::time::Duration;

// OpenRouter API client implementation
pub struct OpenRouterClient {
    api_key: String,
    model_name: String,
    client: reqwest::Client,
}

// Request structure for the OpenRouter API (OpenAI-compatible)
#[derive(Serialize, Debug)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
}

// Message structure for the OpenRouter API (OpenAI-compatible)
#[derive(Serialize, Debug)]
struct ChatMessage {
    role: String,
    content: String,
}

// Response structure for the OpenRouter API
#[derive(Deserialize, Debug)]
struct ChatCompletionResponse {
    #[allow(dead_code)] // Or prefix with _ if preferred and adjust parsing if needed
    _id: String,
    #[allow(dead_code)]
    _object: String,
    #[allow(dead_code)]
    _created: u64,
    #[allow(dead_code)]
    _model: String,
    choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    #[allow(dead_code)]
    _index: u32,
    message: ResponseMessage,
    #[allow(dead_code)]
    _finish_reason: String,
}

#[derive(Deserialize, Debug)]
struct ResponseMessage {
    #[allow(dead_code)]
    _role: String,
    content: String,
}

impl OpenRouterClient {
    pub fn new(api_key: String, model_name: String) -> Result<Self> {
        info!("Creating OpenRouter client for model: {}", model_name);

        // Create HTTP client with timeout
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            api_key,
            model_name,
            client,
        })
    }

    // Convert our Message type to OpenRouter's ChatMessage type
    fn convert_message(message: &Message) -> ChatMessage {
        let role = match message.role {
            shared_protocol_objects::Role::System => "system",
            shared_protocol_objects::Role::User => "user",
            shared_protocol_objects::Role::Assistant => "assistant",
        }.to_string();

        let content = match &message.content {
            Content::Text(text) => text.clone(),
            Content::Image { path: _, alt_text } => {
                alt_text.clone().unwrap_or_else(|| "Image content not supported".to_string())
            },
            Content::ImageUrl { url: _, alt_text } => {
                alt_text.clone().unwrap_or_else(|| "Image URL content not supported".to_string())
            },
        };

        ChatMessage {
            role,
            content,
        }
    }
}

#[async_trait]
impl AIClient for OpenRouterClient {
    fn builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(OpenRouterRequestBuilder {
            api_key: self.api_key.clone(),
            model_name: self.model_name.clone(),
            client: self.client.clone(),
            messages: Vec::new(),
            config: None,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder> {
        self.builder()
    }

    fn model_name(&self) -> String {
        self.model_name.clone()
    }

    fn capabilities(&self) -> ModelCapabilities {
        // Capabilities will depend on the actual model being used
        // For now, provide conservative defaults
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

struct OpenRouterRequestBuilder {
    api_key: String,
    model_name: String,
    client: reqwest::Client,
    messages: Vec<Message>,
    config: Option<GenerationConfig>,
}

#[async_trait]
impl AIRequestBuilder for OpenRouterRequestBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push(Message {
            role: shared_protocol_objects::Role::System,
            content: Content::Text(content),
        });
        self
    }

    fn user(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push(Message {
            role: shared_protocol_objects::Role::User,
            content: Content::Text(content),
        });
        self
    }

    fn user_with_image(self: Box<Self>, text: String, _image_path: &Path) -> Result<Box<dyn AIRequestBuilder>> {
        // Basic implementation for now - just add text and ignore image
        let mut builder = *self;
        builder.messages.push(Message {
            role: shared_protocol_objects::Role::User,
            content: Content::Text(text),
        });
        Ok(Box::new(builder))
    }

    fn user_with_image_url(mut self: Box<Self>, text: String, _image_url: String) -> Box<dyn AIRequestBuilder> {
        // Basic implementation for now - just add text and ignore image URL
        self.messages.push(Message {
            role: shared_protocol_objects::Role::User,
            content: Content::Text(text),
        });
        self
    }

    fn assistant(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push(Message {
            role: shared_protocol_objects::Role::Assistant,
            content: Content::Text(content),
        });
        self
    }

    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder> {
        self.config = Some(config);
        self
    }

    async fn execute(self: Box<Self>) -> Result<String> {
        info!("Executing OpenRouter request for model: {}", self.model_name);

        // Convert our messages to OpenRouter's format
        let messages: Vec<ChatMessage> = self.messages.iter()
            .map(OpenRouterClient::convert_message)
            .collect();

        // Prepare the request
        let mut request = ChatCompletionRequest {
            model: self.model_name.clone(),
            messages,
            temperature: None,
            max_tokens: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
        };

        // Apply configuration if provided
        if let Some(config) = self.config {
            request.temperature = config.temperature;
            request.max_tokens = config.max_tokens;
            request.top_p = config.top_p;
            request.frequency_penalty = config.frequency_penalty;
            request.presence_penalty = config.presence_penalty;
        }

        // Send the request to OpenRouter API
        let response = self.client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://anthropic.com/claude/code")  // Optional: Site URL for OpenRouter stats
            .header("X-Title", "MCP Host")  // Optional: Name for OpenRouter stats
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send request to OpenRouter API: {}", e))?;

        // Handle error responses
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| "Could not read error response".to_string());
            return Err(anyhow!("OpenRouter API error ({}): {}", status, error_text));
        }

        // Parse the response
        let response_json: ChatCompletionResponse = response.json()
            .await
            .map_err(|e| anyhow!("Failed to parse OpenRouter API response: {}", e))?;

        // Extract the text from the first choice
        if let Some(choice) = response_json.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err(anyhow!("OpenRouter API returned empty response"))
        }
    }
}

pub fn create_openrouter_client(config: Value) -> Result<Box<dyn AIClient>> {
    let api_key = config["api_key"].as_str()
        .ok_or_else(|| anyhow!("OpenRouter API key not provided"))?;
    
    let model = config["model"].as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("mistralai/mistral-7b-instruct");  // Default model

    let client = OpenRouterClient::new(api_key.to_string(), model.to_string())?;
    Ok(Box::new(client))
}
