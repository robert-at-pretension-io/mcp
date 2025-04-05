use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use shared_protocol_objects::Role;
use rllm::builder::LLMBackend;
use crate::rllm_adapter;
// Using crate-relative path for the adapter

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
#[async_trait]
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
    
    /// Execute the request and get response as a single string
    async fn execute(self: Box<Self>) -> Result<String>;
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
    
    /// Get the model's capabilities
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
}

/// Capabilities of an AI model
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    pub supports_images: bool,
    pub supports_system_messages: bool,
    pub supports_function_calling: bool,
    pub supports_vision: bool,
    pub max_tokens: Option<u32>,
    pub supports_json_mode: bool,
}

/// Factory for creating AI clients
pub struct AIClientFactory;

impl AIClientFactory {
    pub fn create(provider: &str, config: Value) -> Result<Box<dyn AIClient>> {
        // Use the factory function from rllm_adapter
        rllm_adapter::create_rllm_client_for_provider(provider, config)
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


#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    // Helper to initialize logging for tests
    fn setup_test_logging() {
        // Use try_init to avoid panic if logger is already set
        let _ = env_logger::builder().is_test(true).try_init();
    }


    #[tokio::test]
    async fn test_rllm_openai_client_creation_and_capabilities() -> Result<()> {
        setup_test_logging();
        // Test creating an RLLMClient for OpenAI
        let client_result = crate::rllm_adapter::RLLMClient::new(
            "test-openai-key".to_string(), // Fake key for structure testing
            "gpt-4o-mini".to_string(),
            LLMBackend::OpenAI
        );

        assert!(client_result.is_ok(), "Failed to create RLLMClient for OpenAI");
        let client = client_result.unwrap();

        // Verify basic properties
        assert_eq!(client.model_name(), "gpt-4o-mini");
        assert_eq!(client.backend, LLMBackend::OpenAI);

        // Verify capabilities reported for OpenAI
        let caps = client.capabilities();
        assert!(caps.supports_system_messages, "OpenAI should support system messages");
        assert!(caps.supports_function_calling, "OpenAI should support function calling");
        assert!(caps.supports_json_mode, "OpenAI should support JSON mode");
        assert!(caps.supports_vision, "OpenAI (gpt-4o) should support vision");

        Ok(())
    }

    #[tokio::test]
    async fn test_factory_creates_rllm_client() -> Result<()> {
        setup_test_logging();
        
        // Test creating OpenAI client via factory
        let openai_config = serde_json::json!({
            "api_key": "test-openai-key",
            "model": "gpt-4o-mini"
        });
        
        let client = AIClientFactory::create("openai", openai_config)?;
        assert_eq!(client.model_name(), "gpt-4o-mini");
        assert!(client.capabilities().supports_function_calling);
        
        // Test creating Anthropic client via factory
        let anthropic_config = serde_json::json!({
            "api_key": "test-anthropic-key",
            "model": "claude-3-haiku-20240307"
        });
        
        let client = AIClientFactory::create("anthropic", anthropic_config)?;
        assert_eq!(client.model_name(), "claude-3-haiku-20240307");
        assert!(client.capabilities().supports_system_messages);
        
        Ok(())
    }
}
