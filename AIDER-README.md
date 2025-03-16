# MCP Aider Tool

## Overview

The MCP Aider tool is an AI-powered pair programming assistant integrated into the Model Context Protocol (MCP) framework. It allows AI assistants like Claude to make targeted code changes to repositories by leveraging specialized AI models designed for code understanding and generation.

## Enhanced Multi-Provider Support

The Aider tool has been enhanced to support multiple AI providers and models:

- **Anthropic** (Claude models)
- **OpenAI** (GPT models)
- **Gemini** (Google's models)
- **DeepSeek** (DeepSeek's code models)

## Key Features

### Provider Selection
- Specify which AI provider to use for code assistance
- Each provider has optimized prompting and interaction patterns

### Model Configuration
- Provider-specific model defaults
- Ability to override with specific model versions
- Automatic fallback to appropriate defaults

### Thinking/Reasoning Capabilities
- Enhanced prompting to encourage step-by-step reasoning
- Support for models with different capabilities and strengths
- Optimized context handling for each provider

### Streaming Support
- Real-time response streaming where supported
- Progress indicators for long-running operations

## Usage Examples

### Basic Usage
```json
{
  "directory": "/path/to/your/repo",
  "instruction": "Add error handling to the main function"
}
```

### Specifying Provider
```json
{
  "directory": "/path/to/your/repo",
  "instruction": "Refactor the authentication module",
  "provider": "anthropic"
}
```

### Using a Specific Model
```json
{
  "directory": "/path/to/your/repo",
  "instruction": "Optimize the database queries",
  "provider": "openai",
  "model": "gpt-4o"
}
```

### With Thinking/Reasoning Enabled
```json
{
  "directory": "/path/to/your/repo",
  "instruction": "Implement pagination for the API",
  "provider": "gemini",
  "enable_reasoning": true
}
```

## Environment Configuration

The tool requires appropriate API keys set in environment variables:

- `ANTHROPIC_API_KEY` - For Claude models
- `OPENAI_API_KEY` - For GPT models
- `GEMINI_API_KEY` - For Gemini models
- `DEEPSEEK_API_KEY` - For DeepSeek models

## Default Models

Each provider has a sensible default model:

- Anthropic: `claude-3-opus-20240229`
- OpenAI: `gpt-4o-mini`
- Gemini: `gemini-pro`
- DeepSeek: `deepseek-coder`
