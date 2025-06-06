use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use rmcp::model::Role;
use serde::{Deserialize, Serialize};
use serde_json::Value;
// Use the local Role definition from repl/mod.rs
use crate::ai_client::{
    AIClient, AIRequestBuilder, Content, GenerationConfig, Message, // Message struct uses local Role
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
    #[allow(dead_code)]
    #[serde(default)] // Make optional
    _id: String,
    #[allow(dead_code)]
    #[serde(default)] // Make optional
    _object: String,
    #[allow(dead_code)]
    #[serde(default)] // Make optional
    _created: u64,
    #[allow(dead_code)]
    #[serde(default)] // Make optional
    _model: String,
    choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    #[allow(dead_code)]
    #[serde(default)] // Make this field optional during deserialization
    _index: u32,
    message: ResponseMessage,
    #[allow(dead_code)]
    #[serde(default)] // Make this field optional during deserialization
    _finish_reason: String,
}

#[derive(Deserialize, Debug)]
struct ResponseMessage {
    #[allow(dead_code)]
    #[serde(default)] // Make this field optional during deserialization
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

    // Convert our Message type (which uses local Role) to OpenRouter's ChatMessage type
    fn convert_message(message: &Message) -> ChatMessage {
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
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
    fn builder(&self, system_prompt: &str) -> Box<dyn AIRequestBuilder> { // Add system_prompt back
        Box::new(OpenRouterRequestBuilder {
            api_key: self.api_key.clone(),
            model_name: self.model_name.clone(),
            client: self.client.clone(),
            system_prompt: system_prompt.to_string(), // Store system prompt
            messages: Vec::new(),
            config: None,
        })
    }

    fn raw_builder(&self, system_prompt: &str) -> Box<dyn AIRequestBuilder> { // Add system_prompt back
        self.builder(system_prompt) // Pass system prompt
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
    system_prompt: String, // Added missing field
    messages: Vec<Message>,
    config: Option<GenerationConfig>,
}

#[async_trait]
impl AIRequestBuilder for OpenRouterRequestBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push(Message {
            role: Role::User, // Use local Role enum
            content: Content::Text(content),
        });
        self
    }

    fn user(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push(Message {
            role: Role::User, // Use local Role enum
            content: Content::Text(content),
        });
        self
    }

    fn user_with_image(self: Box<Self>, text: String, _image_path: &Path) -> Result<Box<dyn AIRequestBuilder>> {
        // Basic implementation for now - just add text and ignore image
        let mut builder = *self;
        builder.messages.push(Message {
            role: Role::User, // Use local Role enum
            content: Content::Text(text),
        });
        Ok(Box::new(builder))
    }

    fn user_with_image_url(mut self: Box<Self>, text: String, _image_url: String) -> Box<dyn AIRequestBuilder> {
        // Basic implementation for now - just add text and ignore image URL
        self.messages.push(Message {
            role: Role::User, // Use local Role enum
            content: Content::Text(text),
        });
        self
    }

    fn assistant(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push(Message {
            role: Role::Assistant, // Use local Role enum
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

        // Prepare the final message list, injecting the system prompt
        let mut final_api_messages = Vec::new();
        let mut system_prompt_injected = false;

        for message in &self.messages {
            // Inject system prompt before the first user message
            if message.role == Role::User && !system_prompt_injected { // Use local Role enum
                 if !self.system_prompt.is_empty() {
                     log::debug!("Injecting system prompt before first user message for OpenRouter");
                     final_api_messages.push(ChatMessage {
                         role: "system".to_string(), // OpenRouter uses "system" role
                         content: self.system_prompt.clone(),
                     });
                 }
                 system_prompt_injected = true;
            }
            // Convert and add the current message
            final_api_messages.push(OpenRouterClient::convert_message(message));
        }

        // If system prompt wasn't injected (e.g., no user messages), add it at the beginning
        if !system_prompt_injected && !self.system_prompt.is_empty() {
             log::debug!("No user message found, injecting system prompt at the beginning for OpenRouter");
             final_api_messages.insert(0, ChatMessage {
                 role: "system".to_string(),
                 content: self.system_prompt.clone(),
             });
        }

        // Prepare the request using the final message list
        let mut request = ChatCompletionRequest {
            model: self.model_name.clone(),
            messages: final_api_messages, // Use the processed list
            temperature: None,
            max_tokens: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
        };

       // Apply configuration if provided, otherwise set default max_tokens
       if let Some(config) = &self.config {
           request.temperature = config.temperature;
           // Use configured max_tokens if present, otherwise default to 50000
           request.max_tokens = config.max_tokens.or(Some(50000));
           request.top_p = config.top_p;
           request.frequency_penalty = config.frequency_penalty;
           request.presence_penalty = config.presence_penalty;
       } else {
           // If no config provided at all, set default max_tokens
           request.max_tokens = Some(50000);
       }

        // Log the request body before sending (mask API key if it were included)
        match serde_json::to_string(&request) {
            Ok(body) => log::debug!("OpenRouter Request Body: {}", body),
            Err(e) => log::warn!("Failed to serialize OpenRouter request for logging: {}", e),
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

        // Try to parse the response as JSON, but log raw text on failure
        let response_text = response.text().await
            .map_err(|e| anyhow!("Failed to read OpenRouter response body: {}", e))?;

        match serde_json::from_str::<ChatCompletionResponse>(&response_text) {
            Ok(response_json) => {
                // Extract the text from the first choice
                if let Some(choice) = response_json.choices.first() {
                    Ok(choice.message.content.clone())
                } else {
                    Err(anyhow!("OpenRouter API returned empty response choices"))
                }
            }
            Err(e) => {
                // Log the raw response text that failed parsing
                log::error!("Failed to parse OpenRouter JSON response. Raw response: {}", response_text);
                Err(anyhow!("Failed to parse OpenRouter API response: {}. See logs for raw response.", e))
            }
        }
    }
}

pub fn create_openrouter_client(config: Value) -> Result<Box<dyn AIClient>> {
    let api_key = config["api_key"].as_str()
        .ok_or_else(|| anyhow!("OpenRouter API key not provided"))?;
    
    let model = config["model"].as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("openrouter/optimus-alpha");  // Default model

    let client = OpenRouterClient::new(api_key.to_string(), model.to_string())?;
    Ok(Box::new(client))
}
