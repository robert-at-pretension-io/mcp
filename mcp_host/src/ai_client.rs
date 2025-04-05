use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use shared_protocol_objects::Role;

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

use crate::rllm_adapter; // Add import for the new adapter

impl AIClientFactory {
    pub fn create(provider: &str, config: Value) -> Result<Box<dyn AIClient>> {
        match provider {
            "gemini" => {
                // Keep existing Gemini implementation for now, or switch to RLLM if desired
                let api_key = config["api_key"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Gemini API key not provided"))?;
                // Assuming gemini-1.5-pro is a valid model identifier for the direct client
                let model = config["model"].as_str().unwrap_or("gemini-1.5-pro"); 
                let client = crate::gemini::GeminiClient::new(api_key.to_string(), model.to_string());
                Ok(Box::new(client))
            }
            "anthropic" => {
                let api_key = config["api_key"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Anthropic API key not provided"))?;
                let model = config["model"].as_str().unwrap_or("claude-3-haiku-20240307"); // Use a default model known to rllm

                // Use RLLM if the feature is enabled
                #[cfg(feature = "use_rllm")]
                {
                    log::info!("Using RLLM adapter for Anthropic provider");
                    use rllm::builder::LLMBackend;
                    let client = rllm_adapter::RLLMClient::new(api_key.to_string(), model.to_string(), LLMBackend::Anthropic)?;
                    return Ok(Box::new(client));
                }

                // Fallback to original implementation if feature is not enabled
                #[cfg(not(feature = "use_rllm"))]
                {
                    log::info!("Using direct Anthropic client implementation");
                    let client = crate::anthropic::AnthropicClient::new(api_key.to_string(), model.to_string());
                    Ok(Box::new(client))
                }
            }
            "openai" => {
                let api_key = config["api_key"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("OpenAI API key not provided"))?;
                let model = config["model"].as_str().unwrap_or("gpt-4o-mini"); // Keep existing default

                // Use RLLM if the feature is enabled
                #[cfg(feature = "use_rllm")]
                {
                     log::info!("Using RLLM adapter for OpenAI provider");
                    use rllm::builder::LLMBackend;
                    let client = rllm_adapter::RLLMClient::new(api_key.to_string(), model.to_string(), LLMBackend::OpenAI)?;
                    return Ok(Box::new(client));
                }

                // Fallback to original implementation if feature is not enabled
                #[cfg(not(feature = "use_rllm"))]
                {
                    log::info!("Using direct OpenAI client implementation");
                    let client = crate::openai::OpenAIClient::new(api_key.to_string(), model.to_string());
                    Ok(Box::new(client))
                }
            }
             "ollama" => {
                 // Ollama integration requires the RLLM feature
                 #[cfg(feature = "use_rllm")]
                 {
                     log::info!("Using RLLM adapter for Ollama provider");
                     // Ollama endpoint can be configured, default to localhost
                     let _endpoint = config["endpoint"].as_str().unwrap_or("http://localhost:11434");
                     let model = config["model"].as_str().unwrap_or("llama3"); // Default Ollama model

                     use rllm::builder::LLMBackend;
                     // Ollama doesn't typically require an API key, pass an empty string
                     let client = rllm_adapter::RLLMClient::new("".to_string(), model.to_string(), LLMBackend::Ollama)?;
                     return Ok(Box::new(client));
                 }

                 // If RLLM feature is not enabled, Ollama is not supported
                 #[cfg(not(feature = "use_rllm"))]
                 {
                     log::error!("Ollama provider requires the 'use_rllm' feature to be enabled during compilation.");
                     return Err(anyhow::anyhow!("Ollama support requires the 'use_rllm' feature flag"));
                 }
             }
            _ => Err(anyhow::anyhow!("Unknown or unsupported AI provider: {}", provider))
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


#[cfg(test)]
#[cfg(feature = "use_rllm")]
mod tests {
    use super::*;
    use crate::rllm_adapter::RLLMClient;
    use rllm::builder::LLMBackend;
    use shared_protocol_objects::Role;
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
        let client_result = RLLMClient::new(
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
    async fn test_rllm_anthropic_client_creation_and_capabilities() -> Result<()> {
        setup_test_logging();
        // Test creating an RLLMClient for Anthropic
         let client_result = RLLMClient::new(
            "test-anthropic-key".to_string(), // Fake key
            "claude-3-haiku-20240307".to_string(),
            LLMBackend::Anthropic
        );

        assert!(client_result.is_ok(), "Failed to create RLLMClient for Anthropic");
        let client = client_result.unwrap();

        assert_eq!(client.model_name(), "claude-3-haiku-20240307");
        assert_eq!(client.backend, LLMBackend::Anthropic);

        let caps = client.capabilities();
        assert!(caps.supports_system_messages, "Anthropic should support system messages");
        assert!(caps.supports_function_calling, "Anthropic should support function calling");
         assert!(caps.supports_vision, "Anthropic (Claude 3) should support vision");

        Ok(())
    }

     #[tokio::test]
    async fn test_rllm_ollama_client_creation_and_capabilities() -> Result<()> {
         setup_test_logging();
        // Test creating an RLLMClient for Ollama (no API key needed)
         let client_result = RLLMClient::new(
            "".to_string(), // Empty API key for Ollama
            "llama3".to_string(),
            LLMBackend::Ollama
        );

        assert!(client_result.is_ok(), "Failed to create RLLMClient for Ollama");
         let client = client_result.unwrap();

        assert_eq!(client.model_name(), "llama3");
        assert_eq!(client.backend, LLMBackend::Ollama);

        let caps = client.capabilities();
         assert!(caps.supports_system_messages, "Ollama should support system messages");
         // Default capabilities for Ollama might be false for vision/functions
         assert!(!caps.supports_function_calling, "Ollama default caps assume no function calling");
         assert!(!caps.supports_vision, "Ollama default caps assume no vision support");


        Ok(())
    }

    // Note: The execute test requires a running LLM service (like Ollama) or valid API keys.
    // This test structure assumes you might run Ollama locally for testing.
    // It's marked ignore by default to avoid failing CI if Ollama isn't running.
    #[tokio::test]
    #[ignore] 
    async fn test_rllm_ollama_execute() -> Result<()> {
         setup_test_logging();
         // Ensure Ollama is running locally with llama3 model pulled: `ollama run llama3`
         let client_result = RLLMClient::new(
            "".to_string(),
            "llama3".to_string(), // Use a model you have pulled in Ollama
            LLMBackend::Ollama
        );

        assert!(client_result.is_ok(), "Failed to create RLLMClient for Ollama");
        let client = client_result.unwrap();

        // Test builder pattern and execution
        let builder = client.builder()
            .system("You are a test assistant. Respond concisely.".to_string())
            .user("Say 'hello'".to_string());

        let response = builder.execute().await;

        assert!(response.is_ok(), "RLLM execution failed: {:?}", response.err());
        let response_text = response.unwrap();
        println!("Ollama Response: {}", response_text);
        assert!(!response_text.is_empty(), "Expected a non-empty response from Ollama");

        Ok(())
    }
}
