# Model Context Protocol (MCP) Implementation Guide

This document provides comprehensive guidance for developers looking to implement the Model Context Protocol (MCP) from scratch. MCP defines a standardized way for AI models to interact with tools, resources, and capabilities through a JSON-RPC based communication protocol.

## Core Protocol Concepts

MCP is built around JSON-RPC 2.0 with these key components:

1. **Server & Client Roles**: MCP defines both client and server roles, with bidirectional capabilities where each party can issue requests to the other.

2. **JSON-RPC Messages**: All communication uses JSON-RPC 2.0 format with requests, responses, notifications, and errors.

3. **Initialization Flow**: A required handshake process to establish capabilities and protocol version.

4. **Tools System**: A standardized way to expose executable capabilities to AI models.

5. **Resources System**: A mechanism for sharing readable content with AI models.

6. **Prompts System**: A way to define and template prompts for consistent model interaction.

## Protocol Versions

The latest protocol version is `2025-03-26`. Implementations should handle version negotiation during initialization.

## Message Structure

### JSON-RPC Request

```json
{
  "jsonrpc": "2.0",
  "id": "request-id-1",
  "method": "method/name",
  "params": {
    "_meta": {
      "progressToken": "optional-token-for-progress-updates"
    },
    "paramName1": "value1",
    "paramName2": "value2"
  }
}
```

### JSON-RPC Response

```json
{
  "jsonrpc": "2.0",
  "id": "request-id-1",
  "result": {
    "_meta": {
      "key": "optional-metadata"
    },
    "resultKey1": "value1",
    "resultKey2": "value2"
  }
}
```

### JSON-RPC Error

```json
{
  "jsonrpc": "2.0",
  "id": "request-id-1",
  "error": {
    "code": -32603,
    "message": "Internal error",
    "data": {
      "details": "Additional error information"
    }
  }
}
```

### JSON-RPC Notification

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/name",
  "params": {
    "key1": "value1"
  }
}
```

## Standard Error Codes

- `-32700`: Parse error
- `-32600`: Invalid request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error

## Initialization Flow

### Step 1: Client sends initialize request

```json
{
  "jsonrpc": "2.0",
  "id": "init-1",
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-03-26",
    "clientInfo": {
      "name": "MyMCPClient",
      "version": "1.0.0"
    },
    "capabilities": {
      "sampling": {},
      "roots": {
        "listChanged": true
      }
    }
  }
}
```

### Step 2: Server responds with capabilities

```json
{
  "jsonrpc": "2.0",
  "id": "init-1",
  "result": {
    "protocolVersion": "2025-03-26",
    "serverInfo": {
      "name": "MyMCPServer",
      "version": "1.0.0"
    },
    "capabilities": {
      "tools": {
        "listChanged": true
      },
      "resources": {
        "subscribe": true,
        "listChanged": true
      },
      "prompts": {
        "listChanged": true
      },
      "logging": {}
    },
    "instructions": "This server provides tools for web search, bash commands, and document generation."
  }
}
```

### Step 3: Client sends initialized notification

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/initialized"
}
```

## Tool Implementation

### Tools Discovery

To discover available tools, the client sends:

```json
{
  "jsonrpc": "2.0",
  "id": "tools-1",
  "method": "tools/list"
}
```

The server responds with:

```json
{
  "jsonrpc": "2.0",
  "id": "tools-1",
  "result": {
    "tools": [
      {
        "name": "bash",
        "description": "Execute bash shell commands",
        "inputSchema": {
          "type": "object",
          "properties": {
            "command": {
              "type": "string",
              "description": "The command to execute"
            },
            "cwd": {
              "type": "string",
              "description": "Working directory"
            }
          },
          "required": ["command"]
        }
      }
    ]
  }
}
```

### Tool Execution

To call a tool, the client sends:

```json
{
  "jsonrpc": "2.0",
  "id": "call-1",
  "method": "tools/call",
  "params": {
    "name": "bash",
    "arguments": {
      "command": "ls -la",
      "cwd": "/home/user"
    }
  }
}
```

The server responds with:

```json
{
  "jsonrpc": "2.0",
  "id": "call-1",
  "result": {
    "content": [
      {
        "type": "text",
        "text": "total 32\ndrwxr-xr-x 4 user user 4096 Apr  3 10:15 .\ndrwxr-xr-x 3 user user 4096 Apr  3 09:00 ..\n-rw-r--r-- 1 user user  220 Apr  3 09:00 .bash_logout"
      }
    ]
  }
}
```

Error handling in tool calls:

```json
{
  "jsonrpc": "2.0",
  "id": "call-1",
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Error: Command not found"
      }
    ],
    "isError": true
  }
}
```

## Resource Implementation

### Resource Discovery

```json
{
  "jsonrpc": "2.0",
  "id": "res-1",
  "method": "resources/list"
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": "res-1",
  "result": {
    "resources": [
      {
        "uri": "file:///home/user/document.txt",
        "name": "Sample Document",
        "description": "A text file containing sample content",
        "mimeType": "text/plain"
      }
    ]
  }
}
```

### Reading Resources

