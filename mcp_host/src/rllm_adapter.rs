use anyhow::{Result, anyhow};
use async_trait::async_trait;
use rmcp::model::Role;
// Import LLMError for detailed error matching
use rllm::error::LLMError;
use tracing::info;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, ModelCapabilities};
use serde_json::Value;
// Use the local Role definition from repl/mod.rs
use rllm::builder::{LLMBackend, LLMBuilder};
// Import necessary types from rllm::chat
use rllm::chat::{ChatMessage, ChatRole, MessageType}; // Removed ChatContent, ContentPart
use std::path::Path;
use log;
use regex::Regex;
use once_cell::sync::Lazy; // For static regex compilation

/// Client adapter for the rllm crate to interface with the MCP system
pub struct RLLMClient {
    model_name: String,
    backend: LLMBackend,
    // Store API key for recreating the client if needed
    api_key: String,
}

impl RLLMClient {
    /// Create a new RLLMClient with the given API key, model name, and backend, optionally specifying a base URL.
    pub fn new_with_base_url(api_key: String, model: String, backend: LLMBackend, base_url: Option<String>) -> Result<Self> {
        log::info!("Creating RLLMClient for backend: {:?}, model: {}, base_url: {:?}", backend, model, base_url);

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

        // Add base URL if provided
        if let Some(url) = &base_url {
            builder = builder.base_url(url);
        }

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

    // Removed unused convert_role function
}

// Static Regex for extracting text from GoogleChatResponse string format
// Looks for `text: "` followed by the captured group `(.*?)` until the next `"`
static GOOGLE_TEXT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"text:\s*"(.*?)""#).expect("Invalid Google Text Regex")
});


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

/// Create an RLLM client for the given provider
pub fn create_rllm_client_for_provider(provider: &str, config: Value) -> Result<Box<dyn AIClient>> {
    // Match against lowercase provider name for consistency
    match provider.to_lowercase().as_str() {
        "google" | "gemini" => { // Accept both "google" and "gemini"
            log::info!("Using RLLM adapter for Google Gemini provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Google Gemini API key not provided (GEMINI_API_KEY)"))?;
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'google/gemini'"))?;
            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::Google, None)?;
            Ok(Box::new(client))
        }
        "anthropic" => {
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Anthropic API key not provided"))?;
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                 .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'anthropic'"))?;

            log::info!("Using RLLM adapter for Anthropic provider");
            // Explicitly set the base URL for Anthropic
            let client = RLLMClient::new_with_base_url(
                api_key.to_string(),
                model.to_string(),
                LLMBackend::Anthropic,
                Some("https://api.anthropic.com/v1".to_string()) // Standard Anthropic base URL
            )?;
            Ok(Box::new(client))
        }
        "openai" => {
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("OpenAI API key not provided"))?;
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                 .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'openai'"))?;

            log::info!("Using RLLM adapter for OpenAI provider");
            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::OpenAI, None)?;
            Ok(Box::new(client))
        }
        "ollama" => {
            log::info!("Using RLLM adapter for Ollama provider");
            // Ollama endpoint can be configured, default to localhost
            let base_url = config["endpoint"].as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()); // Store as Option<String>
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                 .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'ollama'"))?;

            // Ollama doesn't typically require an API key, pass an empty string
            // Use new_with_base_url, passing the optional base_url
            let client = RLLMClient::new_with_base_url("".to_string(), model.to_string(), LLMBackend::Ollama, base_url)?;
            Ok(Box::new(client))
        }
        "deepseek" => {
            log::info!("Using RLLM adapter for DeepSeek provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("DeepSeek API key not provided"))?;
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                 .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'deepseek'"))?;

            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::DeepSeek, None)?;
            Ok(Box::new(client))
        }
        "xai" => {
            log::info!("Using RLLM adapter for XAI/Grok provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("XAI API key not provided"))?;
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                 .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'xai'"))?;

            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::XAI, None)?;
            Ok(Box::new(client))
        }
        "phind" => {
            log::info!("Using RLLM adapter for Phind provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Phind API key not provided"))?;
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                 .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'phind'"))?;

            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::Phind, None)?;
            Ok(Box::new(client))
        }
        "groq" => {
            log::info!("Using RLLM adapter for Groq provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Groq API key not provided"))?;
            // Model name MUST be provided by the caller now
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                 .ok_or_else(|| anyhow!("Model name missing or empty in config for provider 'groq'"))?;

            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::Groq, None)?;
            Ok(Box::new(client))
        }
        _ => Err(anyhow!("Unknown or unsupported AI provider: {}", provider))
    }
}

