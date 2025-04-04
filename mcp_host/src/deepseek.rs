use anyhow::Result;
use async_trait::async_trait;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequest, 
        CreateChatCompletionRequestArgs,
    },
    Client,
};
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig};
use shared_protocol_objects::Role;

/// A client for DeepSeek, implementing your `AIClient` trait
#[derive(Debug, Clone)]
pub struct DeepSeekClient {
    api_key: String,
    model: String,
}

impl DeepSeekClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }

    /// Creates a new `async_openai` Client with custom config pointing to DeepSeek
    async fn create_inner_client(&self) -> Client<OpenAIConfig> {
        let config = OpenAIConfig::new()
            .with_api_key(&self.api_key)
            .with_api_base("https://api.deepseek.com/v1"); 
        Client::with_config(config)
    }
}

#[async_trait]
impl AIClient for DeepSeekClient {
    fn model_name(&self) -> String {
        self.model.clone()
    }

    fn builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(DeepSeekCompletionBuilder {
            client: self.clone(),
            messages: Vec::new(),
            config: None,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder> {
        self.builder()
    }
}

/// A builder struct implementing `AIRequestBuilder` for DeepSeek
#[derive(Debug, Clone)]
pub struct DeepSeekCompletionBuilder {
    client: DeepSeekClient,
    messages: Vec<(Role, String)>,
    config: Option<GenerationConfig>,
}

#[async_trait]
impl AIRequestBuilder for DeepSeekCompletionBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::System, content));
        self
    }

    fn user(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::User, content));
        self
    }

    fn user_with_image(self: Box<Self>, text: String, _image_path: &std::path::Path) -> Result<Box<dyn AIRequestBuilder>> {
        // Not truly supported: for now, treat it as text + note
        let mut s = self;
        s.messages.push((Role::User, format!("{} [Image omitted]", text)));
        Ok(s)
    }

    fn user_with_image_url(self: Box<Self>, text: String, _image_url: String) -> Box<dyn AIRequestBuilder> {
        // Similarly, treat as text
        let mut s = self;
        s.messages.push((Role::User, format!("{} [Image URL omitted]", text)));
        s
    }

    fn assistant(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.messages.push((Role::Assistant, content));
        self
    }

    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder> {
        self.config = Some(config);
        self
    }

    /// Execute the request in non-streaming mode, returning a single `String`
    async fn execute(self: Box<Self>) -> Result<String> {
        let client = self.client.create_inner_client().await;
        let request = build_deepseek_request(&self.client.model, &self.messages, self.config.as_ref(), false)?;
        let response = client.chat().create(request).await?;

        let full_content = response.choices
            .get(0)
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default();

        Ok(full_content)
    }
}

fn build_deepseek_request(
    model: &str,
    messages: &[(Role, String)],
    config: Option<&GenerationConfig>,
    streaming: bool,
) -> Result<CreateChatCompletionRequest> {
    // Convert your internal messages to ChatCompletionRequestMessage
    let converted_messages = messages.iter().map(|(role, content)| {
        match role {
            Role::System => {
                let msg = ChatCompletionRequestSystemMessageArgs::default()
                    .content(content.clone())
                    .build()?;
                Ok::<_, anyhow::Error>(msg.into())
            }
            Role::User => {
                let msg = ChatCompletionRequestUserMessageArgs::default()
                    .content(content.clone())
                    .build()?;
                Ok::<_, anyhow::Error>(msg.into())
            }
            Role::Assistant => {
                let msg = ChatCompletionRequestAssistantMessageArgs::default()
                    .content(content.clone())
                    .build()?;
                Ok::<_, anyhow::Error>(msg.into())
            }
        }
    }).collect::<Result<Vec<_>, anyhow::Error>>()?;

    // Create a local builder variable
    let mut builder = CreateChatCompletionRequestArgs::default();
    
    // Chain method calls on the local variable
    builder
        .model(model)
        .messages(converted_messages)
        .stream(streaming);

    // Apply optional config settings
    if let Some(cfg) = config {
        if let Some(temp) = cfg.temperature {
            builder.temperature(temp);
        }
        if let Some(max_tokens) = cfg.max_tokens {
            builder.max_tokens(max_tokens);
        }
        // top_p, frequency_penalty, presence_penalty can be set similarly
    }

    Ok(builder.build()?)
}

