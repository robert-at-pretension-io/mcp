use anyhow::{Result, anyhow};
use async_trait::async_trait;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, ModelCapabilities};
use shared_protocol_objects::Role;
use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chat::{ChatMessage, ChatRole, MessageType};
use rllm::Llm;
use std::path::Path;
use log;

/// Client adapter for the rllm crate to interface with the MCP system
pub struct RLLMClient {
    llm: Llm,
    model_name: String,
    backend: LLMBackend,
}

impl RLLMClient {
    /// Create a new RLLMClient with the given API key, model name, and backend
    pub fn new(api_key: String, model: String, backend: LLMBackend) -> Result<Self> {
        log::info!("Creating RLLMClient for backend: {:?}, model: {}", backend, model);
        let mut builder = LLMBuilder::new()
            .backend(backend.clone())
            .model(&model);

        // Only add API key if it's not empty (Ollama doesn't need one)
        if !api_key.is_empty() {
            builder = builder.api_key(api_key);
        }

        let llm = builder.build()
            .map_err(|e| anyhow!("Failed to build RLLM client: {}", e))?;

        Ok(Self {
            llm,
            model_name: model,
            backend,
        })
    }

    /// Convert MCP Role to rllm's ChatRole
    fn convert_role(role: &Role) -> ChatRole {
        match role {
            Role::User => ChatRole::User,
            Role::Assistant => ChatRole::Assistant,
            Role::System => ChatRole::System, // rllm supports system messages in chat directly
        }
    }
}

// Implement Debug manually as rllm::Llm doesn't derive Debug
impl std::fmt::Debug for RLLMClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RLLMClient")
         .field("model_name", &self.model_name)
         .field("backend", &self.backend)
         // Do not include llm field as it's not Debug
         .finish()
    }
}

#[async_trait]
impl AIClient for RLLMClient {
    fn model_name(&self) -> String {
        self.model_name.clone()
    }

    fn builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(RLLMRequestBuilder {
            client: self.llm.clone(),
            messages: Vec::new(),
            config: None,
            system: None,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder> {
        self.builder()
    }
    
    fn capabilities(&self) -> ModelCapabilities {
        log::debug!("Getting capabilities for RLLM backend: {:?}", self.backend);
        
        // Set capabilities based on the backend
        match self.backend {
            LLMBackend::OpenAI => ModelCapabilities {
                supports_images: true, // OpenAI models like gpt-4 support images
                supports_system_messages: true,
                supports_function_calling: true, // OpenAI supports function calling
                supports_vision: true, // OpenAI supports vision
                max_tokens: Some(4096), // Example, adjust per specific model if needed
                supports_json_mode: true, // OpenAI supports JSON mode
            },
            LLMBackend::Anthropic => ModelCapabilities {
                supports_images: true, // Claude 3 models support images
                supports_system_messages: true, // Anthropic supports system prompts
                supports_function_calling: true, // Claude 3 supports tool use
                supports_vision: true, // Claude 3 supports vision
                max_tokens: Some(4096), // Example, adjust per specific model
                supports_json_mode: true, // Claude 3 supports JSON mode
            },
            LLMBackend::Ollama => ModelCapabilities {
                supports_images: false, // Ollama support varies by model, default false
                supports_system_messages: true,
                supports_function_calling: false, // Generally not supported directly via Ollama API
                supports_vision: false, // Varies by model, default false
                max_tokens: Some(2048), // Common default, adjust as needed
                supports_json_mode: false, // Varies by model, default false
            },
            LLMBackend::DeepSeek => ModelCapabilities {
                supports_images: false, // DeepSeek doesn't generally support image input
                supports_system_messages: true,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: Some(4096),
                supports_json_mode: true, // For DeepSeek-Coder it supports good JSON
            },
            LLMBackend::XAI => ModelCapabilities {
                supports_images: true, // Grok-2 supports image input
                supports_system_messages: true,
                supports_function_calling: true, // Grok supports function calling 
                supports_vision: true,
                max_tokens: Some(4096),
                supports_json_mode: true,
            },
            LLMBackend::Phind => ModelCapabilities {
                supports_images: false, // Phind doesn't support image input
                supports_system_messages: true,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: Some(4096),
                supports_json_mode: true, // Good for code/technical content
            },
            LLMBackend::Groq => ModelCapabilities {
                supports_images: false, // Groq doesn't support image input currently
                supports_system_messages: true,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: Some(4096),
                supports_json_mode: true,
            },
            LLMBackend::Google => ModelCapabilities {
                supports_images: true, // Gemini supports images
                supports_system_messages: true,
                supports_function_calling: true, // Gemini supports function calling
                supports_vision: true,
                max_tokens: Some(8192), // Gemini has high limits
                supports_json_mode: true,
            },
            _ => {
                log::warn!("Capabilities not defined for RLLM backend: {:?}. Using default.", self.backend);
                ModelCapabilities::default()
            }
        }
    }
}

#[derive(Debug)]
struct RLLMRequestBuilder {
    client: Llm,
    messages: Vec<(Role, String)>,
    config: Option<GenerationConfig>,
    system: Option<String>,
}

#[async_trait]
impl AIRequestBuilder for RLLMRequestBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        log::debug!("Setting system message for RLLM request");
        self.system = Some(content);
        self
    }

