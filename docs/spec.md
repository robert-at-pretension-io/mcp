# Comprehensive Technical Breakdown of Model Context Protocol (MCP)

I'll provide a thorough technical breakdown of the Model Context Protocol to help you implement both a client and server from scratch. This guide will cover the protocol fundamentals, communication flow, message structure, and implementation details.

## 1. Protocol Fundamentals

### 1.1 Overview

Model Context Protocol (MCP) is built on JSON-RPC 2.0 and designed to establish a standardized way for AI models (clients) to interact with external data sources and tools (servers). The protocol enables bidirectional communication, allowing both clients and servers to make requests and send notifications.

### 1.2 Transport Layer

MCP doesn't specify the transport layer, so you can implement it over:
- WebSockets (recommended for real-time applications)
- HTTP
- TCP sockets
- In-process communication

### 1.3 Protocol Version

Current version: `2025-03-26`

## 2. JSON-RPC Message Structure

All communication in MCP follows the JSON-RPC 2.0 specification with these core message types:

### 2.1 Request (Expecting Response)

```json
{
  "jsonrpc": "2.0",
  "id": 1,  // Can be string or number
  "method": "resources/read",  // The operation to perform
  "params": {
    // Method-specific parameters
    "_meta": {
      // Optional metadata
      "progressToken": "token123"  // For progress tracking
    }
  }
}
```

### 2.2 Notification (No Response Expected)

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/resources/updated",
  "params": {
    // Method-specific parameters
    "_meta": {
      // Optional metadata
    }
  }
}
```

### 2.3 Response (Success)

```json
{
  "jsonrpc": "2.0",
  "id": 1,  // Matches the request id
  "result": {
    // Method-specific result
    "_meta": {
      // Optional metadata
    }
  }
}
```

### 2.4 Error Response

```json
{
  "jsonrpc": "2.0",
  "id": 1,  // Matches the request id
  "error": {
    "code": -32602,  // Standard JSON-RPC error codes
    "message": "Invalid params",
    "data": {}  // Optional additional error information
  }
}
```

### 2.5 Batch Requests/Responses

The protocol supports batching multiple requests/notifications in a single message:

```json
[
  {"jsonrpc": "2.0", "id": 1, "method": "resources/list"},
  {"jsonrpc": "2.0", "method": "notifications/initialized"}
]
```

## 3. Connection Lifecycle

### 3.1 Initialization

1. **Client sends initialize request**:
   ```json
   {
     "jsonrpc": "2.0",
     "id": 1,
     "method": "initialize",
     "params": {
       "protocolVersion": "2025-03-26",
       "capabilities": {
         "sampling": {},
         "roots": {
           "listChanged": true
         }
       },
       "clientInfo": {
         "name": "MyMCPClient",
         "version": "1.0.0"
       }
     }
   }
   ```

2. **Server responds with capabilities**:
   ```json
   {
     "jsonrpc": "2.0",
     "id": 1,
     "result": {
       "protocolVersion": "2025-03-26",
       "serverInfo": {
         "name": "MyMCPServer",
         "version": "1.0.0"
       },
       "capabilities": {
         "resources": {
           "subscribe": true,
           "listChanged": true
         },
         "tools": {
           "listChanged": true
         },
         "prompts": {
           "listChanged": true
         }
       },
       "instructions": "This server provides access to project data and search tools."
     }
   }
   ```

3. **Client notifies initialized**:
   ```json
   {
     "jsonrpc": "2.0",
     "method": "notifications/initialized"
   }
   ```

### 3.2 Heartbeat/Ping

Either side can send pings to ensure the connection is alive:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "ping"
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {}
}
```

### 3.3 Termination

No specific termination message; simply close the underlying connection.

## 4. Core Capabilities

### 4.1 Resources

Resources represent data the server provides to the client.

#### Listing Resources

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "resources/list"
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "resources": [
      {
        "uri": "file:///project/readme.md",
        "name": "Project README",
        "description": "Project documentation",
        "mimeType": "text/markdown",
        "size": 2048
      }
    ],
    "nextCursor": "cursor123"  // For pagination
  }
}
```

#### Reading Resources

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "resources/read",
  "params": {
    "uri": "file:///project/readme.md"
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {
    "contents": [
      {
        "uri": "file:///project/readme.md",
        "mimeType": "text/markdown",
        "text": "# Project Documentation\n\nThis is a sample project..."
      }
    ]
  }
}
```

