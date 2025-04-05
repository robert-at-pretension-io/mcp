use anyhow::{Result, anyhow}; // Removed unused Context
use async_trait::async_trait;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, ModelCapabilities}; // Removed unused Content
use shared_protocol_objects::Role;
use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chat::{ChatMessage, ChatRole, MessageType}; // Import MessageType
use rllm::Llm; // Import Llm from the crate root
use std::path::Path;
use log; // Import log crate

pub struct RLLMClient {
    llm: Llm, // Use the imported Llm type
    model_name: String,
    backend: LLMBackend,
}

impl RLLMClient {
    pub fn new(api_key: String, model: String, backend: LLMBackend) -> Result<Self> {
        log::info!("Creating RLLMClient for backend: {:?}, model: {}", backend, model);
        let mut builder = LLMBuilder::new()
            .backend(backend.clone())
            .model(&model);

        // Only add API key if it's not empty (Ollama doesn't need one)
        if !api_key.is_empty() {
            builder = builder.api_key(api_key);
        }

        let llm = builder.build()?;

        Ok(Self {
            llm,
            model_name: model,
            backend,
        })
    }

    // Convert Role to rllm's ChatRole, skipping System as it's handled separately
    fn convert_role(role: &Role) -> Option<ChatRole> {
        match role {
            Role::User => Some(ChatRole::User),
            Role::Assistant => Some(ChatRole::Assistant),
            Role::System => None, // System messages are handled by the .system() method
        }
    }
}

// Implement Debug manually as rllm::LLM doesn't derive Debug
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
                supports_images: true, // OpenAI models like gpt-4o support images
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
            // Add other backends as needed
             _ => {
                log::warn!("Capabilities not defined for RLLM backend: {:?}. Using default.", self.backend);
                ModelCapabilities::default()
            }
        }
    }
}

#[derive(Debug)] // Add Debug derive
struct RLLMRequestBuilder {
    client: Llm, // Use the imported Llm type
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
        log::warn!("RLLM adapter received user_with_image request for path: {:?}. Image content will be omitted.", image_path);
        // Note: rllm doesn't have direct image support in the way we're using it
        // But we can adapt with text
        let mut s = self;
        s.messages.push((Role::User, format!("{} [Image at path '{}' processed but not included in prompt]", text, image_path.display())));
        Ok(s)
    }

    fn user_with_image_url(self: Box<Self>, text: String, image_url: String) -> Box<dyn AIRequestBuilder> {
        log::warn!("RLLM adapter received user_with_image_url request for URL: {}. Image content will be omitted.", image_url);
        // Similar to above
        let mut s = self;
        s.messages.push((Role::User, format!("{} [Image at URL '{}' processed but not included in prompt]", text, image_url)));
        s
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
        log::info!("Executing RLLM request");
        let mut client = self.client.clone(); // Start with the base client

        // Apply system message if provided (this modifies the client)
        if let Some(system_content) = &self.system {
            log::debug!("Applying system message to RLLM client");
            client = client.system(system_content);
        }

        // Convert non-system messages to rllm ChatMessage format
        let mut rllm_messages = Vec::new();
        for (role, content) in &self.messages {
            // Use the updated convert_role which returns Option<ChatRole>
            if let Some(chat_role) = RLLMClient::convert_role(role) {
                rllm_messages.push(ChatMessage {
                    role: chat_role,
                    content: content.clone().into(), // Use .into() for String -> ChatContent conversion
                    message_type: MessageType::Text, // Use the correct enum variant
                });
            } else if matches!(role, Role::System) {
                // System messages are handled above by client.system(), skip here
                log::trace!("Skipping Role::System during ChatMessage conversion");
            } else {
                 log::warn!("Unhandled role type encountered: {:?}", role);
            }
        }

        // Apply configuration if provided (this also modifies the client)
        if let Some(cfg) = &self.config {
            if let Some(temp) = cfg.temperature {
                log::debug!("Setting temperature: {}", temp);
                client = client.temperature(temp);
            }
            if let Some(max_tokens) = cfg.max_tokens {
                 log::debug!("Setting max_tokens: {}", max_tokens);
                client = client.max_tokens(max_tokens as usize);
            }
            // Add other config mappings if needed (top_p, etc.)
             if let Some(top_p) = cfg.top_p {
                 log::debug!("Setting top_p: {}", top_p);
                 client = client.top_p(top_p);
             }
        }
        
        // Apply system message if provided
        if let Some(system) = &self.system {
            log::debug!("Applying system message");
            client = client.system(system);
        }
        
        // Execute the request
        log::debug!("Sending chat request to RLLM backend");
        let result = client.chat(&rllm_messages)
            .map_err(|e| anyhow!("RLLM chat execution failed: {}", e))?;
        
        log::info!("RLLM request successful, received response");
        log::debug!("RLLM response content length: {}", result.len());
        
        Ok(result)
    }
}
