use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use anyhow::{Result, Context};
use reqwest;
use log::{debug, error, info, warn};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::path::Path;
use std::{fs, time::{SystemTime, UNIX_EPOCH}};
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig, ModelCapabilities, Role, Content, Message as AIMessage};
use async_trait::async_trait;

#[async_trait]
impl AIClient for OpenAIClient {
    fn model_name(&self) -> String {
        "gpt-4".to_string()
    }

    fn builder(&self) -> Box<dyn AIRequestBuilder + 'static> {
        Box::new(RawCompletionBuilder {
            client: self,
            model: "gpt-4".to_string(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder + 'static> {
        Box::new(RawCompletionBuilder {
            client: self,
            model: "gpt-4".to_string(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Image { url: String, detail: Option<String> },
    MultiPart(Vec<MessagePart>),
}

#[derive(Debug, Clone)]
pub enum MessagePart {
    Text(String),
    Image { url: String, detail: Option<String> },
}

#[derive(Debug, Clone)]
pub struct Message {
    role: String,
    content: MessageContent,
}

#[derive(Debug)]
pub struct OpenAIClient {
    api_key: String,
    endpoint: String,
    speech_endpoint: String,
    transcription_endpoint: String,  // New field
}

#[derive(Debug)]
pub struct SpeechBuilder<'a> {
    client: &'a OpenAIClient,
    model: String,
    input: String,
    voice: String,
    response_format: Option<String>,
    speed: Option<f32>,
}

#[derive(Debug)]
pub struct TranscriptionBuilder<'a> {
    client: &'a OpenAIClient,
    file_path: String,
    model: String,
    language: Option<String>,
    prompt: Option<String>,
    response_format: Option<String>,
    temperature: Option<f32>,
    timestamp_granularities: Option<Vec<String>>,
}

pub struct CompletionBuilder<'a, T> 
where
    T: DeserializeOwned + Serialize + schemars::JsonSchema,
{
    client: &'a OpenAIClient,
    model: String,
    messages: Vec<Message>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    _marker: std::marker::PhantomData<T>,
}

impl OpenAIClient {
    pub fn new(api_key: String) -> Self {
        info!("Creating new OpenAIClient with provided API key");
        OpenAIClient {
            api_key,
            endpoint: String::from("https://api.openai.com/v1/chat/completions"),
            speech_endpoint: String::from("https://api.openai.com/v1/audio/speech"),
            transcription_endpoint: String::from("https://api.openai.com/v1/audio/transcriptions"),
        }
    }

    fn log_payload(method: &str, payload: &serde_json::Value) {
        debug!("log_payload called for method: {}", method);
        let filtered_payload = payload.as_object().map(|obj| {
            let mut filtered = serde_json::Map::new();
            for (key, value) in obj {
                if key == "messages" {
                    debug!("Filtering messages in payload for method: {}", method);
                }
                match value {
                    serde_json::Value::Array(messages) if key == "messages" => {
                        let filtered_messages: Vec<serde_json::Value> = messages.iter().map(|msg| {
                            if let Some(content) = msg.get("content") {
                                if let Some(content_arr) = content.as_array() {
                                    let filtered_content: Vec<serde_json::Value> = content_arr.iter().map(|item| {
                                        if let Some(obj) = item.as_object() {
                                            let mut filtered_item = serde_json::Map::new();
                                            for (k, v) in obj {
                                                if k == "image_url" {
                                                    filtered_item.insert(k.clone(), json!("[BASE64_IMAGE_DATA_FILTERED]"));
                                                } else {
                                                    filtered_item.insert(k.clone(), v.clone());
                                                }
                                            }
                                            serde_json::Value::Object(filtered_item)
                                        } else {
                                            item.clone()
                                        }
                                    }).collect();
                                    serde_json::Value::Array(filtered_content)
                                } else {
                                    content.clone()
                                }
                            } else {
                                msg.clone()
                            }
                        }).collect();
                        filtered.insert(key.clone(), serde_json::Value::Array(filtered_messages));
                    },
                    _ => {
                        filtered.insert(key.clone(), value.clone());
                    }
                }
            }
            serde_json::Value::Object(filtered)
        }).unwrap_or(payload.clone());

        debug!("OpenAI API Payload for {}: {}", 
            method,
            serde_json::to_string_pretty(&filtered_payload).unwrap_or_else(|e| {
                error!("Failed to serialize payload in log_payload: {}", e);
                String::from("Failed to serialize payload")
            })
        );
    }

    fn sanitize_schema_name(name: &str) -> String {
        debug!("sanitize_schema_name called with {}", name);
        let base_name = name.split("::").last().unwrap_or(name);
        let sanitized = base_name
            .chars()
            .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect::<String>();
        if sanitized.chars().next().map_or(true, |c| !c.is_alphabetic()) {
            let final_name = format!("schema_{}", sanitized);
            debug!("Returning sanitized schema name: {}", final_name);
            final_name
        } else {
            debug!("Returning sanitized schema name: {}", sanitized);
            sanitized
        }
    }

    pub fn builder<T>(&self) -> CompletionBuilder<'_, T> 
    where
        T: DeserializeOwned + Serialize + schemars::JsonSchema,
    {
        debug!("Creating CompletionBuilder with generic type T");
        CompletionBuilder {
            client: self,
            model: "gpt-4o-mini".to_string(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            _marker: std::marker::PhantomData,
        }
    }


    pub fn raw_builder(&self) -> RawCompletionBuilder<'_> {
        debug!("Creating RawCompletionBuilder");
        RawCompletionBuilder {
            client: self,
            model: "gpt-4o-mini".into(),
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
        }
    }

    pub fn transcription_builder(&self) -> TranscriptionBuilder<'_> {
        debug!("Creating TranscriptionBuilder");
        TranscriptionBuilder {
            client: self,
            file_path: String::new(),
            model: "whisper-1".to_string(),
            language: None,
            prompt: None,
            response_format: None,
            temperature: None,
            timestamp_granularities: None,
        }
    }

    pub fn speech_builder(&self) -> SpeechBuilder<'_> {
        debug!("Creating SpeechBuilder");
        SpeechBuilder {
            client: self,
            model: "tts-1".to_string(),
            input: String::new(),
            voice: "alloy".to_string(),
            response_format: None,
            speed: None,
        }
    }
}

pub struct RawCompletionBuilder<'a> {
    client: &'a OpenAIClient,
    model: String,
    messages: Vec<Message>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

impl<'a> RawCompletionBuilder<'a> {
    pub fn model(mut self, model: impl Into<String>) -> Self {
        let model_str = model.into();
        debug!("RawCompletionBuilder: Setting model to {}", model_str);
        self.model = model_str;
        self
    }

    pub fn system(mut self, content: impl Into<String>) -> Self {
        let c = content.into();
        debug!("RawCompletionBuilder: Adding system message: {}", c);
        self.messages.push(Message {
            role: "system".to_string(),
            content: MessageContent::Text(c),
        });
        self
    }

    pub fn user(mut self, content: impl Into<String>) -> Self {
        let c = content.into();
        debug!("RawCompletionBuilder: Adding user message: {}", c);
        self.messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::Text(c),
        });
        self
    }