#### Resource Updates

Subscribe:
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "resources/subscribe",
  "params": {
    "uri": "file:///project"
  }
}
```

Update notification:
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/resources/updated",
  "params": {
    "uri": "file:///project/readme.md"
  }
}
```

### 4.2 Tools

Tools are functions the server provides that the client can call.

#### Listing Tools

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/list"
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "result": {
    "tools": [
      {
        "name": "search",
        "description": "Search for documents matching a query",
        "inputSchema": {
          "type": "object",
          "properties": {
            "query": {
              "type": "string",
              "description": "The search query"
            },
            "limit": {
              "type": "integer",
              "description": "Maximum number of results"
            }
          },
          "required": ["query"]
        },
        "annotations": {
          "readOnlyHint": true,
          "title": "Search Documents"
        }
      }
    ]
  }
}
```

#### Calling Tools

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "tools/call",
  "params": {
    "name": "search",
    "arguments": {
      "query": "database schema",
      "limit": 5
    }
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Found 3 results:\n1. database_schema.sql\n2. schema_documentation.md\n3. models.py"
      }
    ]
  }
}
```

### 4.3 Prompts

Prompts are templates the server provides to assist AI interaction.

#### Listing Prompts

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "method": "prompts/list"
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "result": {
    "prompts": [
      {
        "name": "code_review",
        "description": "Generate a code review for a pull request",
        "arguments": [
          {
            "name": "code",
            "description": "The code to review",
            "required": true
          },
          {
            "name": "focus",
            "description": "Focus area (security, performance, style)",
            "required": false
          }
        ]
      }
    ]
  }
}
```

#### Getting Prompts

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "prompts/get",
  "params": {
    "name": "code_review",
    "arguments": {
      "code": "def add(a, b):\n    return a + b",
      "focus": "style"
    }
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "result": {
    "description": "Code Review with style focus",
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "Please review this code focusing on style:\n\ndef add(a, b):\n    return a + b"
        }
      }
    ]
  }
}
```

### 4.4 LLM Sampling

Servers can request AI generation from clients:

Request (from server to client):
```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "sampling/createMessage",
  "params": {
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "What's the design pattern used in this code?"
        }
      }
    ],
    "modelPreferences": {
      "hints": [{"name": "claude-3-haiku"}],
      "speedPriority": 0.8,
      "intelligencePriority": 0.5,
      "costPriority": 0.6
    },
    "maxTokens": 500,
    "temperature": 0.7
  }
}
```

Response (from client to server):
```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "result": {
    "role": "assistant",
    "content": {
      "type": "text",
      "text": "The code appears to be using the Factory pattern..."
    },
    "model": "claude-3-haiku-20240307"
  }
}
```

### 4.5 Roots System

Servers can request access to specific directories:

Request (from server to client):
```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "method": "roots/list"
}
```

Response (from client to server):
```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "result": {
    "roots": [
      {
        "uri": "file:///projects/myproject",
        "name": "Current Project"
      },
      {
        "uri": "file:///projects/libs",
        "name": "Libraries"
      }
    ]
  }
}
```

## 5. Progress Tracking

For long-running operations, use progress notifications:

Request with progress token:
```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "resources/list",
  "params": {
    "_meta": {
      "progressToken": "op123"
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
    "progressToken": "op123",
    "progress": 50,
    "total": 100,
    "message": "Processing files..."
  }
}
```

## 6. Implementation Guidelines

### 6.1 Client Implementation

1. **Connection Management**:
   - Establish transport layer connection (WebSocket/HTTP)
   - Send initialize request
   - Track active requests with id -> callback map

2. **Message Handling**:
   - Parse incoming JSON messages
   - Route requests to handler functions
   - Match responses to pending requests
   - Dispatch notifications to listeners

3. **Core Functions**:
   - Resource discovery and fetching
   - Tool discovery and invocation
   - Prompt discovery and rendering
   - LLM sampling (responding to server requests)

