use anyhow::{Result, anyhow};
use async_trait::async_trait;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, ModelCapabilities};
use shared_protocol_objects::Role;
use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chat::{ChatMessage, ChatRole, MessageType};
use std::path::Path;
use log;

/// Client adapter for the rllm crate to interface with the MCP system
pub struct RLLMClient {
    model_name: String,
    backend: LLMBackend,
    // Store API key for recreating the client if needed
    api_key: String,
}

impl RLLMClient {
    /// Create a new RLLMClient with the given API key, model name, and backend
    pub fn new(api_key: String, model: String, backend: LLMBackend) -> Result<Self> {
        log::info!("Creating RLLMClient for backend: {:?}, model: {}", backend, model);
        
        // Validate parameters before building
        if model.is_empty() {
            return Err(anyhow!("Model name cannot be empty"));
        }
        
        // Check if API key is required but missing
        let key_required = matches!(
            backend,
            LLMBackend::OpenAI | LLMBackend::Anthropic | LLMBackend::Google | 
            LLMBackend::DeepSeek | LLMBackend::XAI | LLMBackend::Groq
        );
        
        if key_required && api_key.is_empty() {
            return Err(anyhow!("API key is required for {:?} backend", backend));
        }
        
        // Build with appropriate options
        let mut builder = LLMBuilder::new()
            .backend(backend.clone())
            .model(&model);

        // Only add API key if it's not empty (Ollama and some others don't need one)
        if !api_key.is_empty() {
            builder = builder.api_key(&api_key);  // Fixed: use reference to avoid moving api_key
        }
        
        // Add specific backend configurations
        match backend {
            LLMBackend::OpenAI => {
                // Configure OpenAI-specific settings
                if model.contains("gpt-4") && !model.contains("vision") && !model.contains("o") {
                    log::warn!("Using GPT-4 model without vision capability. For vision support, use gpt-4-vision or gpt-4o models.");
                }
            },
            LLMBackend::Ollama => {
                // For Ollama, we might want to check that the server is running
                // or validate that the model exists
                log::info!("Using Ollama backend with model {}. Ensure Ollama server is running and the model is pulled.", model);
                
                // Set Ollama API host if custom
                if let Ok(host) = std::env::var("OLLAMA_HOST") {
                    if !host.is_empty() {
                        log::debug!("Using custom Ollama host: {}", host);
                        log::warn!("Custom Ollama host {} defined, but host method not available in current rllm version", host);
                    }
                }
            },
            LLMBackend::Anthropic => {
                // Any Anthropic-specific configurations
                if !model.contains("claude") {
                    log::warn!("Model name '{}' doesn't contain 'claude'. Ensure this is a valid Anthropic model name.", model);
                }
            },
            _ => {
                // No specific configurations for other backends yet
            }
        }

        // Test if RLLM can build a client with these parameters
        // We won't store the client instance, just verify it can be created
        match builder.build() {
            Ok(_) => {
                // Success - we can create a client with these parameters
                Ok(Self {
                    model_name: model.clone(),
                    backend,
                    api_key,  // Move api_key into the struct
                })
            },
            Err(e) => {
                let error_msg = format!("Failed to build RLLM client for {:?} with model {}: {}", 
                                      backend, model, e);
                log::error!("{}", error_msg);
                
                // Add more context to common errors
                if e.to_string().contains("authentication") || e.to_string().contains("authorization") {
                    return Err(anyhow!("{} - Check your API key is valid and has sufficient permissions", error_msg));
                } else if e.to_string().contains("model") && e.to_string().contains("not found") {
                    return Err(anyhow!("{} - Verify the model name is correct and accessible with your account", error_msg));
                } else if e.to_string().contains("network") || e.to_string().contains("connection") {
                    return Err(anyhow!("{} - Check your network connection and ensure the API endpoint is accessible", error_msg));
                }
                
                return Err(anyhow!(error_msg));
            }
        }
    }

    /// Convert MCP Role to rllm's ChatRole
    fn convert_role(role: &Role) -> ChatRole {
        match role {
            Role::User => ChatRole::User,
            Role::Assistant => ChatRole::Assistant,
            // System role doesn't exist in this version, use User role as fallback
            Role::System => ChatRole::User,
        }
    }
}

// Implement Debug
impl std::fmt::Debug for RLLMClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RLLMClient")
         .field("model_name", &self.model_name)
         .field("backend", &self.backend)
         .field("api_key", &format!("{}****", &self.api_key.chars().take(4).collect::<String>()))
         .finish()
    }
}