    fn user(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        log::debug!("Adding user message for RLLM request");
        self.messages.push((Role::User, content));
        self
    }

    fn user_with_image(self: Box<Self>, text: String, image_path: &Path) -> Result<Box<dyn AIRequestBuilder>> {
        log::debug!("Adding user message with image for RLLM request: {}", image_path.display());
        
        // Check if the model supports images
        if !self.client.vision_supported() {
            log::warn!("The selected model does not support images. The image will be ignored.");
            return Ok(Box::new(RLLMRequestBuilder {
                client: self.client,
                messages: {
                    let mut msgs = self.messages;
                    msgs.push((Role::User, format!("{} [Image described at {}]", text, image_path.display())));
                    msgs
                },
                config: self.config,
                system: self.system,
            }));
        }
        
        // Attempt to load the image for models that support it
        match self.client.add_image(image_path.to_string_lossy().to_string()) {
            Ok(client_with_image) => {
                log::debug!("Successfully added image to RLLM client");
                Ok(Box::new(RLLMRequestBuilder {
                    client: client_with_image,
                    messages: {
                        let mut msgs = self.messages;
                        msgs.push((Role::User, text));
                        msgs
                    },
                    config: self.config,
                    system: self.system,
                }))
            },
            Err(e) => {
                log::warn!("Failed to add image to RLLM client: {}", e);
                // Fall back to text-only
                Ok(Box::new(RLLMRequestBuilder {
                    client: self.client,
                    messages: {
                        let mut msgs = self.messages;
                        msgs.push((Role::User, format!("{} [Image failed to load from {}]", text, image_path.display())));
                        msgs
                    },
                    config: self.config,
                    system: self.system,
                }))
            }
        }
    }

    fn user_with_image_url(self: Box<Self>, text: String, image_url: String) -> Box<dyn AIRequestBuilder> {
        log::debug!("Adding user message with image URL for RLLM request: {}", image_url);
        
        // Check if the model supports images
        if !self.client.vision_supported() {
            log::warn!("The selected model does not support images. The image URL will be ignored.");
            return Box::new(RLLMRequestBuilder {
                client: self.client,
                messages: {
                    let mut msgs = self.messages;
                    msgs.push((Role::User, format!("{} [Image from URL: {}]", text, image_url)));
                    msgs
                },
                config: self.config,
                system: self.system,
            });
        }
        
        // Attempt to add the image URL for models that support it
        match self.client.add_image_url(image_url.clone()) {
            Ok(client_with_image) => {
                log::debug!("Successfully added image URL to RLLM client");
                Box::new(RLLMRequestBuilder {
                    client: client_with_image,
                    messages: {
                        let mut msgs = self.messages;
                        msgs.push((Role::User, text));
                        msgs
                    },
                    config: self.config,
                    system: self.system,
                })
            },
            Err(e) => {
                log::warn!("Failed to add image URL to RLLM client: {}", e);
                // Fall back to text-only
                Box::new(RLLMRequestBuilder {
                    client: self.client,
                    messages: {
                        let mut msgs = self.messages;
                        msgs.push((Role::User, format!("{} [Image URL failed to load: {}]", text, image_url)));
                        msgs
                    },
                    config: self.config,
                    system: self.system,
                })
            }
        }
    }