#[async_trait]
impl AIClient for RLLMClient {
    fn model_name(&self) -> String {
        self.model_name.clone()
    }

    fn builder(&self, system_prompt: &str) -> Box<dyn AIRequestBuilder> { // Add system_prompt back
        // Create a new request builder with the client configuration and system prompt
        Box::new(RLLMRequestBuilder {
            api_key: self.api_key.clone(),
            model_name: self.model_name.clone(),
            backend: self.backend.clone(),
            system_prompt: system_prompt.to_string(), // Store system prompt
            messages: Vec::new(),
            config: None,
            // system field removed
        })
    }

    fn raw_builder(&self, system_prompt: &str) -> Box<dyn AIRequestBuilder> { // Add system_prompt back
        self.builder(system_prompt) // Pass system prompt
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
            // Removed unreachable _ pattern as all variants are covered
            // LLMBackend::Other(_) => { ... } // If Other variant exists, handle it
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
    system_prompt: String, // Renamed from 'system'
}

#[async_trait] // Ensure async_trait is applied to the impl block
impl AIRequestBuilder for RLLMRequestBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        log::debug!("Setting system message for RLLM request");
        // Assign to the correct field 'system_prompt'
        self.system_prompt = content;
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
        
        // TODO: Implement proper image path handling. This requires reading the file,
        // potentially base64 encoding it, checking model capabilities, and using
        // MessageType::Image. For now, store as a special text message.
        let mut msgs = self.messages.clone();
        msgs.push((Role::User, text)); // Add the text part
        // Add the image path as a separate placeholder message
        msgs.push((Role::User, format!("__IMAGE_PATH__:{}", image_path.display())));

        let mut builder = (*self).clone();
        builder.messages = msgs; // Update messages in the cloned builder
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
        
        // TODO: Implement proper image URL handling. This requires checking model
        // capabilities and potentially using MessageType::Image(url).
        // For now, store as a special text message.
        let mut msgs = self.messages.clone(); // Keep only this declaration
        msgs.push((Role::User, text)); // Add the text part
        // Add the image URL as a separate placeholder message
        msgs.push((Role::User, format!("__IMAGE_URL__:{}", image_url)));