    pub fn user_with_image(mut self, text: impl Into<String>, image_path: impl AsRef<Path>) -> Result<Self> {
        let t = text.into();
        debug!("RawCompletionBuilder: Adding user message with image from path: {}", image_path.as_ref().display());
        let image_data = fs::read(image_path)?;
        let base64_image = BASE64.encode(&image_data);
        let base64_url = format!("data:image/jpeg;base64,{}", base64_image);

        self.messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::MultiPart(vec![
                MessagePart::Text(t),
                MessagePart::Image { 
                    url: base64_url,
                    detail: Some("high".to_string())
                }
            ]),
        });
        Ok(self)
    }

    pub fn user_with_image_url(mut self, text: impl Into<String>, image_url: impl Into<String>) -> Self {
        let t = text.into();
        let i = image_url.into();
        debug!("RawCompletionBuilder: Adding user message with image_url: {}", i);
        self.messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::MultiPart(vec![
                MessagePart::Text(t),
                MessagePart::Image { 
                    url: i,
                    detail: Some("high".to_string())
                }
            ]),
        });
        self
    }

    pub fn assistant(mut self, content: impl Into<String>) -> Self {
        let c = content.into();
        debug!("RawCompletionBuilder: Adding assistant message: {}", c);
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: MessageContent::Text(c),
        });
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        debug!("RawCompletionBuilder: Setting temperature to {}", temp);
        self.temperature = Some(temp);
        self
    }

    pub fn max_tokens(mut self, tokens: u32) -> Self {
        debug!("RawCompletionBuilder: Setting max_tokens to {}", tokens);
        self.max_tokens = Some(tokens);
        self
    }

    fn format_message_content(content: &MessageContent) -> Value {
        debug!("format_message_content called");
        match content {
            MessageContent::Text(text) => {
                debug!("MessageContent is Text: {}", text);
                json!(text)
            },
            MessageContent::Image { url, detail } => {
                debug!("MessageContent is single Image: {}, detail: {:?}", url, detail);
                json!([{
                    "type": "image_url",
                    "image_url": {
                        "url": url,
                        "detail": detail.clone().unwrap_or_else(|| "high".to_string())
                    }
                }])
            },
            MessageContent::MultiPart(parts) => {
                debug!("MessageContent is MultiPart with {} parts", parts.len());
                let content_parts: Vec<Value> = parts.iter().map(|part| match part {
                    MessagePart::Text(text) => {
                        debug!("MultiPart text: {}", text);
                        json!({
                            "type": "text",
                            "text": text
                        })
                    },
                    MessagePart::Image { url, detail } => {
                        debug!("MultiPart image: {}", url);
                        json!({
                            "type": "image_url",
                            "image_url": {
                                "url": url,
                                "detail": detail.clone().unwrap_or_else(|| "high".to_string())
                            }
                        })
                    }
                }).collect();
                
                json!(content_parts)
            }
        }
    }

    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder> where Self: Sized {
        if let Some(temp) = config.temperature {
            self.temperature = Some(temp);
        }
        if let Some(tokens) = config.max_tokens {
            self.max_tokens = Some(tokens);
        }
        self
    }

    pub async fn execute(self) -> Result<String> {
        debug!("RawCompletionBuilder.execute called");
        let messages = self.messages.iter().map(|msg| {
            debug!("formatting message with role: {}", msg.role);
            json!({
                "role": msg.role,
                "content": Self::format_message_content(&msg.content)
            })
        }).collect::<Vec<_>>();

        let mut payload = json!({
            "model": self.model,
            "messages": messages,
        });

        if let Some(temp) = self.temperature {
            debug!("Setting temperature in payload: {}", temp);
            payload.as_object_mut().unwrap().insert("temperature".to_string(), json!(temp));
        }
        if let Some(tokens) = self.max_tokens {
            debug!("Setting max_tokens in payload: {}", tokens);
            payload.as_object_mut().unwrap().insert("max_tokens".to_string(), json!(tokens));
        }

        OpenAIClient::log_payload("execute_raw", &payload);

        debug!("Sending request to OpenAI API for raw response");
        let client = reqwest::Client::new();
        let response = client
            .post(&self.client.endpoint)
            .header("Authorization", format!("Bearer {}", self.client.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        debug!("Response received, status: {}", response.status());
        let status = response.status();
        if !status.is_success() {
            warn!("Response not successful, status: {}", status);
            let error_text = response.text().await.context("Failed to read error response")?;
            error!("API error response: {}", error_text);
            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_text));
        }

        let response_text: String = response.text().await?;
        debug!("Full API response (raw): {}", response_text);

        let response_json: Value = match serde_json::from_str(&response_text) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse response as JSON: {}. Raw response: {}", e, response_text);
                return Err(anyhow::anyhow!("Failed to parse API response as JSON: {}", response_text));
            }
        };

        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .context("Failed to get content from response")?;

        debug!("Returning raw content from response");
        Ok(content.to_string())
    }
}