#[async_trait]
impl AIClient for RLLMClient {
    fn model_name(&self) -> String {
        self.model_name.clone()
    }

    fn builder(&self) -> Box<dyn AIRequestBuilder> {
        // Create a new request builder with the client configuration
        Box::new(RLLMRequestBuilder {
            api_key: self.api_key.clone(),
            model_name: self.model_name.clone(),
            backend: self.backend.clone(),
            messages: Vec::new(),
            config: None, 
            system: None,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder> {
        self.builder()
    }
    
    fn capabilities(&self) -> ModelCapabilities {
        log::debug!("Getting capabilities for RLLM backend: {:?} with model {}", self.backend, self.model_name);
        
        // Model-specific capability overrides based on known model names
        if self.model_name.starts_with("gpt-4") || self.model_name.starts_with("gpt-4o") {
            if self.model_name.contains("vision") || self.model_name.contains("o") {
                // GPT-4 Vision or GPT-4o models
                return ModelCapabilities {
                    supports_images: true,
                    supports_system_messages: true,
                    supports_function_calling: true,
                    supports_vision: true,
                    max_tokens: Some(4096),
                    supports_json_mode: true,
                };
            }
        } else if self.model_name.starts_with("gpt-3.5") {
            // GPT-3.5 models generally don't support vision
            return ModelCapabilities {
                supports_images: false,
                supports_system_messages: true,
                supports_function_calling: true,
                supports_vision: false,
                max_tokens: Some(4096),
                supports_json_mode: true,
            };
        } else if self.model_name.contains("claude-3") {
            // Claude 3 models
            if self.model_name.contains("opus") {
                return ModelCapabilities {
                    supports_images: true,
                    supports_system_messages: true, 
                    supports_function_calling: true,
                    supports_vision: true,
                    max_tokens: Some(200000), // Claude 3 Opus has very high token limit
                    supports_json_mode: true,
                };
            } else if self.model_name.contains("sonnet") {
                return ModelCapabilities {
                    supports_images: true,
                    supports_system_messages: true,
                    supports_function_calling: true,
                    supports_vision: true,
                    max_tokens: Some(180000), // Claude 3 Sonnet has high token limit
                    supports_json_mode: true,
                };
            } else if self.model_name.contains("haiku") {
                return ModelCapabilities {
                    supports_images: true,
                    supports_system_messages: true,
                    supports_function_calling: true,
                    supports_vision: true,
                    max_tokens: Some(150000), // Claude 3 Haiku
                    supports_json_mode: true,
                };
            }
        } else if self.model_name.contains("gemini") {
            // Specific Gemini model capabilities
            if self.model_name.contains("pro") || self.model_name.contains("1.5") {
                return ModelCapabilities {
                    supports_images: true,
                    supports_system_messages: true,
                    supports_function_calling: true,
                    supports_vision: true,
                    max_tokens: Some(8192),
                    supports_json_mode: true,
                };
            } else if self.model_name.contains("flash") {
                return ModelCapabilities {
                    supports_images: false, // Flash models typically don't support vision
                    supports_system_messages: true,
                    supports_function_calling: true,
                    supports_vision: false,
                    max_tokens: Some(8192),
                    supports_json_mode: true,
                };
            }
        }
        
        // Default capabilities based on backend if no specific model match
        match self.backend {
            LLMBackend::OpenAI => ModelCapabilities {
                supports_images: true, // Most newer OpenAI models support images
                supports_system_messages: true,
                supports_function_calling: true,
                supports_vision: true, // Default to true for newer models
                max_tokens: Some(4096),
                supports_json_mode: true,
            },
            LLMBackend::Anthropic => ModelCapabilities {
                supports_images: true, // Claude 3 models support images
                supports_system_messages: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: Some(100000), // Claude models generally have high token limits
                supports_json_mode: true,
            },
            LLMBackend::Ollama => {
                // For Ollama, try to determine capabilities from model name
                let vision_capable = self.model_name.contains("llava") || 
                                    self.model_name.contains("bakllava") || 
                                    self.model_name.contains("vision");
                
                let function_calling = self.model_name.contains("Function") || 
                                      self.model_name.contains("tool") ||
                                      self.model_name.contains("llama-3");
                
                ModelCapabilities {
                    supports_images: vision_capable,
                    supports_system_messages: true, // Most Ollama models support system messages
                    supports_function_calling: function_calling,
                    supports_vision: vision_capable,
                    max_tokens: Some(2048), // Conservative default for Ollama
                    supports_json_mode: self.model_name.contains("coder") || 
                                       self.model_name.contains("wizard") ||
                                       self.model_name.contains("llama-3"),
                }
            },
            LLMBackend::DeepSeek => ModelCapabilities {
                supports_images: false,
                supports_system_messages: true,
                supports_function_calling: self.model_name.contains("coder"), // DeepSeek-Coder supports function calling
                supports_vision: false,
                max_tokens: Some(8192), // DeepSeek models have good context lengths
                supports_json_mode: true, // Especially good for DeepSeek-Coder
            },
            LLMBackend::XAI => ModelCapabilities {
                supports_images: true, // Grok-2 supports image input
                supports_system_messages: true,
                supports_function_calling: true, 
                supports_vision: true,
                max_tokens: Some(8192), // Grok has large context
                supports_json_mode: true,
            },
            LLMBackend::Phind => ModelCapabilities {
                supports_images: false,
                supports_system_messages: true,
                supports_function_calling: self.model_name.contains("34b"), // Latest Phind models support function calling
                supports_vision: false,
                max_tokens: Some(4096),
                supports_json_mode: true, // Good for code/technical content
            },
            LLMBackend::Groq => ModelCapabilities {
                supports_images: false,
                supports_system_messages: true,
                supports_function_calling: false, // Groq doesn't support function calling directly yet
                supports_vision: false,
                max_tokens: Some(4096),
                supports_json_mode: true,
            },
            LLMBackend::Google => ModelCapabilities {
                supports_images: true, // Gemini models generally support images
                supports_system_messages: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: Some(8192), // Gemini has high limits
                supports_json_mode: true,
            },
            _ => {
                // This is a catch-all case that shouldn't be reached, but is here for robustness
                log::warn!("Capabilities not defined for RLLM backend: {:?} with model {}. Using default capabilities.", 
                          self.backend, self.model_name);
                ModelCapabilities::default()
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RLLMRequestBuilder {
    // Store the configuration needed to create an RLLM client
    api_key: String,
    model_name: String,
    backend: LLMBackend,
    // Store messages and configuration
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
        
        // Verify image file exists first
        if !image_path.exists() {
            let error_msg = format!("Image file does not exist: {}", image_path.display());
            log::error!("{}", error_msg);
            return Err(anyhow!(error_msg));
        }
        
        // Check file size
        if let Ok(metadata) = std::fs::metadata(image_path) {
            let size_mb = metadata.len() as f64 / 1_048_576.0;
            if size_mb > 20.0 {
                log::warn!("Image file is {:.2} MB, which may be too large for some models (recommended < 20MB)", size_mb);
            }
        }
        
        // Log image format information
        if let Some(extension) = image_path.extension().and_then(|e| e.to_str()) {
            let format = extension.to_lowercase();
            if !["jpg", "jpeg", "png", "gif", "webp"].contains(&format.as_str()) {
                log::warn!("Image format '{}' might not be supported by all models. Recommended formats: JPG, PNG, WebP", format);
            }
        }
        
        // Store the image path in a special format that will be handled during execution
        let mut msgs = self.messages.clone();
        msgs.push((Role::User, text));
        msgs.push((Role::User, format!("__IMAGE_PATH__:{}", image_path.display())));
        
        let mut builder = (*self).clone();
        builder.messages = msgs;
        Ok(Box::new(builder))
    }

    fn user_with_image_url(self: Box<Self>, text: String, image_url: String) -> Box<dyn AIRequestBuilder> {
        log::debug!("Adding user message with image URL for RLLM request: {}", image_url);
        
        // Basic URL validation
        if !image_url.starts_with("http://") && !image_url.starts_with("https://") {
            log::error!("Invalid image URL format: {}", image_url);
            let mut msgs = self.messages.clone();
            msgs.push((Role::User, format!("{} [Invalid image URL format: {}]", text, image_url)));
            
            let mut builder = (*self).clone();
            builder.messages = msgs;
            return Box::new(builder);
        }
        
        // Check for supported URL patterns/file extensions
        let url_lower = image_url.to_lowercase();
        let supported_extensions = [".jpg", ".jpeg", ".png", ".gif", ".webp"];
        let has_supported_ext = supported_extensions.iter().any(|ext| url_lower.ends_with(ext));
        
        if !has_supported_ext && !url_lower.contains("?") {  // Skip check if URL has query params
            log::warn!("Image URL doesn't have a common image extension (.jpg, .png, etc). Some models may reject it.");
        }
        
        // Store messages with the image URL in a special format
        let mut msgs = self.messages.clone();
        msgs.push((Role::User, text));
        
        // Store the image URL in a special format
        msgs.push((Role::User, format!("__IMAGE_URL__:{}", image_url)));
        
        let mut builder = (*self).clone();
        builder.messages = msgs;
        Box::new(builder)
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
        log::info!("Executing RLLM request with model {}", self.model_name);
        
        // Create a new LLMBuilder with our stored configuration
        let mut builder = LLMBuilder::new()
            .backend(self.backend.clone())
            .model(&self.model_name)
            .api_key(&self.api_key);
        
        // Apply configuration options if provided
        if let Some(cfg) = &self.config {
            if let Some(temp) = cfg.temperature {
                builder = builder.temperature(temp);
            }
            
            if let Some(max_tokens) = cfg.max_tokens {
                builder = builder.max_tokens(max_tokens);
            }
            
            if let Some(top_p) = cfg.top_p {
                builder = builder.top_p(top_p);
            }
        }
        
        // System message is handled separately
        if let Some(sys) = &self.system {
            builder = builder.system(sys);
        }
        
        // Build the client
        let llm = match builder.build() {
            Ok(provider) => provider,
            Err(e) => {
                let error_msg = format!("Failed to build RLLM client: {}", e);
                log::error!("{}", error_msg);
                return Err(anyhow!(error_msg));
            }
        };
        
        // Build the chat messages
        let mut chat_messages = Vec::new();
        let mut _has_image = false;
        let mut image_text = String::new();
        
        for (i, (role, content)) in self.messages.iter().enumerate() {
            // Check for special image markers
            if content.starts_with("__IMAGE_PATH__:") || content.starts_with("__IMAGE_URL__:") {
                _has_image = true;
                
                // If previous message was the image text, skip adding it separately
                if i > 0 && self.messages[i-1].0 == Role::User && !image_text.is_empty() {
                    continue;
                }
                
                // Otherwise, add the image reference with the content
                let marker_type = if content.starts_with("__IMAGE_PATH__:") { "path" } else { "URL" };
                let path_or_url = if content.starts_with("__IMAGE_PATH__:") {
                    content.strip_prefix("__IMAGE_PATH__:").unwrap_or("")
                } else {
                    content.strip_prefix("__IMAGE_URL__:").unwrap_or("")
                };
                
                log::debug!("Adding image {} as text in message: {}", marker_type, path_or_url);
                
                // Add a user message that includes the image reference as text
                chat_messages.push(ChatMessage {
                    role: ChatRole::User,
                    content: format!("{}\n[Image {}: {}]", image_text, marker_type, path_or_url).into(),
                    message_type: MessageType::Text,
                });
                
                // Reset image text for next image
                image_text = String::new();
                continue;
            }
            
            // Handle regular messages
            if *role == Role::User && i+1 < self.messages.len() {
                // Check if this message is followed by an image marker
                let next_content = &self.messages[i+1].1;
                if next_content.starts_with("__IMAGE_PATH__:") || next_content.starts_with("__IMAGE_URL__:") {
                    // This text belongs with the image
                    image_text = content.clone();
                    continue;
                }
            }
            
            // Regular message (not part of an image)
            let chat_role = RLLMClient::convert_role(role);
            log::debug!("Adding regular message with role {:?}", chat_role);
            chat_messages.push(ChatMessage {
                role: chat_role,
                content: content.clone().into(),
                message_type: MessageType::Text,
            });
        }
        
        // Execute the chat request
        log::debug!("Sending chat request with {} messages", chat_messages.len());
        let start_time = std::time::Instant::now();
        
        // Chat with the LLM
        match llm.chat(&chat_messages).await {
            Ok(response) => {
                let elapsed = start_time.elapsed();
                // Convert the response to a string
                let response_str = response.to_string();
                log::info!(
                    "RLLM request completed in {:.2}s, received {} characters",
                    elapsed.as_secs_f64(), 
                    response_str.len()
                );
                Ok(response_str)
            },
            Err(e) => {
                let elapsed = start_time.elapsed();
                let error_msg = format!(
                    "RLLM chat request failed after {:.2}s: {}",
                    elapsed.as_secs_f64(),
                    e
                );
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
