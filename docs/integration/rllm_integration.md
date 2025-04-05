# RLLM Integration Guide

This document provides a comprehensive guide on how to use the RLLM (Rust LLM) crate in the MCP project. RLLM is a Rust library that lets you use multiple LLM backends in a single project through a unified API.

## Overview

RLLM provides a unified interface to interact with various LLM providers, including:
- OpenAI (GPT models)
- Anthropic (Claude models)
- Ollama (local LLMs)
- DeepSeek
- xAI/Grok
- Phind
- Groq
- Google/Gemini

RLLM is now the default backend for all model providers.

## Setup

### 1. Dependencies

The RLLM dependency is defined in the workspace Cargo.toml with all necessary features:

```toml
# In workspace Cargo.toml
[workspace.dependencies]
rllm = { version = "1.1.7", features = ["openai", "anthropic", "ollama", "deepseek", "xai", "phind", "google"] }
```

### 2. Usage in mcp_host

The mcp_host crate includes RLLM as a standard dependency:

```toml
# In mcp_host/Cargo.toml
[dependencies]
rllm = { workspace = true }
```

## Components

The integration consists of two main components:

1. **RLLM Adapter (`rllm_adapter.rs`)**: A bridge between the MCP `AIClient` interface and the RLLM library.
2. **AI Client Factory**: Uses RLLM for all supported providers.

### RLLM Adapter

The `rllm_adapter.rs` module provides:

- `RLLMClient`: Implements the MCP `AIClient` interface using RLLM.
- `RLLMRequestBuilder`: Implements the MCP `AIRequestBuilder` interface to construct and execute requests.

Key features:
- Model capabilities reporting tailored to each backend type
- Support for image inputs (when the model supports them)
- Support for system messages
- Configuration mapping for temperature, max_tokens, etc.
- Error handling and detailed logging

### AI Client Factory

The `AIClientFactory` in `ai_client.rs` uses RLLM for all providers:
- Creates clients for all RLLM-supported backends
- Provides consistent behavior across all model providers
- Uses optimized implementations for all models

## Usage

### Using RLLM

RLLM is now enabled by default for all builds:

```bash
cargo build
```

### Creating Clients

Create clients through the factory:

```rust
// OpenAI
let openai_config = serde_json::json!({
    "api_key": "YOUR_OPENAI_API_KEY",
    "model": "gpt-4o"
});
let openai_client = AIClientFactory::create("openai", openai_config)?;

// Anthropic
let anthropic_config = serde_json::json!({
    "api_key": "YOUR_ANTHROPIC_API_KEY",
    "model": "claude-3-opus-20240229"
});
let anthropic_client = AIClientFactory::create("anthropic", anthropic_config)?;

// Ollama (local)
let ollama_config = serde_json::json!({
    "model": "llama3"
});
let ollama_client = AIClientFactory::create("ollama", ollama_config)?;
```

### Supported Providers

The following providers are all supported using RLLM:

| Provider | Config Keys | Default Model |
|----------|-------------|---------------|
| openai | api_key, model | gpt-4o-mini |
| anthropic | api_key, model | claude-3-haiku-20240307 |
| gemini | api_key, model | gemini-1.5-pro |
| ollama | model, endpoint | llama3 |
| deepseek | api_key, model | deepseek-chat |
| xai | api_key, model | grok-2-latest |
| phind | api_key, model | Phind-70B |
| groq | api_key, model | llama3-8b-8192 |

## Model Capabilities

Each model backend reports different capabilities:

| Backend | Images | System Messages | Function Calling | Vision | JSON Mode |
|---------|--------|----------------|------------------|--------|-----------|
| OpenAI | ✓ | ✓ | ✓ | ✓ | ✓ |
| Anthropic | ✓ | ✓ | ✓ | ✓ | ✓ |
| Ollama | ✗ | ✓ | ✗ | ✗ | ✗ |
| DeepSeek | ✗ | ✓ | ✗ | ✗ | ✓ |
| XAI/Grok | ✓ | ✓ | ✓ | ✓ | ✓ |
| Phind | ✗ | ✓ | ✗ | ✗ | ✓ |
| Groq | ✗ | ✓ | ✗ | ✗ | ✓ |
| Google/Gemini | ✓ | ✓ | ✓ | ✓ | ✓ |

*Note: Actual capabilities may vary depending on the specific model chosen.*

## Testing

Tests for the RLLM adapter are included in two locations:
- Basic adapter tests in `rllm_adapter.rs`
- Factory integration tests in `ai_client.rs`

To run tests:

```bash
cargo test
```

## Limitations and Considerations

1. **API Keys**: You need valid API keys for the respective services.
2. **Ollama**: For Ollama, you need to have Ollama running locally (default: http://localhost:11434).
3. **Model Support**: Not all models support all features (e.g., vision, function calling).
4. **Rate Limits**: Be mindful of rate limits when using cloud-based providers.
5. **Error Handling**: The adapter includes robust error handling but may need refinement for specific use cases.

## Future Improvements

1. **Streaming Support**: Enhance streaming response support
2. **Function Calling**: Improve function calling integration with the MCP tool system
3. **Caching**: Implement token-based caching to reduce API calls
4. **Advanced Configuration**: Support more provider-specific configuration options
5. **Performance Optimization**: Optimize request handling for high-throughput scenarios

## References

- [RLLM GitHub Repository](https://github.com/graniet/rllm)
- [RLLM Crate Documentation](https://crates.io/crates/rllm)
