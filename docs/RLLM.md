# RLLM (Rust LLM) Crate Documentation

RLLM is a unified interface for interacting with Large Language Model providers in Rust. This crate provides a consistent API for working with different LLM backends by abstracting away provider-specific implementation details.

**Current Version:** 1.1.7

## Overview

RLLM lets you use multiple LLM backends in a single project, including:
- OpenAI (GPT models)
- Anthropic (Claude models)
- Ollama (local LLM provider)
- DeepSeek
- Google (Gemini)
- xAI (formerly Twitter)
- Phind (code-specialized models)
- Groq

## Key Features

- **Multi-backend**: Manage multiple LLM providers through a single unified API
- **Multi-step chains**: Create complex chains with different backends at each step
- **Templates**: Create complex prompts with variables
- **Builder pattern**: Configure your LLM with a fluent interface
- **Chat & Completions**: Unified traits for most use cases
- **Extensible**: Easily add new backends
- **Validation**: Add validation to ensure expected output
- **Evaluation**: Score and evaluate LLM outputs
- **Parallel Evaluation**: Test multiple providers and select the best response
- **Function calling**: Add tool usage to your LLM requests
- **REST API**: Serve any LLM backend with OpenAI-compatible API
- **Vision**: Support for image input in LLM requests
- **Reasoning**: Add reasoning capabilities to your LLM requests

## Installation

Add RLLM to your `Cargo.toml`:

```toml
[dependencies]
rllm = { version = "1.1.7", features = ["openai", "anthropic", "ollama"] }
```

## Core Architecture

The crate is organized into modules that handle different aspects of LLM interactions:

### Modules

| Module | Description |
|--------|-------------|
| `backends` | Backend implementations for supported LLM providers |
| `builder` | Builder pattern for configuring and instantiating LLM providers |
| `chain` | Chain multiple LLM providers together for complex workflows |
| `chat` | Chat-based interactions with language models (e.g., ChatGPT style) |
| `completion` | Text completion capabilities (e.g., GPT-3 style completion) |
| `embedding` | Vector embeddings generation for text |
| `error` | Error types and handling |
| `evaluator` | Evaluator for LLM providers |
| `validated_llm` | Validation wrapper for LLM providers with retry capabilities |

## Core Traits

### LLMProvider

The `LLMProvider` trait is the core interface that all LLM backends implement. It combines chat, completion, and embedding capabilities into a single interface.

```rust
pub trait LLMProvider: ChatProvider + CompletionProvider + EmbeddingProvider {
    // Returns available tools for this provider
    fn tools(&self) -> Option<&[Tool]> { ... }
}
```

#### Implementors
- `Anthropic`
- `DeepSeek`
- `Google`
- `Ollama`
- `OpenAI`
- `Phind`
- `XAI`
- `ValidatedLLM`

### ChatProvider

Trait for providers that support chat-style interactions.

```rust
pub trait ChatProvider: Sync + Send {
    // Required method for chat with tools
    fn chat_with_tools<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        messages: &'life1 [ChatMessage],
        tools: Option<&'life2 [Tool]>,
    ) -> Pin<Box<dyn Future<Output = Result<String, LLMError>> + Send + 'async_trait>>;

    // Provided method for simpler chat (without tools)
    fn chat<'life0, 'life1, 'async_trait>(
        &'life0 self,
        messages: &'life1 [ChatMessage],
    ) -> Pin<Box<dyn Future<Output = Result<String, LLMError>> + Send + 'async_trait>> { ... }
}
```

### CompletionProvider

Trait for providers that support text completion.

```rust
pub trait CompletionProvider: Sync + Send {
    // Completes text based on a prompt
    fn complete<'life0, 'life1, 'async_trait>(
        &'life0 self,
        prompt: &'life1 str,
    ) -> Pin<Box<dyn Future<Output = Result<String, LLMError>> + Send + 'async_trait>>;
}
```

### EmbeddingProvider

Trait for providers that support text embeddings.

```rust
pub trait EmbeddingProvider: Sync + Send {
    // Generates embeddings from text
    fn embed<'life0, 'life1, 'async_trait>(
        &'life0 self,
        text: &'life1 str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<f32>, LLMError>> + Send + 'async_trait>>;
}
```

## Builder Module

The builder module provides a flexible builder pattern for creating and configuring LLM provider instances.

### LLMBuilder

Core builder for configuring and instantiating LLM providers. Provides a fluent interface for setting various configuration options.