impl<'a> TranscriptionBuilder<'a> {
    pub fn file(mut self, path: impl Into<String>) -> Self {
        let p = path.into();
        debug!("Setting transcription file path: {}", p);
        self.file_path = p;
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        let m = model.into();
        debug!("Setting transcription model: {}", m);
        self.model = m;
        self
    }

    pub fn language(mut self, language: impl Into<String>) -> Self {
        let l = language.into();
        debug!("Setting transcription language: {}", l);
        self.language = Some(l);
        self
    }

    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        let pr = prompt.into();
        debug!("Setting transcription prompt: {}", pr);
        self.prompt = Some(pr);
        self
    }

    pub fn response_format(mut self, format: impl Into<String>) -> Self {
        let f = format.into();
        debug!("Setting transcription response_format: {}", f);
        self.response_format = Some(f);
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        debug!("Setting transcription temperature: {}", temp);
        self.temperature = Some(temp);
        self
    }

    pub fn timestamp_granularities(mut self, granularities: Vec<String>) -> Self {
        debug!("Setting transcription timestamp_granularities: {:?}", granularities);
        self.timestamp_granularities = Some(granularities);
        self
    }

    pub async fn execute(self) -> Result<String> {
        debug!("TranscriptionBuilder.execute called");
        let file_path = Path::new(&self.file_path);
        
        let file_bytes = tokio::fs::read(&file_path).await?;
        let file_name = Path::new(&file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
            
        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.to_string());
            
        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model);

        if let Some(lang) = self.language {
            debug!("Adding language to transcription form: {}", lang);
            form = form.text("language", lang);
        }
    
        if let Some(prompt) = self.prompt {
            debug!("Adding prompt to transcription form: {}", prompt);
            form = form.text("prompt", prompt);
        }
    
        if let Some(format) = self.response_format {
            debug!("Adding response_format to transcription form: {}", format);
            form = form.text("response_format", format);
        }
    
        if let Some(temp) = self.temperature {
            debug!("Adding temperature to transcription form: {}", temp);
            form = form.text("temperature", temp.to_string());
        }
    
        if let Some(grans) = self.timestamp_granularities {
            debug!("Adding timestamp_granularities to transcription form: {:?}", grans);
            for granularity in grans {
                form = form.text("timestamp_granularities[]", granularity);
            }
        }
    
        debug!("Sending request to OpenAI Transcription API");
        let client = reqwest::Client::new();
        let response = client
            .post(&self.client.transcription_endpoint)
            .header("Authorization", format!("Bearer {}", self.client.api_key))
            .multipart(form)
            .send()
            .await?;
    
        debug!("Transcription response received with status: {}", response.status());
        
        let status = response.status();
        if !status.is_success() {
            warn!("Transcription request not successful, status: {}", status);
            let error_text = response.text().await.context("Failed to read error response")?;
            error!("API error response for transcription: {}", error_text);
            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_text));
        }
    
        let response_text = response.text().await?;
        debug!("Transcription response text: {}", response_text);
        Ok(response_text)
    }
    
}

