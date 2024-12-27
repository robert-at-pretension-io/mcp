use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage,
        CreateChatCompletionRequest, CreateChatCompletionRequestArgs, ChatChoice, ChatCompletionResponseStream,
    },
    Client,
};
use futures::StreamExt;
use log::{debug, error};
use serde_json::Value;
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, StreamResult};
use shared_protocol_objects::Role;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use futures::Stream;

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
            stream: false,
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
    stream: bool,
}

#[async_trait]
impl AIRequestBuilder for DeepSeekCompletionBuilder {
    fn streaming(mut self: Box<Self>, enabled: bool) -> Box<dyn AIRequestBuilder> {
        self.stream = enabled;
        self
    }

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

    /// Execute the request in streaming mode, returning a StreamResult 
    async fn execute_streaming(self: Box<Self>) -> Result<StreamResult> {
        // Build the chat request using async_openai's CreateChatCompletionRequest
        let client = self.client.create_inner_client().await;
        let request = build_deepseek_request(&self.client.model, &self.messages, self.config.as_ref(), /* streaming */ true)?;
        let mut stream = client.chat().create_stream(request).await?;

        // We'll convert that `ChatCompletionResponseStream` into our own Stream of `StreamEvent`
        let event_stream = DeepSeekStream { inner: stream };
        Ok(Box::pin(event_stream))
    }

    /// Execute the request in non-streaming mode, returning a single `String`
    async fn execute(self: Box<Self>) -> Result<String> {
        let client = self.client.create_inner_client().await;
        let request = build_deepseek_request(&self.client.model, &self.messages, self.config.as_ref(), /* streaming */ false)?;
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
    let converted_messages: Vec<ChatCompletionRequestMessage> = messages.iter().map(|(role, content)| {
        let role_str = match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        ChatCompletionRequestMessage {
            role: role_str.to_string(),
            content: Some(content.clone()),
            name: None,
            function_call: None,
        }
    }).collect();

    // Build the request
    let mut builder = CreateChatCompletionRequestArgs::default()
        .model(model)  
        .messages(converted_messages)
        .stream(streaming);

    if let Some(cfg) = config {
        if let Some(temp) = cfg.temperature {
            builder = builder.temperature(temp);
        }
        if let Some(max_tokens) = cfg.max_tokens {
            builder = builder.max_tokens(max_tokens);
        }
        // top_p, frequency_penalty, presence_penalty can be set similarly 
    }

    Ok(builder.build()?)
}

/// A custom Stream wrapper that converts `ChatCompletionResponseStream` items into `StreamEvent`
struct DeepSeekStream {
    inner: ChatCompletionResponseStream,
}

impl Stream for DeepSeekStream {
    type Item = Result<crate::ai_client::StreamEvent>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>
    ) -> Poll<Option<Self::Item>> {
        let me = self.get_mut();

        match Pin::new(&mut me.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(response))) => {
                // Each chunk is a partial response 
                if let Some(choice) = response.choices.first() {
                    // If there's partial text, yield it as ContentDelta
                    if let Some(delta_text) = &choice.delta.content {
                        let event = crate::ai_client::StreamEvent::ContentDelta {
                            index: 0,
                            text: delta_text.clone(),
                        };
                        return Poll::Ready(Some(Ok(event)));
                    }
                    // End condition
                    if choice.finish_reason.is_some() {
                        // If the finish reason is "stop", we yield a MessageStop
                        let event = crate::ai_client::StreamEvent::MessageStop;
                        return Poll::Ready(Some(Ok(event)));
                    }
                }
                // Otherwise keep waiting
                Poll::Ready(Some(Ok(crate::ai_client::StreamEvent::ContentDelta {
                    index: 0,
                    text: "".into(),
                })))
            }
            Poll::Ready(Some(Err(e))) => {
                // If the streaming had an error
                Poll::Ready(Some(Err(anyhow!("DeepSeek stream error: {}", e))))
            }
            Poll::Ready(None) => {
                // No more messages
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