```rust
pub struct LLMBuilder {
    /* private fields */
}

impl LLMBuilder {
    // Creates a new empty builder instance with default values
    pub fn new() -> LLMBuilder { ... }

    // Sets the backend provider to use
    pub fn backend(self, backend: LLMBackend) -> LLMBuilder { ... }

    // Sets the API key for authentication
    pub fn api_key(self, key: impl Into<String>) -> LLMBuilder { ... }

    // Sets the base URL for API requests
    pub fn base_url(self, url: impl Into<String>) -> LLMBuilder { ... }

    // Sets the model identifier to use
    pub fn model(self, model: impl Into<String>) -> LLMBuilder { ... }

    // Sets the maximum number of tokens to generate
    pub fn max_tokens(self, max_tokens: u32) -> LLMBuilder { ... }

    // Sets the temperature for controlling response randomness (0.0-1.0)
    pub fn temperature(self, temperature: f32) -> LLMBuilder { ... }

    // Sets the system prompt/context
    pub fn system(self, system: impl Into<String>) -> LLMBuilder { ... }

    // Sets the reasoning effort level
    pub fn reasoning_effort(self, reasoning_effort: ReasoningEffort) -> LLMBuilder { ... }

    // Enables or disables reasoning
    pub fn reasoning(self, reasoning: bool) -> LLMBuilder { ... }

    // Sets the reasoning budget tokens
    pub fn reasoning_budget_tokens(self, reasoning_budget_tokens: u32) -> LLMBuilder { ... }

    // Sets the request timeout in seconds
    pub fn timeout_seconds(self, timeout_seconds: u64) -> LLMBuilder { ... }

    // Enables or disables streaming responses
    pub fn stream(self, stream: bool) -> LLMBuilder { ... }

    // Sets the top-p (nucleus) sampling parameter
    pub fn top_p(self, top_p: f32) -> LLMBuilder { ... }

    // Sets the top-k sampling parameter
    pub fn top_k(self, top_k: u32) -> LLMBuilder { ... }

    // Sets the encoding format for embeddings
    pub fn embedding_encoding_format(
        self,
        embedding_encoding_format: impl Into<String>,
    ) -> LLMBuilder { ... }

    // Sets the dimensions for embeddings
    pub fn embedding_dimensions(self, embedding_dimensions: u32) -> LLMBuilder { ... }

    // Sets a validation function to verify LLM responses
    pub fn validator(self, f: F) -> LLMBuilder
    where
        F: Fn(&str) -> Result<(), String> + Send + Sync + 'static,
    { ... }

    // Sets the number of retry attempts for validation failures
    pub fn validator_attempts(self, attempts: usize) -> LLMBuilder { ... }

    // Adds a function tool to the builder
    pub fn function(self, function_builder: FunctionBuilder) -> LLMBuilder { ... }

    // Builds and returns a configured LLM provider instance
    pub fn build(self) -> Result<Box<dyn LLMProvider>, LLMError> { ... }
}
```

### LLMBackend

Enum representing the supported LLM backend providers.

```rust
pub enum LLMBackend {
    OpenAI,    // OpenAI API provider (GPT-3, GPT-4, etc.)
    Anthropic, // Anthropic API provider (Claude models)
    Ollama,    // Ollama local LLM provider for self-hosted models
    DeepSeek,  // DeepSeek API provider for their LLM models
    XAI,       // X.AI (formerly Twitter) API provider
    Phind,     // Phind API provider for code-specialized models
    Google,    // Google Gemini API provider
    Groq,      // Groq API provider
}
```

### FunctionBuilder

Builder for function tools.

```rust
pub struct FunctionBuilder {
    /* private fields */
}

impl FunctionBuilder {
    // Creates a new function builder with the specified name
    pub fn new(name: impl Into<String>) -> Self { ... }

    // Sets the description for this function
    pub fn description(self, description: impl Into<String>) -> Self { ... }

    // Adds a parameter to this function
    pub fn param(self, param_builder: ParamBuilder) -> Self { ... }

    // Builds the function into a Tool
    pub fn build(self) -> Tool { ... }
}
```

### ParamBuilder

Builder for function parameters.

```rust
pub struct ParamBuilder {
    /* private fields */
}

impl ParamBuilder {
    // Creates a new parameter builder with the specified name
    pub fn new(name: impl Into<String>) -> Self { ... }

    // Sets the type of this parameter
    pub fn type_(self, type_: impl Into<String>) -> Self { ... }

    // Sets the description for this parameter
    pub fn description(self, description: impl Into<String>) -> Self { ... }

    // Sets whether this parameter is required
    pub fn required(self, required: bool) -> Self { ... }

    // Builds the parameter into a ParameterProperty
    pub fn build(self) -> ParameterProperty { ... }
}
```

