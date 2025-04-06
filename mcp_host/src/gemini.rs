use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use log::{debug, error};
use crate::ai_client::{AIClient, AIRequestBuilder, GenerationConfig};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::path::Path;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContentPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "inlineData", skip_serializing_if = "Option::is_none")]
    inline_data: Option<GeminiInlineData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    role: String,
    parts: Vec<GeminiContentPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetySetting {
    category: String,
    threshold: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSystemInstruction {
    parts: Vec<GeminiContentPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(rename = "safetySettings", skip_serializing_if = "Option::is_none")]
    safety_settings: Option<Vec<SafetySetting>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiGenerationConfig {
    temperature: Option<f32>,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
    #[serde(rename = "topP")]
    top_p: Option<f32>,
    #[serde(rename = "responseModalities")]
    response_modalities: Vec<String>,
}

impl Default for GeminiGenerationConfig {
    fn default() -> Self {
        Self {
            temperature: None,
            max_output_tokens: None,
            top_p: None,
            response_modalities: vec!["TEXT".to_string()],
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeminiClient {
    api_key: String,
    endpoint: String,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            endpoint: format!("https://us-central1-aiplatform.googleapis.com/v1/projects/liminalnook/locations/us-central1/publishers/google/models/{}:streamGenerateContent", model),
        }
    }

    fn default_safety_settings() -> Vec<SafetySetting> {
        vec![
            SafetySetting {
                category: "HARM_CATEGORY_HATE_SPEECH".to_string(),
                threshold: "OFF".to_string(),
            },
            SafetySetting {
                category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(),
                threshold: "OFF".to_string(),
            },
            SafetySetting {
                category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(),
                threshold: "OFF".to_string(),
            },
            SafetySetting {
                category: "HARM_CATEGORY_HARASSMENT".to_string(),
                threshold: "OFF".to_string(),
            },
        ]
    }
}

#[async_trait]
impl<'a> AIClient for GeminiClient {
    fn model_name(&self) -> String {
        self.endpoint
            .split('/')
            .last()
            .and_then(|s| s.split(':').next())
            .unwrap_or("gemini-pro")
            .to_string()
    }

    fn builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(GeminiCompletionBuilder {
            client: self.clone(),
            system_instruction: None,
            contents: Vec::new(),
            generation_config: None,
        })
    }

    fn raw_builder(&self) -> Box<dyn AIRequestBuilder> {
        Box::new(GeminiCompletionBuilder {
            client: self.clone(),
            system_instruction: None,
            contents: Vec::new(),
            generation_config: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct GeminiCompletionBuilder {
    client: GeminiClient,
    contents: Vec<GeminiContent>,
    system_instruction: Option<GeminiSystemInstruction>,
    generation_config: Option<GeminiGenerationConfig>,
}

#[async_trait] // Ensure async_trait is applied to the impl block
impl AIRequestBuilder for GeminiCompletionBuilder {
    fn system(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        let system_instruction = GeminiSystemInstruction {
            parts: vec![GeminiContentPart {
                text: Some(content),
                inline_data: None,
            }],
        };
        let builder = &mut *self;
        builder.system_instruction = Some(system_instruction);
        Box::new(builder.clone())
    }

    fn user(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.contents.push(GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiContentPart {
                text: Some(content),
                inline_data: None,
            }],
        });
        self
    }

    fn user_with_image(mut self: Box<Self>, text: String, image_path: &Path) -> Result<Box<dyn AIRequestBuilder>> {
        let image_data = fs::read(image_path)?;
        let base64_image = BASE64.encode(&image_data);

        self.contents.push(GeminiContent {
            role: "user".to_string(),
            parts: vec![
                GeminiContentPart {
                    text: Some(text),
                    inline_data: None,
                },
                GeminiContentPart {
                    text: None,
                    inline_data: Some(GeminiInlineData {
                        mime_type: "image/jpeg".to_string(),
                        data: base64_image,
                    }),
                },
            ],
        });
        Ok(self)
    }

    fn user_with_image_url(mut self: Box<Self>, text: String, image_url: String) -> Box<dyn AIRequestBuilder> {
        self.contents.push(GeminiContent {
            role: "user".to_string(),
            parts: vec![
                GeminiContentPart {
                    text: Some(text),
                    inline_data: None,
                },
                GeminiContentPart {
                    text: None,
                    inline_data: Some(GeminiInlineData {
                        mime_type: "image/jpeg".to_string(),
                        data: image_url,
                    }),
                },
            ],
        });
        self
    }

    fn assistant(mut self: Box<Self>, content: String) -> Box<dyn AIRequestBuilder> {
        self.contents.push(GeminiContent {
            role: "model".to_string(),
            parts: vec![GeminiContentPart {
                text: Some(content),
                inline_data: None,
            }],
        });
        self
    }

    fn config(mut self: Box<Self>, config: GenerationConfig) -> Box<dyn AIRequestBuilder> {
        self.generation_config = Some(GeminiGenerationConfig {
            temperature: config.temperature,
            max_output_tokens: config.max_tokens,
            top_p: Some(0.95),
            response_modalities: vec!["TEXT".to_string()],
        });
        self
    }

    async fn execute(self: Box<Self>) -> Result<String> {
        let mut config = self.generation_config.unwrap_or_default();
        if config.top_p.is_none() {
            config.top_p = Some(0.95);
        }
    
        let request = GeminiRequest {
            contents: self.contents,
            system_instruction: self.system_instruction.clone(),
            generation_config: Some(config),
            safety_settings: Some(GeminiClient::default_safety_settings()),
        };
    
        debug!("Sending request to Gemini API");
        let client = reqwest::Client::new();
        let response = client
            .post(&self.client.endpoint)
            .header("Content-Type", "application/json") 
            .header("Authorization", format!("Bearer {}", &self.client.api_key))
            .json(&request)
            .send()
            .await?;
    
        debug!("Response received, status: {}", response.status());
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.context("Failed to read error response")?;
            error!("API error response: {}", error_text);
            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_text));
        }
    
        let response_text = response.text().await?;
        debug!("Full API response: {}", response_text);
    
        // Parse the full response as an array of streamed responses
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;
        debug!("Parsed JSON response: {:#?}", response_json);
    
        let arr = response_json.as_array()
            .context("Expected the response to be an array of streamed chunks")?;
    
        let mut full_text = String::new();
    
        for chunk in arr {
            // Navigate to candidates -> [0] -> content -> parts -> [0] -> text
            if let Some(text_val) = chunk.pointer("/candidates/0/content/parts/0/text") {
                if let Some(text_str) = text_val.as_str() {
                    full_text.push_str(text_str);
                }
            }
    
            // Check if finishReason is STOP
            if let Some(finish_reason) = chunk.pointer("/candidates/0/finishReason") {
                if finish_reason == "STOP" {
                    break;
                }
            }
        }
    
        debug!("Final combined text: {}", full_text);
        Ok(full_text)
    }
}

