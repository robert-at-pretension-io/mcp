use anyhow::{Result, anyhow};
use async_trait::async_trait;
use rmcp::model::Role;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, ModelCapabilities};
use serde_json::Value;
// Use the local Role definition from repl/mod.rs
use rllm::builder::{LLMBackend, LLMBuilder};
// Import ContentPart and ChatContent if they exist, or adjust based on rllm's actual API
// Assuming ChatMessage content field takes something convertible from Vec<ContentPart>
use rllm::chat::{ChatMessage, ChatRole, MessageType, ChatContent, ContentPart};
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

    /// Convert MCP Role to rllm's ChatRole
    fn convert_role(role: &Role) -> ChatRole {
        match role {
            Role::User => ChatRole::User,
            Role::Assistant => ChatRole::Assistant
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

/// Create an RLLM client for the given provider
pub fn create_rllm_client_for_provider(provider: &str, config: Value) -> Result<Box<dyn AIClient>> {
    // Match against lowercase provider name for consistency
    match provider.to_lowercase().as_str() {
        "google" | "gemini" => { // Accept both "google" and "gemini"
            log::info!("Using RLLM adapter for Google Gemini provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Google Gemini API key not provided (GEMINI_API_KEY)"))?;
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty()) // Ensure model is not empty string
                .unwrap_or("gemini-1.5-flash"); // Default Gemini model
            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::Google, None)?;
            Ok(Box::new(client))
        }
        "anthropic" => {
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Anthropic API key not provided"))?;
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("claude-3-haiku-20240307"); // Default Anthropic model

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
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("gpt-4o-mini"); // Default OpenAI model

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
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("llama3"); // Default Ollama model

            // Ollama doesn't typically require an API key, pass an empty string
            // Use new_with_base_url, passing the optional base_url
            let client = RLLMClient::new_with_base_url("".to_string(), model.to_string(), LLMBackend::Ollama, base_url)?;
            Ok(Box::new(client))
        }
        "deepseek" => {
            log::info!("Using RLLM adapter for DeepSeek provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("DeepSeek API key not provided"))?;
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("deepseek-chat"); // Default DeepSeek model

            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::DeepSeek, None)?;
            Ok(Box::new(client))
        }
        "xai" => {
            log::info!("Using RLLM adapter for XAI/Grok provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("XAI API key not provided"))?;
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("grok-1"); // Default XAI model

            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::XAI, None)?;
            Ok(Box::new(client))
        }
        "phind" => {
            log::info!("Using RLLM adapter for Phind provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Phind API key not provided"))?;
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("Phind-70B"); // Default Phind model

            // Use new_with_base_url, passing None for base_url
            let client = RLLMClient::new_with_base_url(api_key.to_string(), model.to_string(), LLMBackend::Phind, None)?;
            Ok(Box::new(client))
        }
        "groq" => {
            log::info!("Using RLLM adapter for Groq provider");
            let api_key = config["api_key"].as_str()
                .ok_or_else(|| anyhow!("Groq API key not provided"))?;
            // Use provider default if model is missing or empty
            let model = config["model"].as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("llama3-8b-8192"); // Default Groq model

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

        // --- System Prompt is handled by the builder now, removed manual injection ---

        // --- Process User/Assistant Messages ---
        // Use the appropriate type for content parts, assuming ContentPart enum exists
        let mut current_user_message_parts: Vec<ContentPart> = Vec::new();

        for (role, content) in self.messages.iter() {
            match role {
                Role::User => {
                    // Handle potential multi-part user messages (text + image)
                    if content.starts_with("__IMAGE_PATH__:") {
                        _has_image = true;
                        let path_str = content.strip_prefix("__IMAGE_PATH__:").unwrap_or("");
                        log::warn!("Image path handling not fully implemented in rllm_adapter. Sending path as text for now: {}", path_str);
                        // TODO: Implement proper image path handling (read file, base64 encode)
                        // This requires checking model capabilities and using rllm's MessageType::ImageBytes
                        // For now, append as text part to potentially combine with previous text
                        current_user_message_parts.push(format!("[Image Path: {}]", path_str).into());

                    } else if content.starts_with("__IMAGE_URL__:") {
                         _has_image = true;
                         let url_str = content.strip_prefix("__IMAGE_URL__:").unwrap_or("");
                         log::debug!("Adding image URL part: {}", url_str);
                         // TODO: Check model capabilities for vision support before adding image URL.
                         // Assuming rllm handles the structure for OpenAI vision via ContentPart::ImageUrl
                         current_user_message_parts.push(ContentPart::ImageUrl { url: url_str.to_string() });

                    } else {
                        // Regular text part
                        log::debug!("Adding user text part: '{}...'", content.chars().take(50).collect::<String>());
                        // Assume ContentPart::Text exists
                        current_user_message_parts.push(ContentPart::Text(content.clone()));
                    }
                }
                Role::Assistant => {
                    // If we have pending user message parts, finalize and add them first
                    // If we have pending user message parts, finalize and add them first
                    if !current_user_message_parts.is_empty() {
                        log::debug!("Finalizing user message with {} parts.", current_user_message_parts.len());
                        // Create ChatContent from the Vec<ContentPart>
                        let user_content: ChatContent = current_user_message_parts.clone().into();
                        chat_messages.push(ChatMessage {
                            role: ChatRole::User,
                            content: user_content,
                            // Remove message_type, let rllm infer or handle it
                        });
                        current_user_message_parts.clear(); // Clear parts for the next user message
                        _has_image = false; // Reset image flag
                    }

                    // Add the assistant message (assuming content is always text for assistant)
                    log::debug!("Adding assistant message: '{}...'", content.chars().take(50).collect::<String>());
                    let assistant_content: ChatContent = content.clone().into();
                    chat_messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: assistant_content,
                        // Remove message_type
                    });
                }
                // System role is handled at the beginning
            }
        }

        // Add any remaining user message parts after the loop
        // Add any remaining user message parts after the loop
        if !current_user_message_parts.is_empty() {
             log::debug!("Finalizing trailing user message with {} parts.", current_user_message_parts.len());
             let trailing_user_content: ChatContent = current_user_message_parts.into();
             chat_messages.push(ChatMessage {
                 role: ChatRole::User,
                 content: trailing_user_content,
                 // Remove message_type
             });
        }


        // --- Log the final messages being sent (using rllm Debug) ---
        log::debug!("Final RLLM ChatMessages Payload ({} messages):", chat_messages.len());
        for msg in &chat_messages {
            // Access the content string directly and take a preview
            let content_preview = msg.content.lines().next().unwrap_or("").chars().take(100).collect::<String>();
            log::debug!("  - Role: {:?}, Content Preview: '{}...'", msg.role, content_preview);
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
            Ok(response) => {
                let elapsed = start_time.elapsed();
                // Extract the primary text content from the response
                // Assuming the response object has a method or field like `text()` or `content()`
                // Adjust based on the actual structure of rllm::chat::ChatResponse
                let response_str = response.text() // Assuming this method exists
                    .unwrap_or_else(|| {
                        log::warn!("Response text extraction failed, falling back to full response.");
                        response.to_string() // Fallback to the full string representation
                    });
                
                // choices.get(0) // Get the first choice
                //     .and_then(|choice| choice.message.content.clone()) // Get the message content
                //     .unwrap_or_else(|| {
                //         log::warn!("Could not extract primary text from RLLM response, falling back to full string representation.");
                //         response.to_string() // Fallback to the full string representation if extraction fails
                //     });

                log::info!(
                    "RLLM request completed in {:.2}s, received {} characters",
                    elapsed.as_secs_f64(),
                    response_str.len()
                );
                Ok(response_str)
            },
            Err(e) => {
                let elapsed = start_time.elapsed();
                // Log the detailed error from rllm crate
                log::error!("Underlying RLLM chat error: {:?}", e);
                let error_msg = format!(
                    "RLLM chat request failed after {:.2}s: {}",
                    elapsed.as_secs_f64(),
                    e // Keep the original error message for the final anyhow error
                );
                log::error!("{}", error_msg); // Log the formatted message too
                Err(anyhow!(error_msg))
            }
        }
    }
}

