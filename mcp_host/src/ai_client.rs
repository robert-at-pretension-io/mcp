use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

/// Role of a message in a conversation
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Content types that can be sent to AI models
#[derive(Debug, Clone)]
pub enum Content {
    Text(String),
    Image { path: String, alt_text: Option<String> },
    ImageUrl { url: String, alt_text: Option<String> },
}

/// A message in a conversation
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Content,
}

/// Configuration for AI model generation
#[derive(Debug, Clone, Default)]
pub struct GenerationConfig {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
}

/// Builder for constructing AI requests
pub trait AIRequestBuilder: Send {
    /// Add a system message
    fn system(self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder>;
    
    /// Add a user message
    fn user(self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder>;
    
    /// Add a user message with an image
    fn user_with_image(self: Box<Self>, text: String, image_path: &Path) -> Result<Box<dyn AIRequestBuilder>>;
    
    /// Add a user message with an image URL
    fn user_with_image_url(self: Box<Self>, text: String, image_url: String) -> Box<dyn AIRequestBuilder>;
    
    /// Add an assistant message
    fn assistant(self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder>;
    
    /// Set generation parameters
    fn config(self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder>;
    
    /// Get the executor for this builder
    fn executor(self: Box<Self>) -> Box<dyn AIRequestExecutor>;
}

#[async_trait]
pub trait AIRequestExecutor: Send {
    /// Execute the request and get response
    async fn execute(&self) -> Result<String>;
}

/// Core trait for AI model implementations
#[async_trait]
pub trait AIClient: Send + Sync {
    /// Create a new request builder
    fn builder(&self) -> Box<dyn AIRequestBuilder>;
    
    /// Create a raw request builder without schema validation
    fn raw_builder(&self) -> Box<dyn AIRequestBuilder>;
    
    /// Get the model's name/identifier
    fn model_name(&self) -> String;
}

/// Capabilities of an AI model
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    pub supports_images: bool,
    pub supports_system_messages: bool,
    pub supports_function_calling: bool,
    pub supports_streaming: bool,
    pub max_tokens: Option<u32>,
    pub supports_json_mode: bool,
}

/// Factory for creating AI clients
pub struct AIClientFactory;

impl AIClientFactory {
    pub fn create(provider: &str, config: Value) -> Result<Box<dyn AIClient>> {
        match provider {
            "openai" => {
                let api_key = config["api_key"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("OpenAI API key not provided"))?;
                let client = crate::openai::OpenAIClient::new(api_key.to_string());
                Ok(Box::new(client))
            }
            "gemini" => {
                let api_key = config["api_key"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Gemini API key not provided"))?;
                let client = crate::gemini::GeminiClient::new(api_key.to_string());
                Ok(Box::new(client))
            }
            _ => Err(anyhow::anyhow!("Unknown AI provider: {}", provider))
        }
    }
}

/// Helper function to format messages for models that don't support all roles
pub fn format_message_for_basic_model(role: &Role, content: &str) -> String {
    match role {
        Role::System => format!("System: {}", content),
        Role::User => content.to_string(),
        Role::Assistant => format!("Assistant: {}", content),
    }
}