    fn assistant(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        log::debug!("Adding assistant message for RLLM request");
        self.messages.push((Role::Assistant, content));
        self
    }

    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder> {
        log::debug!("Applying generation config for RLLM request: {:?}", config);
        self.config = Some(config);
        self
    }

    async fn execute(self: Box<Self>) -> Result<String> {
        log::info!("Executing RLLM request with backend: {:?}", self.client.backend());
        let mut client = self.client.clone();

        // Apply system message if provided
        if let Some(system_content) = &self.system {
            log::debug!("Applying system message: {}", system_content);
            client = client.system(system_content);
        }

        // Convert messages to rllm ChatMessage format
        let mut rllm_messages = Vec::new();
        for (role, content) in &self.messages {
            let chat_role = RLLMClient::convert_role(role);
            rllm_messages.push(ChatMessage {
                role: chat_role,
                content: content.clone().into(),
                message_type: MessageType::Text,
            });
        }

        // Apply configuration if provided
        if let Some(cfg) = &self.config {
            // Common parameters
            if let Some(temp) = cfg.temperature {
                log::debug!("Setting temperature: {}", temp);
                client = client.temperature(temp);
            }
            
            if let Some(max_tokens) = cfg.max_tokens {
                log::debug!("Setting max_tokens: {}", max_tokens);
                client = client.max_tokens(max_tokens as usize);
            }
            
            if let Some(top_p) = cfg.top_p {
                log::debug!("Setting top_p: {}", top_p);
                client = client.top_p(top_p);
            }
            
            // OpenAI-specific parameters
            if matches!(client.backend(), LLMBackend::OpenAI) {
                if let Some(freq_penalty) = cfg.frequency_penalty {
                    log::debug!("Setting frequency_penalty: {}", freq_penalty);
                    client = client.frequency_penalty(freq_penalty);
                }
                
                if let Some(pres_penalty) = cfg.presence_penalty {
                    log::debug!("Setting presence_penalty: {}", pres_penalty);
                    client = client.presence_penalty(pres_penalty);
                }
            }
            
            // Models that support JSON mode
            if self.client.json_supported() {
                client = client.json(true);
                log::debug!("Enabled JSON mode for supported model");
            }
        }
        
        // Execute the request and handle errors
        log::debug!("Sending chat request to RLLM backend with {} messages", rllm_messages.len());
        match client.chat(&rllm_messages) {
            Ok(result) => {
                log::info!("RLLM request successful, received response of {} characters", result.len());
                Ok(result)
            },
            Err(e) => {
                let error_msg = format!("RLLM chat execution failed: {}", e);
                log::error!("{}", error_msg);
                Err(anyhow!(error_msg))
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
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

    // Test for DeepSeek capabilities
    #[tokio::test]
    async fn test_rllm_deepseek_client_creation_and_capabilities() -> Result<()> {
        setup_test_logging();
        let client_result = RLLMClient::new(
            "test-deepseek-key".to_string(),
            "deepseek-chat".to_string(),
            LLMBackend::DeepSeek
        );

        assert!(client_result.is_ok(), "Failed to create RLLMClient for DeepSeek");
        let client = client_result.unwrap();

        assert_eq!(client.model_name(), "deepseek-chat");
        assert_eq!(client.backend, LLMBackend::DeepSeek);

        let caps = client.capabilities();
        assert!(caps.supports_system_messages, "DeepSeek should support system messages");
        assert!(!caps.supports_vision, "DeepSeek should not support vision");
        assert!(caps.supports_json_mode, "DeepSeek should support JSON mode");

        Ok(())
    }

    // Test for role conversion
    #[test]
    fn test_convert_role() {
        // Test all role types
        assert_eq!(RLLMClient::convert_role(&Role::User), ChatRole::User);
        assert_eq!(RLLMClient::convert_role(&Role::Assistant), ChatRole::Assistant);
        assert_eq!(RLLMClient::convert_role(&Role::System), ChatRole::System);
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