4. **Error Handling**:
   - Handle JSON-RPC error responses
   - Implement timeouts for requests
   - Reconnection logic

### 6.2 Server Implementation

1. **Connection Management**:
   - Accept connections
   - Handle initialize request
   - Track client capabilities

2. **Resource Provider**:
   - Implement resource listing
   - Implement resource reading
   - Track resource subscriptions
   - Send update notifications

3. **Tool Provider**:
   - Register available tools
   - Implement tool dispatching
   - Return results or errors

4. **Prompt Provider**:
   - Define prompt templates
   - Implement template rendering

5. **LLM Consumer**:
   - Request AI generation when needed
   - Process AI responses

## 7. Implementation Example (Pseudocode)

### 7.1 Client Implementation

```javascript
class MCPClient {
  constructor(transport) {
    this.transport = transport;
    this.nextId = 1;
    this.pendingRequests = new Map();
    this.initialized = false;
    this.capabilities = null;
    
    // Set up message handler
    this.transport.onMessage(this.handleMessage.bind(this));
  }
  
  async initialize() {
    const result = await this.sendRequest('initialize', {
      protocolVersion: '2025-03-26',
      capabilities: {
        sampling: {},
        roots: { listChanged: true }
      },
      clientInfo: {
        name: 'MyMCPClient',
        version: '1.0.0'
      }
    });
    
    this.capabilities = result.capabilities;
    this.serverInfo = result.serverInfo;
    this.initialized = true;
    
    // Send initialized notification
    this.sendNotification('notifications/initialized');
    
    return result;
  }
  
  async listResources(cursor) {
    return this.sendRequest('resources/list', cursor ? { cursor } : {});
  }
  
  async readResource(uri) {
    return this.sendRequest('resources/read', { uri });
  }
  
  async callTool(name, arguments) {
    return this.sendRequest('tools/call', { name, arguments });
  }
  
  // Handle LLM sampling requests from server
  async handleCreateMessage(id, params) {
    // Validate request, show to user for approval
    // Generate response from LLM
    const response = await this.llm.generate(params);
    
    this.sendResponse(id, {
      role: 'assistant',
      content: {
        type: 'text',
        text: response.text
      },
      model: response.model
    });
  }
  
  // Core message handling
  handleMessage(message) {
    if (Array.isArray(message)) {
      // Handle batch
      message.forEach(m => this.handleSingleMessage(m));
      return;
    }
    
    this.handleSingleMessage(message);
  }
  
  handleSingleMessage(message) {
    // Request from server
    if (message.method && message.id) {
      this.handleRequest(message);
    }
    // Notification from server
    else if (message.method) {
      this.handleNotification(message);
    }
    // Response to our request
    else if (message.id) {
      this.handleResponse(message);
    }
  }
  
  handleRequest(request) {
    const { id, method, params } = request;
    
    switch (method) {
      case 'ping':
        this.sendResponse(id, {});
        break;
      case 'sampling/createMessage':
        this.handleCreateMessage(id, params);
        break;
      case 'roots/list':
        this.handleListRoots(id);
        break;
      default:
        this.sendError(id, METHOD_NOT_FOUND, `Method ${method} not supported`);
    }
  }
  
  // Helper methods for sending requests/notifications
  async sendRequest(method, params = {}) {
    const id = this.nextId++;
    const request = {
      jsonrpc: '2.0',
      id,
      method,
      params
    };
    
    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
      this.transport.send(JSON.stringify(request));
    });
  }
  
  sendNotification(method, params = {}) {
    const notification = {
      jsonrpc: '2.0',
      method,
      params
    };
    
    this.transport.send(JSON.stringify(notification));
  }
  
  sendResponse(id, result) {
    const response = {
      jsonrpc: '2.0',
      id,
      result
    };
    
    this.transport.send(JSON.stringify(response));
  }
  
  sendError(id, code, message, data) {
    const error = {
      jsonrpc: '2.0',
      id,
      error: {
        code,
        message,
        data
      }
    };
    
    this.transport.send(JSON.stringify(error));
  }
}
```

### 7.2 Server Implementation