        let mut builder = (*self).clone();
        builder.messages = msgs; // Update messages in the cloned builder
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
           // Use configured max_tokens if present, otherwise default to 50000
           builder = builder.max_tokens(cfg.max_tokens.unwrap_or(50000));
           } else {
               // If no config provided at all, set default max_tokens
               builder = builder.max_tokens(50000);
           }

           if let Some(top_p) = cfg.top_p {
               builder = builder.top_p(top_p);
           }
       } else {
            // If no config provided at all, set default max_tokens
            builder = builder.max_tokens(50000);
       }

        // --- Apply System Prompt using Builder ---
        if !self.system_prompt.is_empty() {
            log::debug!("Applying system prompt via LLMBuilder: '{}...'", self.system_prompt.chars().take(50).collect::<String>());
            builder = builder.system(&self.system_prompt);
        } else {
            log::debug!("No system prompt to apply via LLMBuilder.");
        }
        // --- End System Prompt ---

        // Build the client
        let llm = match builder.build() {
            Ok(provider) => provider,
            Err(e) => {
                let error_msg = format!("Failed to build RLLM client: {}", e);
                log::error!("{}", error_msg);
                return Err(anyhow!(error_msg));
            }
        };

        // Build the chat messages for the rllm library
        let mut chat_messages = Vec::new();
        let mut _has_image = false; // Keep track if images are involved

        // --- System Prompt is handled by the builder now ---

        // --- Process User/Assistant Messages ---
        // Skip the first message if a system prompt was set via the builder
        let messages_to_add = if !self.system_prompt.is_empty() && !self.messages.is_empty() {
            // Check if the first message content actually matches the system prompt
            // This is a safety check in case the state management changes
            if self.messages[0].1 == self.system_prompt {
                 log::debug!("Skipping first message in adapter loop as it matches the system prompt.");
                 self.messages.iter().skip(1)
            } else {
                 log::warn!("System prompt set, but first message content doesn't match. Adding all messages.");
                 self.messages.iter().skip(0) // Add all if mismatch
            }
        } else {
            self.messages.iter().skip(0) // Add all messages if no system prompt was set
        };

        for (role, content) in messages_to_add {
            let (rllm_role, message_type) = match role {
                Role::User => {
                    // Determine message type based on content prefix
                    if content.starts_with("__IMAGE_PATH__:") {
                        // TODO: Implement proper image path handling (read file, base64 encode, use MessageType::Image)
                        log::warn!("Image path handling not fully implemented. Sending as text.");
                        (ChatRole::User, MessageType::Text)
                    } else if content.starts_with("__IMAGE_URL__:") {
                        // TODO: Check model capabilities and use MessageType::Image(url) if supported.
                        log::warn!("Image URL handling not fully implemented. Sending as text.");
                        (ChatRole::User, MessageType::Text)
                    } else {
                        (ChatRole::User, MessageType::Text)
                    }
                }
                Role::Assistant => (ChatRole::Assistant, MessageType::Text),
                // System role is handled by the builder
            };

            log::debug!("Adding message: Role={:?}, Type={:?}, Content='{}...'",
                       rllm_role, message_type, content.chars().take(50).collect::<String>());

            chat_messages.push(ChatMessage {
                role: rllm_role,
                content: content.clone(), // Content is now just a String
                message_type, // Add the required message_type field
                // name: None, // Removed: Field not found in rllm::chat::ChatMessage
            });
        }

        // --- Log the final messages being sent (using rllm Debug) ---
        log::debug!("Final RLLM ChatMessages Payload ({} messages):", chat_messages.len());
        for msg in &chat_messages {
            let content_preview = msg.content.lines().next().unwrap_or("").chars().take(100).collect::<String>();
            log::debug!("  - Role: {:?}, Type: {:?}, Content Preview: '{}...'", msg.role, msg.message_type, content_preview);
        }
        // --- End Logging ---


        // Execute the chat request
        let message_count = chat_messages.len();
        log::debug!("Sending chat request with {} messages to RLLM backend", message_count);
        // Add warning for large message counts
        if message_count > 20 { // Threshold can be adjusted
            log::warn!("Sending a large number of messages ({}) to RLLM backend {:?}. This might exceed token limits or increase costs.", message_count, self.backend);
        }
        let start_time = std::time::Instant::now();

        // Chat with the LLM
        match llm.chat(&chat_messages).await {
            Ok(response_box) => {
                let elapsed = start_time.elapsed();
                // Convert the Box<dyn ChatResponse> to a String using Display trait
                info!("time elapsed: {:.2}s", elapsed.as_secs_f64());
                Ok(response_box.text().unwrap())

                // Removed old extraction logic:
                // choices.get(0)
                //     .and_then(|choice| choice.message.content.clone())
                //     .unwrap_or_else(|| {
                //         log::warn!("Could not extract primary text from RLLM response, falling back to full string representation.");
                //         response.to_string() // Fallback to the full string representation if extraction fails
                //     });

            },
            Err(e) => {
                let elapsed = start_time.elapsed();
                // Log more detailed error information
                log::error!("RLLM chat request failed after {:.2}s.", elapsed.as_secs_f64());
                log::error!("Underlying RLLM Error (Debug): {:?}", e); // Log Debug representation
                log::error!("Underlying RLLM Error (Display): {}", e); // Log Display representation

                // Extract more details if it's an HttpError
                let detailed_error_msg = if let LLMError::HttpError(http_err_str) = &e {
                    // The HttpError variant often contains the response body or more specific details
                    format!("HTTP Error Details: {}", http_err_str)
                } else {
                    // For other error types, just use the standard display format
                    format!("Error: {}", e)
                };

                log::error!("Formatted Error for Reporting: {}", detailed_error_msg);

                // Construct the final error message for anyhow, including the detailed info if available
                let final_error_msg = format!(
                    "RLLM chat request failed after {:.2}s: {}",
                    elapsed.as_secs_f64(),
                    detailed_error_msg // Use the potentially more detailed message
                );

                Err(anyhow!(final_error_msg)) // Return the final error
            }
        }
    }
}