## Chat Module

Module for chat-based interactions with language models.

### ChatMessage

Represents a single message in a chat conversation.

```rust
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub name: Option<String>,
    pub message_type: MessageType,
}
```

### ChatMessageBuilder

Builder for chat messages.

```rust
pub struct ChatMessageBuilder {
    /* private fields */
}

impl ChatMessageBuilder {
    // Creates a new builder for a user message
    pub fn user(content: impl Into<String>) -> Self { ... }

    // Creates a new builder for an assistant message
    pub fn assistant(content: impl Into<String>) -> Self { ... }

    // Creates a new builder for a system message
    pub fn system(content: impl Into<String>) -> Self { ... }

    // Sets the message name (for function calls)
    pub fn name(self, name: impl Into<String>) -> Self { ... }

    // Sets the message type (text, image, etc.)
    pub fn message_type(self, message_type: MessageType) -> Self { ... }

    // Builds the chat message
    pub fn build(self) -> ChatMessage { ... }
}
```

### ChatRole

Enum representing the role of a message in a conversation.

```rust
pub enum ChatRole {
    System,
    User,
    Assistant,
    Function,
    Tool,
}
```

### MessageType

Enum representing the type of content in a message.

```rust
pub enum MessageType {
    Text,
    Image(String),
    FunctionCall { name: String, arguments: String },
    ToolCall(ToolCall),
}
```

### Tool

Represents a tool that can be used by the LLM during chat.

```rust
pub struct Tool {
    pub r#type: String,
    pub function: FunctionTool,
}
```

### FunctionTool

Represents a function tool that can be called by the LLM.

```rust
pub struct FunctionTool {
    pub name: String,
    pub description: Option<String>,
    pub parameters: ParametersSchema,
}
```

### ParametersSchema

Schema for function parameters.

```rust
pub struct ParametersSchema {
    pub r#type: String,
    pub properties: HashMap<String, ParameterProperty>,
    pub required: Vec<String>,
}
```

### ParameterProperty

Property of a parameter in a function.

```rust
pub struct ParameterProperty {
    pub r#type: String,
    pub description: Option<String>,
}
```

## Chain Module

Module for chaining multiple LLM providers together for complex workflows.

### Chain

Core struct for chaining LLM operations together.

```rust
pub struct Chain {
    /* private fields */
}

impl Chain {
    // Creates a new empty chain
    pub fn new() -> Self { ... }

    // Adds a new step to the chain
    pub fn then<F, Fut, T>(mut self, f: F) -> Self
    where
        F: FnOnce(String) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, LLMError>> + Send,
        T: Into<String> + Send,
    { ... }

    // Executes the chain with the given input
    pub async fn execute(self, input: impl Into<String>) -> Result<String, LLMError> { ... }
}
```

## Evaluator Module

Module for evaluating and comparing responses from multiple LLM providers.

### Evaluator

Core struct for evaluating LLM providers.

```rust
pub struct Evaluator<'a> {
    /* private fields */
}

impl<'a> Evaluator<'a> {
    // Creates a new evaluator
    pub fn new() -> Self { ... }

    // Adds a provider to evaluate
    pub fn add_provider(
        &mut self,
        name: impl Into<String>,
        provider: &'a dyn LLMProvider,
    ) -> &mut Self { ... }

    // Adds a scoring function
    pub fn add_scorer(
        &mut self,
        name: impl Into<String>,
        weight: f32,
        scorer: impl Fn(&str) -> f32 + Send + Sync + 'static,
    ) -> &mut Self { ... }

    // Evaluates all providers with the given input
    pub async fn evaluate(&self, input: &[ChatMessage]) -> Result<EvaluationResult, LLMError> { ... }
}
```

### EvaluationResult

Results of an evaluation run.

```rust
pub struct EvaluationResult {
    pub scores: HashMap<String, HashMap<String, f32>>,
    pub total_scores: HashMap<String, f32>,
    pub best_provider: String,
    pub responses: HashMap<String, String>,
}
```

## ValidatedLLM Module

Module for validating LLM responses and implementing retry logic.

### ValidatedLLM

A wrapper around an LLM provider that applies validation and retry logic.

```rust
pub struct ValidatedLLM {
    /* private fields */
}

impl ValidatedLLM {
    // Creates a new validated LLM
    pub fn new(
        provider: Box<dyn LLMProvider>,
        validator: Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>,
        max_attempts: usize,
    ) -> Self { ... }
}
```