```javascript
class MCPServer {
  constructor(transport) {
    this.transport = transport;
    this.nextId = 1;
    this.pendingRequests = new Map();
    this.clients = new Set();
    this.resourceSubscriptions = new Map();
    
    // Register available tools and resources
    this.tools = this.registerTools();
    this.resources = this.registerResources();
    this.prompts = this.registerPrompts();
    
    // Set up message handler
    this.transport.onConnect(this.handleConnection.bind(this));
  }
  
  handleConnection(client) {
    client.onMessage(message => this.handleMessage(client, message));
    client.onDisconnect(() => this.handleDisconnect(client));
    this.clients.add(client);
  }
  
  handleDisconnect(client) {
    this.clients.delete(client);
    // Clean up subscriptions
    for (const [uri, clients] of this.resourceSubscriptions.entries()) {
      clients.delete(client);
      if (clients.size === 0) {
        this.resourceSubscriptions.delete(uri);
      }
    }
  }
  
  handleMessage(client, message) {
    if (Array.isArray(message)) {
      // Handle batch
      message.forEach(m => this.handleSingleMessage(client, m));
      return;
    }
    
    this.handleSingleMessage(client, message);
  }
  
  handleSingleMessage(client, message) {
    // Request from client
    if (message.method && message.id) {
      this.handleRequest(client, message);
    }
    // Notification from client
    else if (message.method) {
      this.handleNotification(client, message);
    }
    // Response to our request
    else if (message.id) {
      this.handleResponse(client, message);
    }
  }
  
  handleRequest(client, request) {
    const { id, method, params } = request;
    
    switch (method) {
      case 'initialize':
        this.handleInitialize(client, id, params);
        break;
      case 'ping':
        this.sendResponse(client, id, {});
        break;
      case 'resources/list':
        this.handleListResources(client, id, params);
        break;
      case 'resources/read':
        this.handleReadResource(client, id, params);
        break;
      case 'resources/subscribe':
        this.handleSubscribe(client, id, params);
        break;
      case 'tools/list':
        this.handleListTools(client, id, params);
        break;
      case 'tools/call':
        this.handleCallTool(client, id, params);
        break;
      case 'prompts/list':
        this.handleListPrompts(client, id, params);
        break;
      case 'prompts/get':
        this.handleGetPrompt(client, id, params);
        break;
      default:
        this.sendError(client, id, METHOD_NOT_FOUND, `Method ${method} not supported`);
    }
  }
  
  handleInitialize(client, id, params) {
    client.capabilities = params.capabilities;
    client.protocolVersion = params.protocolVersion;
    client.clientInfo = params.clientInfo;
    
    this.sendResponse(client, id, {
      protocolVersion: '2025-03-26',
      serverInfo: {
        name: 'MyMCPServer',
        version: '1.0.0'
      },
      capabilities: {
        resources: {
          subscribe: true,
          listChanged: true
        },
        tools: {
          listChanged: true
        },
        prompts: {
          listChanged: true
        }
      },
      instructions: "This server provides access to project data and tools."
    });
  }
  
  handleListResources(client, id, params) {
    const { cursor } = params || {};
    const resources = this.getResources(cursor);
    
    this.sendResponse(client, id, {
      resources,
      nextCursor: resources.length >= 100 ? this.generateCursor() : undefined
    });
  }
  
  handleReadResource(client, id, params) {
    const { uri } = params;
    try {
      const resource = this.readResource(uri);
      this.sendResponse(client, id, {
        contents: [resource]
      });
    } catch (error) {
      this.sendError(client, id, INTERNAL_ERROR, error.message);
    }
  }
  
  handleSubscribe(client, id, params) {
    const { uri } = params;
    
    if (!this.resourceSubscriptions.has(uri)) {
      this.resourceSubscriptions.set(uri, new Set());
    }
    
    this.resourceSubscriptions.get(uri).add(client);
    this.sendResponse(client, id, {});
  }
  
  handleListTools(client, id, params) {
    const { cursor } = params || {};
    const tools = this.getTools(cursor);
    
    this.sendResponse(client, id, {
      tools,
      nextCursor: tools.length >= 100 ? this.generateCursor() : undefined
    });
  }
  
  handleCallTool(client, id, params) {
    const { name, arguments: args } = params;
    const tool = this.tools.find(t => t.name === name);
    
    if (!tool) {
      this.sendError(client, id, METHOD_NOT_FOUND, `Tool ${name} not found`);
      return;
    }
    
    try {
      const result = this.executeTool(name, args);
      this.sendResponse(client, id, {
        content: [
          {
            type: 'text',
            text: result
          }
        ]
      });
    } catch (error) {
      // Tool errors are part of the result, not JSON-RPC errors
      this.sendResponse(client, id, {
        content: [
          {
            type: 'text',
            text: error.message
          }
        ],
        isError: true
      });
    }
  }
  
  // Sample LLM request
  async getAIGeneration(prompt) {
    // Find a client that supports sampling
    const client = [...this.clients].find(c => 
      c.capabilities && c.capabilities.sampling);
    
    if (!client) {
      throw new Error("No clients support AI sampling");
    }
    
    return this.sendRequest(client, 'sampling/createMessage', {
      messages: [
        {
          role: 'user',
          content: {
            type: 'text',
            text: prompt
          }
        }
      ],
      maxTokens: 1000
    });
  }
  
  // Helper methods for sending messages
  sendResponse(client, id, result) {
    const response = {
      jsonrpc: '2.0',
      id,
      result
    };
    
    client.send(JSON.stringify(response));
  }
  
  sendError(client, id, code, message, data) {
    const error = {
      jsonrpc: '2.0',
      id,
      error: {
        code,
        message,
        data
      }
    };
    
    client.send(JSON.stringify(error));
  }
  
  async sendRequest(client, method, params = {}) {
    const id = this.nextId++;
    const request = {
      jsonrpc: '2.0',
      id,
      method,
      params
    };
    
    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject, client });
      client.send(JSON.stringify(request));
    });
  }
  
  sendNotification(client, method, params = {}) {
    const notification = {
      jsonrpc: '2.0',
      method,
      params
    };
    
    client.send(JSON.stringify(notification));
  }
  
  // Resource update notification
  notifyResourceUpdated(uri) {
    const subscribers = this.resourceSubscriptions.get(uri) || new Set();
    
    for (const client of subscribers) {
      this.sendNotification(client, 'notifications/resources/updated', {
        uri
      });
    }
  }
}
```