impl<'a, T> CompletionBuilder<'a, T>
where
    T: DeserializeOwned + Serialize + schemars::JsonSchema,
{
    pub fn model(mut self, model: impl Into<String>) -> Self {
        let m = model.into();
        debug!("CompletionBuilder: Setting model: {}", m);
        self.model = m;
        self
    }

    pub fn system(mut self, content: impl Into<String>) -> Self {
        let c = content.into();
        debug!("CompletionBuilder: Adding system message: {}", c);
        self.messages.push(Message {
            role: "system".to_string(),
            content: MessageContent::Text(c),
        });
        self
    }

    pub fn user(mut self, content: impl Into<String>) -> Self {
        let c = content.into();
        debug!("CompletionBuilder: Adding user message: {}", c);
        self.messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::Text(c),
        });
        self
    }

    pub fn user_with_image(mut self, text: impl Into<String>, image_path: impl AsRef<Path>) -> Result<Self> {
        let t = text.into();
        debug!("CompletionBuilder: Adding user_with_image from {}", image_path.as_ref().display());
        let image_data = fs::read(image_path)?;
        let base64_image = BASE64.encode(&image_data);
        let base64_url = format!("data:image/jpeg;base64,{}", base64_image);

        self.messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::MultiPart(vec![
                MessagePart::Text(t),
                MessagePart::Image { 
                    url: base64_url,
                    detail: Some("high".to_string())
                }
            ]),
        });
        Ok(self)
    }

    pub fn user_with_image_url(mut self, text: impl Into<String>, image_url: impl Into<String>) -> Self {
        let t = text.into();
        let i = image_url.into();
        debug!("CompletionBuilder: Adding user_with_image_url: {}", i);
        self.messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::MultiPart(vec![
                MessagePart::Text(t),
                MessagePart::Image { 
                    url: i,
                    detail: Some("high".to_string())
                }
            ]),
        });
        self
    }

    pub fn assistant(mut self, content: impl Into<String>) -> Self {
        let c = content.into();
        debug!("CompletionBuilder: Adding assistant message: {}", c);
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: MessageContent::Text(c),
        });
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        debug!("CompletionBuilder: Setting temperature: {}", temp);
        self.temperature = Some(temp);
        self
    }

    pub fn max_tokens(mut self, tokens: u32) -> Self {
        debug!("CompletionBuilder: Setting max_tokens: {}", tokens);
        self.max_tokens = Some(tokens);
        self
    }

    fn format_message_content(content: &MessageContent) -> Value {
        debug!("CompletionBuilder.format_message_content called");
        match content {
            MessageContent::Text(text) => {
                debug!("Content is text: {}", text);
                json!(text)
            },
            MessageContent::Image { url, detail } => {
                debug!("Content is single image");
                json!([{
                    "type": "image_url",
                    "image_url": {
                        "url": url,
                        "detail": detail.clone().unwrap_or_else(|| "high".to_string())
                    }
                }])
            },
            MessageContent::MultiPart(parts) => {
                debug!("Content is MultiPart with {} parts", parts.len());
                let content_parts: Vec<Value> = parts.iter().map(|part| match part {
                    MessagePart::Text(text) => {
                        debug!("MultiPart text: {}", text);
                        json!({
                            "type": "text",
                            "text": text
                        })
                    },
                    MessagePart::Image { url, detail } => {
                        debug!("MultiPart image..");
                        json!({
                            "type": "image_url",
                            "image_url": {
                                "url": url,
                                "detail": detail.clone().unwrap_or_else(|| "high".to_string())
                            }
                        })
                    }
                }).collect();
                json!(content_parts)
            }
        }
    }

    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder> where Self: Sized {
        if let Some(temp) = config.temperature {
            self.temperature = Some(temp);
        }
        if let Some(tokens) = config.max_tokens {
            self.max_tokens = Some(tokens);
        }
        self
    }

    pub async fn execute(self) -> Result<T> {
        debug!("CompletionBuilder.execute called");
        let schema = schemars::gen::SchemaGenerator::default()
            .into_root_schema_for::<T>();
        
        let schema_name = OpenAIClient::sanitize_schema_name(std::any::type_name::<T>());
        debug!("Using schema_name: {}", schema_name);

        let messages = self.messages.iter().map(|msg| {
            debug!("Formatting message with role: {}", msg.role);
            json!({
                "role": msg.role,
                "content": Self::format_message_content(&msg.content)
            })
        }).collect::<Vec<_>>();

        let mut payload = json!({
            "model": self.model,
            "messages": messages,
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": schema_name,
                    "schema": schema,
                }
            }
        });

        if let Some(temp) = self.temperature {
            debug!("Adding temperature to payload: {}", temp);
            payload.as_object_mut().unwrap().insert("temperature".to_string(), json!(temp));
        }
        if let Some(tokens) = self.max_tokens {
            debug!("Adding max_tokens to payload: {}", tokens);
            payload.as_object_mut().unwrap().insert("max_tokens".to_string(), json!(tokens));
        }

        OpenAIClient::log_payload("execute", &payload);

        debug!("Sending request to OpenAI API for structured response");
        let client = reqwest::Client::new();
        let response = client
            .post(&self.client.endpoint)
            .header("Authorization", format!("Bearer {}", self.client.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        debug!("Response received with status: {}", response.status());
        let status = response.status();
        if !status.is_success() {
            warn!("Request not successful, status: {}", status);
            let error_text = response.text().await.context("Failed to read error response")?;
            error!("API error response: {}", error_text);
            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_text));
        }

        let response_text = response.text().await?;
        debug!("Full API structured response: {}", response_text);

        let response_json: Value = match serde_json::from_str(&response_text) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse response as JSON: {}. Raw: {}", e, response_text);
                return Err(anyhow::anyhow!("Failed to parse API response as JSON: {}", response_text));
            }
        };

        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .context("Failed to get content from response")?;
        debug!("Parsing content into target type");

        let result: T = match serde_json::from_str(content) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse content into target type: {}", e);
                return Err(anyhow::anyhow!("Failed to parse content into target type: {}", content));
            }
        };

        Ok(result)
    }
}


