# Smiley-Delimited Tool Calling

This document describes the smiley-delimited tool calling approach implemented in MCP host.

## Overview

Instead of relying on API-specific function calling features, the smiley-delimited approach uses prompt engineering to instruct AI models to format their tool calls in a specific way - using exactly 14 smiley emojis (😊) to delimit JSON tool call requests.

## Benefits

1. **Cross-Model Compatibility**: Works with any text-generating AI model that can follow instructions
2. **No API Dependencies**: Doesn't require specific function calling capabilities
3. **Easy Integration**: Simple to implement and integrate with existing systems
4. **Multiple Tools**: Supports multiple tool calls in a single response
5. **Mixed Content**: Allows the AI to mix normal text with tool calls

## Format

Tool calls are formatted as JSON objects delimited by exactly 14 smiley emojis:

```
😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "tool_name",
  "arguments": {
    "arg1": "value1",
    "arg2": "value2"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊
```

## Implementation

The implementation consists of several components:

### 1. System Prompt Generator

The `generate_smiley_tool_system_prompt` function in `conversation_service.rs` creates a system prompt that instructs the AI about:

- The available MCP tools (names, descriptions, and expected arguments)
- The precise format of tool requests using the smiley delimiters
- The expected JSON structure for tool calls

### 2. Parser

The `SmileyToolParser` in `smiley_tool_parser.rs`:

- Scans AI responses for the exact smiley delimiter pattern
- Extracts the JSON content between delimiters
- Validates the JSON syntax and required fields
- Returns structured `ToolCall` objects

### 3. Conversation Service Integration

The `handle_assistant_response` function in `conversation_service.rs` has been updated to:

- Process responses for smiley-delimited tool calls
- Execute multiple tool calls in sequence
- Feed tool results back to the AI
- Allow the AI to make additional tool calls or provide a final response

### 4. REPL Integration

The REPL system has been modified to:

- Use the smiley-delimited system prompt in chat mode
- Parse and execute tool calls from AI responses
- Support multiple tool calls in a single response

## Usage Example

In a conversation with an AI, the smiley-delimited approach allows for natural interaction:

**User**: What's 123 * 456? Also, what's the weather in Paris?

**AI**: I'll help you with both of your questions.

Let me calculate 123 * 456 first:

😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "calculator",
  "arguments": {
    "expression": "123 * 456"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊

Now let me check the weather in Paris:

😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "weather",
  "arguments": {
    "location": "Paris"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊

## Configuration

No special configuration is required to use the smiley-delimited approach. It's automatically integrated into the MCP host system when entering chat mode with a server.

## Limitations

1. **Instruction Following**: Relies on the AI model's ability to follow format instructions
2. **Token Usage**: The delimiter format consumes additional tokens
3. **Error Recovery**: Requires robust handling for incorrectly formatted tool calls

## Future Improvements

1. **Format Variations**: Support for different delimiter patterns or formats
2. **Better Error Handling**: More sophisticated recovery for malformed tool calls
3. **Token Optimization**: Reduce token usage while maintaining reliability
4. **Streaming Support**: Better handling of streaming responses with tool calls