## 8. Testing Your Implementation

1. **Connection Tests**:
   - Test initialization flow
   - Test ping/pong
   - Test reconnection

2. **Resources Tests**:
   - Test listing resources
   - Test reading resources
   - Test subscribing to updates
   - Test receiving update notifications

3. **Tools Tests**:
   - Test listing tools
   - Test calling tools with valid arguments
   - Test error handling for invalid arguments

4. **Prompts Tests**:
   - Test listing prompts
   - Test retrieving prompts with arguments

5. **LLM Sampling Tests**:
   - Test receiving sampling requests (server-to-client)
   - Test providing sampling results

6. **Edge Cases**:
   - Test invalid JSON
   - Test unsupported methods
   - Test connection loss
   - Test expired requests (timeouts)
   - Test batch requests/responses

## 9. Security Considerations

1. **Authentication**: The protocol doesn't specify authentication; implement app-specific auth.
2. **Authorization**: Validate access rights for resources, tools, etc.
3. **Input Validation**: Validate all parameters before processing.
4. **Rate Limiting**: Implement rate limiting to prevent abuse.
5. **Resource Constraints**: Set limits on resource size, request frequency, etc.
6. **Tool Safety**: Validate tool inputs to prevent injection attacks or destructive operations.

## 10. Performance Optimization

1. **Connection Pooling**: Reuse connections for multiple requests.
2. **Batch Processing**: Use batch requests for multiple operations.
3. **Pagination**: Implement cursor-based pagination for large resource sets.
4. **Incremental Updates**: Send only changed resources, not full re-reads.
5. **Compression**: Consider compressing large messages.
6. **Caching**: Cache resources and tool results where appropriate.

This implementation guide should give you a solid foundation for building both clients and servers that support the Model Context Protocol. The protocol is designed to be extensible, so you can add custom capabilities as needed for your specific use case.