#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug)]
struct ImageAnalysis {
    description: String,
    objects: Vec<String>,
    dominant_colors: Vec<String>,
    mood: String,
}


#[derive(Debug, Serialize, Deserialize)]
struct ImageMetadata {
    hash: String,
    filename: String,
    status: String, // e.g. "in_progress", "done"
    output_file: Option<String>, // file where combined text is stored
    last_updated: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Metadata {
    images: Vec<ImageMetadata>,
}

impl Metadata {
    fn load(path: &str) -> Result<Metadata> {
        debug!("Loading metadata from {}", path);
        if Path::new(path).exists() {
            let data = fs::read_to_string(path)?;
            let metadata: Metadata = serde_json::from_str(&data)?;
            debug!("Loaded metadata with {} images", metadata.images.len());
            Ok(metadata)
        } else {
            debug!("No metadata file found at {}, returning empty", path);
            Ok(Metadata::default())
        }
    }

    fn save(&self, path: &str) -> Result<()> {
        debug!("Saving metadata to {}", path);
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    fn get_image(&self, hash: &str) -> Option<&ImageMetadata> {
        debug!("get_image called for hash: {}", hash);
        self.images.iter().find(|img| img.hash == hash)
    }

    fn get_image_mut(&mut self, hash: &str) -> Option<&mut ImageMetadata> {
        debug!("get_image_mut called for hash: {}", hash);
        self.images.iter_mut().find(|img| img.hash == hash)
    }

    fn upsert_image(&mut self, meta: ImageMetadata) {
        debug!("upsert_image called for hash: {}", meta.hash);
        if let Some(img) = self.get_image_mut(&meta.hash) {
            debug!("Image with hash {} already exists, updating", meta.hash);
            *img = meta;
        } else {
            debug!("Image with hash {} not found, inserting new", meta.hash);
            self.images.push(meta);
        }
    }
}



pub struct Processor<'a> {
    client: &'a OpenAIClient,
    metadata_path: String,
    output_dir: String,
}

impl<'a> Processor<'a> {
    pub fn new(client: &'a OpenAIClient, metadata_path: &str, output_dir: &str) -> Self {
        debug!("Creating Processor with metadata_path: {} and output_dir: {}", metadata_path, output_dir);
        Processor {
            client,
            metadata_path: metadata_path.to_string(),
            output_dir: output_dir.to_string(),
        }
    }