```json
{
  "jsonrpc": "2.0",
  "id": "read-1",
  "method": "resources/read",
  "params": {
    "uri": "file:///home/user/document.txt"
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": "read-1",
  "result": {
    "contents": [
      {
        "uri": "file:///home/user/document.txt",
        "mimeType": "text/plain",
        "text": "This is the content of the document."
      }
    ]
  }
}
```

## Prompts System

### Prompt Discovery

```json
{
  "jsonrpc": "2.0",
  "id": "prompts-1",
  "method": "prompts/list"
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": "prompts-1",
  "result": {
    "prompts": [
      {
        "name": "summarize",
        "description": "Summarize the provided text",
        "arguments": [
          {
            "name": "text",
            "description": "The text to summarize",
            "required": true
          }
        ]
      }
    ]
  }
}
```

### Getting Prompts

```json
{
  "jsonrpc": "2.0",
  "id": "get-prompt-1",
  "method": "prompts/get",
  "params": {
    "name": "summarize",
    "arguments": {
      "text": "The quick brown fox jumps over the lazy dog."
    }
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": "get-prompt-1",
  "result": {
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "Please summarize the following text:\n\nThe quick brown fox jumps over the lazy dog."
        }
      }
    ]
  }
}
```

## Content Types

MCP supports various content types in messages and responses:

### Text Content

```json
{
  "type": "text",
  "text": "This is text content"
}
```

### Image Content

```json
{
  "type": "image",
  "data": "base64-encoded-image-data",
  "mimeType": "image/png"
}
```

### Audio Content

```json
{
  "type": "audio",
  "data": "base64-encoded-audio-data",
  "mimeType": "audio/wav"
}
```

### Embedded Resource

```json
{
  "type": "resource",
  "resource": {
    "uri": "file:///path/to/resource",
    "text": "Content of the resource"
  }
}
```

## Progress Notifications

For long-running operations, progress updates can be requested:

Request with progress token:

```json
{
  "jsonrpc": "2.0",
  "id": "long-op-1",
  "method": "tools/call",
  "params": {
    "_meta": {
      "progressToken": "progress-1"
    },
    "name": "long-running-task",
    "arguments": {
      "duration": 30
    }
  }
}
```

Progress notification:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/progress",
  "params": {
    "progressToken": "progress-1",
    "progress": 10,
    "total": 30,
    "message": "Task is one-third complete"
  }
}
```

## Cancellation

To cancel an in-flight request:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/cancelled",
  "params": {
    "requestId": "long-op-1",
    "reason": "User interrupted operation"
  }
}
```

## LLM Sampling

When a server needs to use an LLM via the client:

```json
{
  "jsonrpc": "2.0",
  "id": "sample-1",
  "method": "sampling/createMessage",
  "params": {
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "What is the capital of France?"
        }
      }
    ],
    "modelPreferences": {
      "hints": [
        {
          "name": "claude-3"
        }
      ],
      "intelligencePriority": 0.8,
      "speedPriority": 0.4
    },
    "maxTokens": 1000
  }
}
```

Client response:

```json
{
  "jsonrpc": "2.0",
  "id": "sample-1",
  "result": {
    "role": "assistant",
    "content": {
      "type": "text",
      "text": "The capital of France is Paris."
    },
    "model": "claude-3-opus-20240229",
    "stopReason": "endTurn"
  }
}
```

## Logging

To adjust logging level:

```json
{
  "jsonrpc": "2.0",
  "id": "log-1",
  "method": "logging/setLevel",
  "params": {
    "level": "info"
  }
}
```

Log message notification:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/message",
  "params": {
    "level": "info",
    "logger": "tool-executor",
    "data": "Tool 'bash' executed successfully"
  }
}
```

## Roots System

For clients that expose file system access:

Request from server to list roots:

```json
{
  "jsonrpc": "2.0",
  "id": "roots-1",
  "method": "roots/list"
}
```

Client response:

```json
{
  "jsonrpc": "2.0",
  "id": "roots-1",
  "result": {
    "roots": [
      {
        "uri": "file:///home/user/projects",
        "name": "User Projects"
      }
    ]
  }
}
```

## Implementation Advice

### Server Implementation

1. Start by implementing the initialization flow to establish capabilities
2. Create a router for handling JSON-RPC requests
3. Implement tool registration and discovery logic
4. Implement the tool execution system
5. Add resource management capabilities
6. Implement the prompt template system
7. Add support for binary content (images, audio)

### Client Implementation

1. Implement the JSON-RPC client with proper error handling
2. Handle the initialization handshake
3. Implement tool discovery and calling
4. Add support for resource reading
5. Implement any client capabilities (sampling, roots)
6. Add proper progress handling and cancellation support

## Security Considerations

1. **Authentication**: Add appropriate authentication to prevent unauthorized access
2. **Tool Validation**: Validate all tool inputs to prevent injection attacks
3. **Resource Boundaries**: Restrict resource access to appropriate boundaries
4. **Sandboxing**: Consider sandboxing tool execution, especially for dangerous operations
5. **Request Validation**: Validate all incoming JSON-RPC requests for proper format
6. **Rate Limiting**: Implement rate limiting to prevent abuse

This documentation provides the foundation for implementing a compliant MCP client or server. For specific edge cases and detailed behavior, refer to the full specification at the Model Context Protocol repository.