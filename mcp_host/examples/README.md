# Smiley-Delimited Tool Calling

This module implements a simpler client approach for tool calling that works across different AI models without requiring specific API features like function calling.

## Overview

Instead of using model-specific function calling APIs, this implementation uses prompt engineering to instruct the AI to format its tool calls as JSON objects delimited by exactly 14 smiley emojis (😊).

Key benefits:
- Works with any text-generating AI model
- No need for specialized API features
- Cross-model compatibility
- Easy to parse and validate

## How It Works

1. **System Prompt**: The AI is instructed to use a specific format for tool calls:
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

2. **Parser**: The `SmileyToolParser` extracts and validates tool calls from the AI's responses.

3. **Tool Execution**: Each tool call is executed and the results are fed back to the AI.

4. **Continued Conversation**: The AI can make multiple tool calls in a single response, or across multiple responses.

## Components

- `smiley_tool_parser.rs`: Handles parsing of smiley-delimited JSON tool calls
- `conversation_service.rs`: Updated to handle tool calls with the new format
- `examples/smiley_delimiter_example.rs`: Example implementation

## Example Usage

Run the example to see it in action:

```bash
# Set your API key
export ANTHROPIC_API_KEY=your_key_here
# or
export OPENAI_API_KEY=your_key_here

# Run the example
cargo run --example smiley_delimiter_example
```

## Integration with MCP

The smiley-delimited approach integrates seamlessly with the existing MCP tool ecosystem:

1. Tool specifications are unchanged
2. Tool execution flows through the same MCP host system
3. Only the parsing of tool calls from AI responses is different
4. Existing tools work without modification

## Benefits

- **Simplicity**: No need for complex API requirements
- **Portability**: Works with any AI model
- **Control**: Custom validation and error handling
- **Extensibility**: Can be extended to other formats if needed

## Implementation Details

The core of the implementation is in `smiley_tool_parser.rs` which:
1. Scans for the exact smiley pattern 😊😊😊😊😊😊😊😊😊😊😊😊😊😊
2. Extracts JSON content between delimiters
3. Validates the JSON has required fields (`name` and `arguments`)
4. Returns structured `ToolCall` objects

Multiple tool calls can be included in a single AI response, each with its own smiley delimiters.