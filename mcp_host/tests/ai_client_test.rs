use mcp_host::ai_client::{AIClient, AIClientFactory, GenerationConfig, ModelCapabilities}; // Removed Content
use serde_json::json;
use shared_protocol_objects::Role;
use std::path::Path;
use anyhow::Result;

// Mock AI Client implementation for testing
struct MockAIClient {
    model_name: String,
    capabilities: ModelCapabilities,
}

impl MockAIClient {
    fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
            capabilities: ModelCapabilities {
                supports_images: true,
                supports_system_messages: true,
                supports_function_calling: true,
                supports_vision: false,
                max_tokens: Some(4096),
                supports_json_mode: true,
            },
        }
    }
}

struct MockRequestBuilder {
    messages: Vec<(Role, String)>,
    config: Option<GenerationConfig>,
}

impl MockRequestBuilder {
    fn new() -> Self {
        Self {
            messages: Vec::new(),
            config: None,
        }
    }
}

#[async_trait::async_trait]
impl mcp_host::ai_client::AIRequestBuilder for MockRequestBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        self.messages.push((Role::System, content));
        self
    }
    
    fn user(mut self: Box<Self>, content: String) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        self.messages.push((Role::User, content));
        self
    }
    
    fn user_with_image(mut self: Box<Self>, text: String, _image_path: &Path) -> Result<Box<dyn mcp_host::ai_client::AIRequestBuilder>> {
        self.messages.push((Role::User, format!("{}[IMAGE]", text)));
        Ok(self)
    }
    
    fn user_with_image_url(mut self: Box<Self>, text: String, _image_url: String) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        self.messages.push((Role::User, format!("{}[IMAGE_URL]", text)));
        self
    }
    
    fn assistant(mut self: Box<Self>, content: String) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        self.messages.push((Role::Assistant, content));
        self
    }
    
    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        self.config = Some(config);
        self
    }
    
    async fn execute(self: Box<Self>) -> Result<String> {
        // For testing, just concatenate all messages
        let mut result = String::new();
        
        for (role, content) in self.messages {
            let role_str = match role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
            };
            
            result.push_str(&format!("{}: {}\n", role_str, content));
        }
        
        if let Some(config) = self.config {
            result.push_str(&format!("\nConfig: temperature={:?}, max_tokens={:?}",
                config.temperature, config.max_tokens));
        }
        
        Ok(result)
    }
}

#[async_trait::async_trait]
impl AIClient for MockAIClient {
    // Add system_prompt argument to match trait
    fn builder(&self, _system_prompt: &str) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        Box::new(MockRequestBuilder::new())
    }

    // Add system_prompt argument to match trait
    fn raw_builder(&self, _system_prompt: &str) -> Box<dyn mcp_host::ai_client::AIRequestBuilder> {
        Box::new(MockRequestBuilder::new())
    }
    fn model_name(&self) -> String {
        self.model_name.clone()
    }
    
    fn capabilities(&self) -> ModelCapabilities {
        self.capabilities.clone()
    }
}

#[tokio::test]
async fn test_ai_client_basic_functionality() {
    let client = MockAIClient::new("test-model");
    
    // Test model name
    assert_eq!(client.model_name(), "test-model");
    
    // Test capabilities
    let caps = client.capabilities();
    assert!(caps.supports_images);
    assert!(caps.supports_system_messages);
    assert_eq!(caps.max_tokens, Some(4096));

    // Test builder - pass empty system prompt
    let builder = client.builder("");
    let builder = builder.system("System prompt".to_string());
    let builder = builder.user("User message".to_string());
    
    let response = builder.execute().await.unwrap();
    assert!(response.contains("System: System prompt"));
    assert!(response.contains("User: User message"));
}

#[tokio::test]
async fn test_ai_client_builder_chaining() {
    let client = MockAIClient::new("test-model");

    // Test chained builder methods - pass empty system prompt
    let builder = client.builder("")
        .system("Initial instructions".to_string())
        .user("Hello".to_string())
        .assistant("Hi there".to_string())
        .user("How are you?".to_string());
    
    let response = builder.execute().await.unwrap();
    
    // Verify all messages are included
    assert!(response.contains("System: Initial instructions"));
    assert!(response.contains("User: Hello"));
    assert!(response.contains("Assistant: Hi there"));
    assert!(response.contains("User: How are you?"));
}

#[tokio::test]
async fn test_ai_client_with_config() {
    let client = MockAIClient::new("test-model");
    
    // Test with configuration
    let config = GenerationConfig {
        temperature: Some(0.7),
        max_tokens: Some(1000),
        top_p: None,
        frequency_penalty: None,
        presence_penalty: None,
    };

    // Pass empty system prompt
    let builder = client.builder("")
        .system("System message".to_string())
        .config(config);
    
    let response = builder.execute().await.unwrap();
    
    // Verify config is applied
    assert!(response.contains("Config: temperature=Some(0.7), max_tokens=Some(1000)"));
}

#[tokio::test]
async fn test_ai_client_factory() {
    // Test creating OpenAI client
    let config = json!({
        "api_key": "test-key",
        "model": "gpt-4"
    });
    
    let client_result = AIClientFactory::create("openai", config);
    assert!(client_result.is_ok());
    let client = client_result.unwrap();
    assert_eq!(client.model_name(), "gpt-4o-mini");
    
    // Test creating with missing API key
    let config = json!({
        "model": "gpt-4"
    });
    
    let client_result = AIClientFactory::create("openai", config);
    assert!(client_result.is_err());
    
    // Test with unknown provider
    let config = json!({
        "api_key": "test-key"
    });
    
    let client_result = AIClientFactory::create("unknown-provider", config);
    assert!(client_result.is_err());
}

#[tokio::test]
async fn test_format_message_for_basic_model() {
    // Test system message formatting
    let formatted = mcp_host::ai_client::format_message_for_basic_model(&Role::System, "Test instructions");
    assert_eq!(formatted, "System: Test instructions");
    
    // Test user message formatting (should be unchanged)
    let formatted = mcp_host::ai_client::format_message_for_basic_model(&Role::User, "Hello");
    assert_eq!(formatted, "Hello");
    
    // Test assistant message formatting
    let formatted = mcp_host::ai_client::format_message_for_basic_model(&Role::Assistant, "Hi there");
    assert_eq!(formatted, "Assistant: Hi there");
}