    async fn process_image_chunks(&self, image_path: &str) -> Result<(Vec<String>, Vec<String>)> {
        info!("process_image_chunks called for {}", image_path);
        let img = image::open(&image_path)
            .with_context(|| format!("Failed to open image: {}", image_path))?;
        debug!("Opened image {} successfully", image_path);
    
        let (width, height) = img.dimensions();
        debug!("Image dimensions: {}x{}", width, height);
    
        let chunk_height = 1000;
        debug!("Using chunk_height: {}", chunk_height);
        let mut y = 0;
        let mut piece_paths = vec![];
    
        while y < height {
            let h = if y + chunk_height > height {
                height - y
            } else {
                chunk_height
            };
    
            debug!("Processing chunk at y={}, height={}", y, h);
            let subimg = img.view(0, y, width, h);
            let subimg_buffer: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> = subimg.to_image();
            let piece_path = format!("{}_chunk_{}.png", image_path, piece_paths.len());
            debug!("Saving chunk to {}", piece_path);
            subimg_buffer.save(&piece_path)?;
            piece_paths.push(piece_path.clone());
    
            y += h;
        }
    
        info!("Split {} into {} chunks", image_path, piece_paths.len());
        let mut all_responses = Vec::new();
    
        for piece_path in &piece_paths {
            debug!("Sending chunk {} for analysis", piece_path);
            let raw_response = self.client
                .raw_builder()
                .model("gpt-4o-mini")
                .system("You are an expert at extracting text and code from images.")
                .user_with_image(
                    "Extract all visible code (in Markdown code blocks if possible) and all visible text:",
                    piece_path
                )?
                .temperature(0.0)
                .max_tokens(5000)
                .execute()
                .await;
    
            match raw_response {
                Ok(resp) => {
                    debug!("Received response for chunk {}: {} chars", piece_path, resp.len());
                    all_responses.push(resp);
                },
                Err(e) => {
                    error!("Error processing chunk {}: {}", piece_path, e);
                    // Decide if this should fail early or continue
                    // For now, continue
                }
            }
        }
    
        debug!("Returning all responses for {}", image_path);
        Ok((all_responses, piece_paths))
    }
    