## Usage Examples

### Basic Chat Example

```rust
use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chat::{ChatMessageBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an instance of an LLM provider using the builder pattern
    let llm = LLMBuilder::new()
        .backend(LLMBackend::OpenAI)
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .model("gpt-4")
        .temperature(0.7)
        .max_tokens(1000)
        .build()?;
    
    // Create a chat conversation
    let messages = vec![
        ChatMessageBuilder::system("You are a helpful assistant.").build(),
        ChatMessageBuilder::user("What is the capital of France?").build(),
    ];
    
    // Send the chat request
    let response = llm.chat(&messages).await?;
    
    println!("Response: {}", response);
    
    Ok(())
}
```

### Using Function Calling

```rust
use rllm::builder::{FunctionBuilder, LLMBackend, LLMBuilder, ParamBuilder};
use rllm::chat::{ChatMessageBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define a function tool for weather information
    let get_weather = FunctionBuilder::new("get_weather")
        .description("Get the current weather in a location")
        .param(
            ParamBuilder::new("location")
                .type_("string")
                .description("The city and state, e.g. San Francisco, CA")
                .required(true)
                .build()
        )
        .build();
    
    // Create an LLM instance with the function
    let llm = LLMBuilder::new()
        .backend(LLMBackend::OpenAI)
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .model("gpt-4")
        .function(get_weather)
        .build()?;
    
    // Create a chat conversation
    let messages = vec![
        ChatMessageBuilder::user("What's the weather like in Paris?").build(),
    ];
    
    // Send the chat request with tools
    let response = llm.chat_with_tools(&messages, None).await?;
    
    println!("Response: {}", response);
    
    Ok(())
}
```

### Using Validation

```rust
use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chat::{ChatMessageBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an LLM instance with validation
    let llm = LLMBuilder::new()
        .backend(LLMBackend::OpenAI)
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .model("gpt-4")
        .validator(|response| {
            // Ensure the response contains a date
            if response.contains("2023") || response.contains("2024") || response.contains("2025") {
                Ok(())
            } else {
                Err("Response must contain a year".to_string())
            }
        })
        .validator_attempts(3)
        .build()?;
    
    // Create a chat conversation
    let messages = vec![
        ChatMessageBuilder::user("When was the Eiffel Tower built?").build(),
    ];
    
    // Send the chat request
    let response = llm.chat(&messages).await?;
    
    println!("Response: {}", response);
    
    Ok(())
}
```

### Using Chains

```rust
use rllm::builder::{LLMBackend, LLMBuilder};
use rllm::chain::Chain;
use rllm::chat::{ChatMessageBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create two LLM instances
    let summarizer = LLMBuilder::new()
        .backend(LLMBackend::OpenAI)
        .api_key(std::env::var("OPENAI_API_KEY")?)
        .model("gpt-3.5-turbo")
        .build()?;
    
    let analyzer = LLMBuilder::new()
        .backend(LLMBackend::Anthropic)
        .api_key(std::env::var("ANTHROPIC_API_KEY")?)
        .model("claude-3-opus-20240229")
        .build()?;
    
    // Create a chain that summarizes text and then analyzes the summary
    let result = Chain::new()
        .then(|input| {
            let messages = vec![
                ChatMessageBuilder::system("Summarize the following text concisely:").build(),
                ChatMessageBuilder::user(input).build(),
            ];
            summarizer.chat(&messages)
        })
        .then(|summary| {
            let messages = vec![
                ChatMessageBuilder::system("Analyze the key themes in this summary:").build(),
                ChatMessageBuilder::user(summary).build(),
            ];
            analyzer.chat(&messages)
        })
        .execute("Long text to summarize and analyze...")
        .await?;
    
    println!("Result: {}", result);
    
    Ok(())
}
```

## Error Handling

The crate uses a unified `LLMError` type for error handling:

```rust
pub enum LLMError {
    ApiError(String),
    AuthenticationError(String),
    ConfigurationError(String),
    NetworkError(String),
    RateLimitError(String),
    InvalidResponseError(String),
    ValidationError(String),
    ChainError(String),
    UnknownError(String),
}
```

## Notes

- RLLM is a wrapper around the [llm](https://github.com/graniet/llm) crate, providing the same features.
- Different backends are enabled via feature flags in Cargo.toml.
- The crate is designed with a unified API that abstracts away the differences between providers.

For more detailed information about specific modules and functions, refer to the [official documentation](https://docs.rs/rllm/latest/rllm/).