impl GeminiCompletionBuilder {
    // pub fn user(mut self, content: impl Into<String>) -> Self {
    //     let c = content.into();
    //     debug!("GeminiCompletionBuilder: Adding user message: {}", c);
    //     self.contents.push(GeminiContent {
    //         role: "user".to_string(),
    //         parts: vec![GeminiContentPart {
    //             text: Some(c),
    //             inline_data: None,
    //         }],
    //     });
    //     self
    // }

    pub fn user_with_image(mut self, text: impl Into<String>, image_path: impl AsRef<Path>) -> Result<Self> {
        let t = text.into();
        debug!("GeminiCompletionBuilder: Adding user message with image from path: {}", image_path.as_ref().display());
        let image_data = fs::read(image_path)?;
        let base64_image = BASE64.encode(&image_data);

        let mut parts = vec![GeminiContentPart {
            text: Some(t),
            inline_data: None,
        }];

        parts.push(GeminiContentPart {
            text: None,
            inline_data: Some(GeminiInlineData {
                mime_type: "image/jpeg".to_string(),
                data: base64_image,
            }),
        });

        self.contents.push(GeminiContent {
            role: "user".to_string(),
            parts,
        });
        Ok(self)
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        debug!("GeminiCompletionBuilder: Setting temperature to {}", temp);
        let config = self.generation_config.get_or_insert(GeminiGenerationConfig::default());
        config.temperature = Some(temp);
        self
    }

    pub fn max_tokens(mut self, tokens: u32) -> Self {
        debug!("GeminiCompletionBuilder: Setting max_tokens to {}", tokens);
        let config = self.generation_config.get_or_insert(GeminiGenerationConfig::default());
        config.max_output_tokens = Some(tokens);
        self
    }

    pub async fn execute(self) -> Result<String> {
        debug!("GeminiCompletionBuilder.execute called");
        let mut config = self.generation_config.unwrap_or_default();
        if config.top_p.is_none() {
            config.top_p = Some(0.95);
        }
        
        let request = GeminiRequest {
            contents: self.contents,
            system_instruction: self.system_instruction.clone(),
            generation_config: Some(config),
            safety_settings: Some(GeminiClient::default_safety_settings()),
        };

        debug!("Sending request to Gemini API with payload: {:#?}", request);
        let client = reqwest::Client::new();
        let response = client
            .post(&self.client.endpoint)
            .header("Content-Type", "application/json")
            .query(&[("key", &self.client.api_key)])
            .json(&request)
            .send()
            .await?;

        debug!("Response received, status: {}", response.status());
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.context("Failed to read error response")?;
            error!("API error response: {}", error_text);
            return Err(anyhow::anyhow!("API request failed with status {}: {}", status, error_text));
        }

        let response_text = response.text().await?;
        debug!("Full API response: {}", response_text);

        // Parse response and extract text
        let response_json: serde_json::Value = serde_json::from_str(&response_text)?;
        debug!("Parsed JSON response: {:#?}", response_json);

        let content = response_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .context("Failed to get text from response")?;

        debug!("Extracted content: {}", content);
        Ok(content.to_string())
    }
}