    // In process_image:
    pub async fn process_image(&self, image_path: &str) -> Result<()> {
        info!("process_image called for {}", image_path);
        let hash = self.compute_hash(image_path)?;
        debug!("Computed hash for {}: {}", image_path, hash);
    
        let mut metadata = Metadata::load(&self.metadata_path)?;
    
        if let Some(img_meta) = metadata.get_image(&hash) {
            if img_meta.status == "done" {
                info!("Image {} (hash: {}) is already processed. Skipping.", image_path, hash);
                return Ok(());
            } else if img_meta.status == "in_progress" {
                warn!("Image {} (hash: {}) was previously in_progress. Attempting to reprocess.", image_path, hash);
            }
        } else {
            debug!("No existing record for image {}", image_path);
        }
    
        self.update_metadata(&mut metadata, &hash, image_path, "in_progress", None)?;
    
        debug!("Processing image chunks for {}", image_path);
        let (responses, piece_paths) = self.process_image_chunks(image_path).await?;
        debug!("Got {} responses from chunks of {}", responses.len(), image_path);
    
        let combined = responses.join("\n\n---\n\n");
        fs::create_dir_all(&self.output_dir)?;
        let output_file = format!("{}/{}.txt", self.output_dir, hash);
        debug!("Writing combined output to {}", output_file);
        fs::write(&output_file, &combined)?;
    
        self.update_metadata(&mut metadata, &hash, image_path, "done", Some(&output_file))?;
    
        info!("Processing of {} complete. Output saved to {}", image_path, output_file);
    
        // Clean up chunks
        debug!("Removing chunk files for {}", image_path);
        for p in piece_paths {
            debug!("Removing chunk file: {}", p);
            if let Err(e) = fs::remove_file(&p) {
                error!("Failed to remove file {}: {}", p, e);
            }
        }
    
        Ok(())
    }


    fn compute_hash(&self, path: &str) -> Result<String> {
        debug!("compute_hash called for {}", path);
        let data = fs::read(path)?;
        debug!("Read {} bytes from {}", data.len(), path);
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let result = hasher.finalize();
        let hash_str = format!("{:x}", result);
        debug!("Hash for {}: {}", path, hash_str);
        Ok(hash_str)
    }

    fn update_metadata(&self, metadata: &mut Metadata, hash: &str, filename: &str, status: &str, output_file: Option<&str>) -> Result<()> {
        debug!("update_metadata called for hash={} status={} output_file={:?}", hash, status, output_file);
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let meta = ImageMetadata {
            hash: hash.to_string(),
            filename: filename.to_string(),
            status: status.to_string(),
            output_file: output_file.map(|s| s.to_string()),
            last_updated: now,
        };

        metadata.upsert_image(meta);
        metadata.save(&self.metadata_path)?;
        debug!("Metadata updated and saved");
        Ok(())
    }

    pub async fn process_all_images_in_dir(&self, dir: &str) -> Result<()> {
        info!("process_all_images_in_dir called for {}", dir);
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "png" || ext == "jpg" || ext == "jpeg" {
                    info!("Found image file: {:?}", path);
                    if let Err(e) = self.process_image(path.to_str().unwrap()).await {
                        error!("Error processing {:?}: {}", path, e);
                    }
                } else {
                    debug!("Skipping non-image file: {:?}", path);
                }
            } else {
                debug!("Skipping non-file entry: {:?}", path);
            }
        }
        info!("Completed process_all_images_in_dir for {}", dir);
        Ok(())
    }
}

use sha2::{Sha256, Digest};

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug)]
struct ImageAnalysisResult {
    extracted_text: String,
    extracted_code: String,
}



use image::GenericImageView;
