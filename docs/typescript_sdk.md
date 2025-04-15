Model Context Protocol (MCP) Technical Implementation Guide
Table of Contents

Introduction
Core Concepts
Protocol Specification
Client Implementation
Server Implementation
Transport Mechanisms
Resources
Tools
Prompts
Authentication
Advanced Features
Best Practices

Introduction
The Model Context Protocol (MCP) is a standardized communication protocol designed to allow language model (LLM) applications to access external context through a secure, well-defined interface. MCP separates the concerns of providing context from the actual LLM interaction, enabling developers to create both:

MCP Servers: Backend services that expose data and functionality to LLMs
MCP Clients: Applications that can connect to any MCP server to retrieve context

This document provides comprehensive technical guidance for implementing MCP clients and servers based on the official specification, with examples drawn from the TypeScript SDK implementation.
Core Concepts
Protocol Overview
MCP is built on JSON-RPC 2.0, a lightweight remote procedure call protocol that uses JSON for data encoding. The protocol consists of:

Requests: Sent from client to server (or vice versa) to retrieve information or perform actions
Responses: Replies to requests with either successful results or error information
Notifications: One-way messages that don't require responses

Key Components
1. Server

Exposes data through Resources
Provides functionality through Tools
Defines interaction patterns through Prompts

2. Client

Connects to servers using transports (stdio, HTTP+SSE, etc.)
Retrieves resources and calls tools
Can facilitate sampling (LLM generation)

3. Transport Layer

Provides communication between clients and servers
Supported transports include stdin/stdout, HTTP with Server-Sent Events

Protocol Specification
Message Format
All MCP messages follow the JSON-RPC 2.0 format:
typescript// Request
{
  "jsonrpc": "2.0",
  "id": 123,                 // Used to correlate requests and responses
  "method": "resources/read",
  "params": {
    "uri": "file://example.txt"
  }
}

// Successful Response
{
  "jsonrpc": "2.0",
  "id": 123,
  "result": {
    "contents": [
      {
        "uri": "file://example.txt",
        "text": "File contents here"
      }
    ]
  }
}

// Error Response
{
  "jsonrpc": "2.0",
  "id": 123,
  "error": {
    "code": -32602,
    "message": "Invalid resource URI"
  }
}

// Notification (no response expected)
{
  "jsonrpc": "2.0",
  "method": "notifications/resources/updated",
  "params": {
    "uri": "file://example.txt"
  }
}
Protocol Versioning
The MCP protocol uses dates as version identifiers:
typescriptexport const LATEST_PROTOCOL_VERSION = "2024-11-05";
export const SUPPORTED_PROTOCOL_VERSIONS = [
  LATEST_PROTOCOL_VERSION,
  "2024-10-07",
];
Clients and servers must negotiate a compatible protocol version during initialization.
Connection Lifecycle

Initialization:
typescript// Client -> Server
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": { /* client capabilities */ },
    "clientInfo": { "name": "Example Client", "version": "1.0.0" }
  }
}

// Server -> Client
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": { /* server capabilities */ },
    "serverInfo": { "name": "Example Server", "version": "1.0.0" },
    "instructions": "Optional instructions for the client"
  }
}

// Client -> Server (notification)
{
  "jsonrpc": "2.0",
  "method": "notifications/initialized"
}

Normal Operation: After initialization, clients can list and access resources, call tools, and utilize prompts.
Termination: Either side can close the connection at any time.

Error Handling
MCP defines standard JSON-RPC error codes plus a few MCP-specific codes:
typescriptexport enum ErrorCode {
  // SDK error codes
  ConnectionClosed = -32000,
  RequestTimeout = -32001,

  // Standard JSON-RPC error codes
  ParseError = -32700,
  InvalidRequest = -32600,
  MethodNotFound = -32601,
  InvalidParams = -32602,
  InternalError = -32603,
}
Client Implementation
An MCP client connects to servers and consumes their resources and tools. Here's how to implement one:
Basic Client Structure
typescriptimport { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

// 1. Create a transport to the server
const transport = new StdioClientTransport({
  command: "node",
  args: ["server.js"]
});

// 2. Create and initialize the client
const client = new Client({
  name: "example-client",
  version: "1.0.0"
});

// 3. Connect to the server
await client.connect(transport);

// 4. Use the server's capabilities
const resources = await client.listResources();
console.log("Available resources:", resources);

const tools = await client.listTools();
console.log("Available tools:", tools);

// 5. Read a resource
const resource = await client.readResource({ uri: "file://example.txt" });
console.log("Resource content:", resource.contents[0].text);

// 6. Call a tool
const result = await client.callTool({
  name: "calculate",
  arguments: { a: 5, b: 3 }
});
console.log("Tool result:", result.content[0].text);
Implementing Client Capabilities
Clients can support various capabilities that servers can use:
typescriptconst client = new Client(
  { name: "example-client", version: "1.0.0" },
  {
    capabilities: {
      // Support for LLM sampling
      sampling: {},
      
      // Support for roots (file system access)
      roots: {
        listChanged: true  // Support notifications when roots change
      }
    }
  }
);
Handling Server Notifications
typescript// Set up handlers for notifications before connecting
client.setNotificationHandler(
  ResourceUpdatedNotificationSchema,
  (notification) => {
    console.log(`Resource updated: ${notification.params.uri}`);
    // Re-fetch the resource
    client.readResource({ uri: notification.params.uri })
      .then(resource => { /* process updated resource */ });
  }
);
Progress Tracking
For long-running operations, clients can request progress updates:
typescriptconst result = await client.readResource(
  { uri: "large-file://archive.zip" },
  {
    onprogress: (progress) => {
      const percent = progress.total 
        ? Math.round((progress.progress / progress.total) * 100)
        : `${progress.progress} units`;
      console.log(`Loading: ${percent}%`);
    }
  }
);
Server Implementation
An MCP server exposes resources, tools, and prompts to clients. Here's how to implement one:
Basic Server Structure
typescriptimport { McpServer, ResourceTemplate } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

// 1. Create the server
const server = new McpServer({
  name: "example-server",
  version: "1.0.0"
});

// 2. Add resources, tools, and prompts
server.resource(
  "example",
  "file://example.txt",
  async (uri) => ({
    contents: [{
      uri: uri.href,
      text: "Example file contents"
    }]
  })
);

server.tool(
  "add",
  "Add two numbers together",
  { a: z.number(), b: z.number() },
  async ({ a, b }) => ({
    content: [{ type: "text", text: String(a + b) }]
  })
);

server.prompt(
  "greet",
  "Generate a greeting for someone",
  { name: z.string() },
  ({ name }) => ({
    messages: [{
      role: "user",
      content: {
        type: "text",
        text: `Please generate a friendly greeting for ${name}.`
      }
    }]
  })
);

// 3. Start the server
const transport = new StdioServerTransport();
await server.connect(transport);
Dynamic Server Content
You can add, update, or remove resources, tools, and prompts after the server is connected:
typescript// Add a resource that can be disabled later
const secretResource = server.resource(
  "secret-file",
  "secret://file.txt",
  async (uri) => ({
    contents: [{
      uri: uri.href,
      text: "Top secret information"
    }]
  })
);

// Later, disable the resource (it will still exist but won't be listed or readable)
secretResource.disable();

// Later, re-enable the resource
secretResource.enable();

// Or update a resource's properties
secretResource.update({
  name: "public-file",
  uri: "public://file.txt",
  metadata: {
    description: "This file is now public"
  }
});

// Or completely remove a resource
secretResource.remove();
Server Capabilities
Servers express their capabilities during initialization:
typescriptconst server = new McpServer(
  { name: "example-server", version: "1.0.0" },
  {
    capabilities: {
      // Support for sending log messages
      logging: {},
      
      // Support for auto-completion
      completions: {},
      
      // Support for providing prompts, with list change notifications
      prompts: {
        listChanged: true
      },
      
      // Support for resources with subscriptions and list change notifications
      resources: {
        subscribe: true,
        listChanged: true
      },
      
      // Support for tools with list change notifications
      tools: {
        listChanged: true
      }
    }
  }
);
Transport Mechanisms
MCP supports multiple transport mechanisms to connect clients and servers.
stdio Transport
The simplest transport operates over standard input/output streams:
Server-side:
typescriptimport { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";

const transport = new StdioServerTransport();
await server.connect(transport);
Client-side:
typescriptimport { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const transport = new StdioClientTransport({
  command: "node",
  args: ["path/to/server.js"]
});
await client.connect(transport);
HTTP with Server-Sent Events (SSE)
For network-based communication:
Server-side:
typescriptimport express from "express";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { SSEServerTransport } from "@modelcontextprotocol/sdk/server/sse.js";

const server = new McpServer({
  name: "example-server",
  version: "1.0.0"
});

const app = express();
const transports = {};

app.get("/sse", async (_, res) => {
  const transport = new SSEServerTransport('/messages', res);
  transports[transport.sessionId] = transport;
  res.on("close", () => {
    delete transports[transport.sessionId];
  });
  await server.connect(transport);
});

app.post("/messages", async (req, res) => {
  const sessionId = req.query.sessionId;
  const transport = transports[sessionId];
  if (transport) {
    await transport.handlePostMessage(req, res);
  } else {
    res.status(400).send('No transport found for sessionId');
  }
});

app.listen(3001);
Client-side:
typescriptimport { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";

const transport = new SSEClientTransport({
  baseUrl: "http://localhost:3001"
});
await client.connect(transport);
Resources
Resources are how servers expose data to clients. They're similar to GET endpoints in REST APIs.
Resource Types

Static Resources: Fixed URIs with specific content
Resource Templates: URI patterns with variables that can match multiple resources

Static Resources
typescriptserver.resource(
  "config",
  "config://app",
  { description: "Application configuration" },  // optional metadata
  async (uri) => ({
    contents: [{
      uri: uri.href,
      text: "key1=value1\nkey2=value2"
    }]
  })
);
Resource Templates
typescriptserver.resource(
  "user-profile",
  new ResourceTemplate(
    "users://{userId}/profile", 
    {
      // Callback to list all matching resources (can be undefined)
      list: async () => ({
        resources: [
          { uri: "users://1/profile", name: "User 1" },
          { uri: "users://2/profile", name: "User 2" }
        ]
      }),
      // Optional completions for template variables
      complete: {
        userId: async (value) => ["1", "2", "3"].filter(id => id.startsWith(value))
      }
    }
  ),
  async (uri, variables) => ({
    contents: [{
      uri: uri.href,
      text: `Profile data for user ${variables.userId}`
    }]
  })
);
Resource Formats
Resources can contain text or binary data:
typescript// Text resource
{
  contents: [{
    uri: "file://example.txt",
    text: "File contents here",
    mimeType: "text/plain"  // optional
  }]
}

// Binary resource
{
  contents: [{
    uri: "file://image.png",
    blob: "iVBORw0KGgoAAAANS...", // base64-encoded data
    mimeType: "image/png"
  }]
}
Resource Subscriptions
Clients can subscribe to resource updates:
typescript// Client-side
await client.subscribeResource({ uri: "file://data.json" });

// Server-side (when the resource changes)
server.notification({
  method: "notifications/resources/updated",
  params: { uri: "file://data.json" }
});
Tools
Tools are server-provided functions that clients can call. They're similar to POST endpoints in REST APIs.
Tool Definition
Tools are defined with a name, optional description, parameter schema, and callback function:
typescriptserver.tool(
  "calculate-bmi",
  "Calculate Body Mass Index",
  {
    weightKg: z.number().positive(),
    heightM: z.number().positive()
  },
  async ({ weightKg, heightM }) => ({
    content: [{
      type: "text",
      text: String(weightKg / (heightM * heightM))
    }]
  })
);
Error Handling in Tools
typescriptserver.tool(
  "divide",
  {
    numerator: z.number(),
    denominator: z.number()
  },
  async ({ numerator, denominator }) => {
    if (denominator === 0) {
      return {
        content: [{ type: "text", text: "Error: Division by zero" }],
        isError: true
      };
    }
    return {
      content: [{ type: "text", text: String(numerator / denominator) }]
    };
  }
);
Rich Content in Tool Results
Tools can return various content types:
typescriptserver.tool(
  "get-image",
  { id: z.string() },
  async ({ id }) => {
    const imageData = await fetchImage(id);
    return {
      content: [{
        type: "image",
        data: imageData, // base64-encoded
        mimeType: "image/jpeg"
      }]
    };
  }
);
Prompts
Prompts are server-provided templates for LLM interactions.
Prompt Definition
typescriptserver.prompt(
  "code-review",
  "Generate a code review for a given piece of code",
  {
    code: z.string(),
    language: z.string().optional()
  },
  ({ code, language }) => ({
    description: "A detailed code review",
    messages: [{
      role: "user",
      content: {
        type: "text",
        text: `Please review this ${language || ""} code:\n\n${code}`
      }
    }]
  })
);
Using Prompts
Clients can retrieve and use prompts:
typescript// List available prompts
const prompts = await client.listPrompts();

// Get a specific prompt with arguments
const prompt = await client.getPrompt({
  name: "code-review",
  arguments: {
    code: "function hello() { return 'world'; }",
    language: "javascript"
  }
});

// Use the prompt with an LLM
const messages = prompt.messages;
// ... send these messages to an LLM API
Authentication
MCP supports OAuth 2.0 for authentication:
typescriptimport express from 'express';
import { ProxyOAuthServerProvider, mcpAuthRouter } from '@modelcontextprotocol/sdk';

const app = express();

const proxyProvider = new ProxyOAuthServerProvider({
    endpoints: {
        authorizationUrl: "https://auth.external.com/oauth2/v1/authorize",
        tokenUrl: "https://auth.external.com/oauth2/v1/token",
        revocationUrl: "https://auth.external.com/oauth2/v1/revoke",
    },
    verifyAccessToken: async (token) => {
        return {
            token,
            clientId: "123",
            scopes: ["openid", "email", "profile"],
        }
    },
    getClient: async (client_id) => {
        return {
            client_id,
            redirect_uris: ["http://localhost:3000/callback"],
        }
    }
})

app.use(mcpAuthRouter({
    provider: proxyProvider,
    issuerUrl: new URL("http://auth.external.com"),
    baseUrl: new URL("http://mcp.example.com"),
    serviceDocumentationUrl: new URL("https://docs.example.com/"),
}))
Advanced Features
URI Templates
URI templates follow RFC 6570 and enable dynamic resource addressing:
typescriptimport { UriTemplate } from "@modelcontextprotocol/sdk/shared/uriTemplate.js";

// Create a template
const template = new UriTemplate("files://{folder}/{filename}");

// Expand variables into a concrete URI
const uri = template.expand({ folder: "documents", filename: "report.pdf" });
// "files://documents/report.pdf"

// Match a URI against the template
const variables = template.match("files://projects/proposal.docx");
// { folder: "projects", filename: "proposal.docx" }
Completions
MCP servers can provide auto-completion suggestions for arguments:
typescriptimport { Completable } from "@modelcontextprotocol/sdk/server/completable.js";

server.prompt(
  "weather-forecast",
  {
    // Make the city parameter completable
    city: Completable.string(async (value) => {
      return ["New York", "London", "Tokyo", "Paris", "Beijing"]
        .filter(city => city.toLowerCase().startsWith(value.toLowerCase()));
    })
  },
  ({ city }) => ({ /* prompt implementation */ })
);
LLM Sampling
Servers can request LLMs to generate content via the client:
typescript// Server-side
const result = await server.server.request(
  {
    method: "sampling/createMessage",
    params: {
      messages: [
        {
          role: "user",
          content: {
            type: "text",
            text: "What is the capital of France?"
          }
        }
      ],
      maxTokens: 100,
      temperature: 0.7,
      includeContext: "thisServer"
    }
  },
  CreateMessageResultSchema
);

// Client-side implementation
client.setRequestHandler(
  CreateMessageRequestSchema,
  async (request) => {
    // Call an LLM API with the request parameters
    const llmResponse = await callLlmApi(request.params);
    
    return {
      model: "gpt-4",
      role: "assistant",
      content: {
        type: "text",
        text: llmResponse
      },
      stopReason: "endTurn"
    };
  }
);
Best Practices
Security Considerations

Input Validation: Always validate client inputs using schemas (the SDK uses Zod for this)
Resource Access Control: Implement proper authorization for resources and tools
Rate Limiting: Consider implementing rate limits for resource access and tool calls
Sanitize Error Messages: Avoid leaking sensitive information in error responses

Performance Optimization

Resource Caching: Implement caching for frequently accessed resources
Pagination: Use pagination for large resource lists:

typescript// Server-side pagination
server.setRequestHandler(
  ListResourcesRequestSchema,
  async (request) => {
    const pageSize = 10;
    const cursor = request.params?.cursor;
    const startIndex = cursor ? parseInt(cursor, 10) : 0;
    
    const allResources = await getAllResources();
    const pageResources = allResources.slice(startIndex, startIndex + pageSize);
    
    const nextCursor = startIndex + pageSize < allResources.length 
      ? (startIndex + pageSize).toString() 
      : undefined;
    
    return {
      resources: pageResources,
      nextCursor
    };
  }
);

// Client-side pagination
async function getAllResources(client) {
  const resources = [];
  let cursor;
  
  do {
    const response = await client.listResources({ cursor });
    resources.push(...response.resources);
    cursor = response.nextCursor;
  } while (cursor);
  
  return resources;
}
Extensibility

Custom Capabilities: Extend the MCP protocol with custom capabilities:

typescriptconst server = new McpServer(
  { name: "example-server", version: "1.0.0" },
  {
    capabilities: {
      experimental: {
        customFeature: {
          version: "1.0.0",
          options: { /* feature-specific options */ }
        }
      }
    }
  }
);

Protocol Versioning: Handle backward compatibility:

typescript// Check protocol version and adapt behavior
if (server.getClientProtocolVersion() === "2024-10-07") {
  // Use older response format
} else {
  // Use newer format
}

This technical document provides a comprehensive guide to implementing both clients and servers that support all features of the MCP specification. By following these patterns and examples, developers can create MCP-compatible systems that enhance LLM applications with robust context retrieval and tool execution capabilities.RetryClaude does not have the ability to run the code it generates yet. Claude does not have internet access. Links provided may not be accurate or up to date.REContinueEditModel Context Protocol (MCP) Technical Implementation Guide (Continued)
Additional Advanced Features
Custom Transports
While the SDK provides stdio and HTTP+SSE transports, you can implement custom transports by implementing the Transport interface:
typescriptimport { Transport } from "@modelcontextprotocol/sdk/shared/transport.js";
import { JSONRPCMessage } from "@modelcontextprotocol/sdk/types.js";

class CustomTransport implements Transport {
  onclose?: () => void;
  onerror?: (error: Error) => void;
  onmessage?: (message: JSONRPCMessage) => void;
  sessionId?: string;

  async start(): Promise<void> {
    // Initialize your transport
  }

  async close(): Promise<void> {
    // Clean up resources
  }

  async send(message: JSONRPCMessage, options?: { relatedRequestId?: string | number }): Promise<void> {
    // Send the message through your transport mechanism
  }
}
Nested Resources
Resources can contain links to other resources, enabling complex data hierarchies:
typescriptserver.resource(
  "project",
  new ResourceTemplate("projects://{projectId}", { list: undefined }),
  async (uri, { projectId }) => ({
    contents: [{
      uri: uri.href,
      text: JSON.stringify({
        id: projectId,
        name: `Project ${projectId}`,
        // Links to related resources
        files: "projects://{projectId}/files",
        members: "projects://{projectId}/members",
      })
    }]
  })
);

server.resource(
  "project-files",
  new ResourceTemplate("projects://{projectId}/files", { list: undefined }),
  async (uri, { projectId }) => ({
    contents: [{
      uri: uri.href,
      text: JSON.stringify([
        { name: "README.md", uri: `projects://${projectId}/files/README.md` },
        { name: "main.js", uri: `projects://${projectId}/files/main.js` }
      ])
    }]
  })
);
Resource Composition
Servers can combine multiple data sources into a single resource:
typescriptserver.resource(
  "dashboard",
  "dashboard://summary",
  async (uri) => {
    const [userStats, systemStatus, recentActivity] = await Promise.all([
      fetchUserStats(),
      fetchSystemStatus(),
      fetchRecentActivity()
    ]);
    
    return {
      contents: [{
        uri: uri.href,
        text: `# Dashboard Summary\n\n## User Stats\n${userStats}\n\n## System Status\n${systemStatus}\n\n## Recent Activity\n${recentActivity}`
      }]
    };
  }
);
Embedded Resources in Tool Responses
Tools can include resources directly in their responses:
typescriptserver.tool(
  "search-documents",
  { query: z.string() },
  async ({ query }) => {
    const results = await searchDocuments(query);
    
    return {
      content: [
        { type: "text", text: `Found ${results.length} documents for "${query}":` },
        ...results.map(doc => ({
          type: "resource" as const,
          resource: {
            uri: `documents://${doc.id}`,
            text: doc.content,
            mimeType: "text/plain"
          }
        }))
      ]
    };
  }
);
Error Handling Patterns
Graceful Degradation
Implement graceful degradation for client-server interactions:
typescriptasync function getResourceSafely(client, uri) {
  try {
    return await client.readResource({ uri });
  } catch (error) {
    console.warn(`Failed to read resource ${uri}: ${error.message}`);
    return {
      contents: [{
        uri,
        text: "[Resource unavailable]"
      }]
    };
  }
}
Detailed Error Information
Provide detailed error information to help clients diagnose problems:
typescriptserver.tool(
  "query-database",
  { sql: z.string() },
  async ({ sql }) => {
    try {
      const results = await executeQuery(sql);
      return {
        content: [{ type: "text", text: JSON.stringify(results) }]
      };
    } catch (error) {
      return {
        content: [{ 
          type: "text", 
          text: `Error executing query: ${error.message}\n` +
                `Error code: ${error.code}\n` +
                `Position: ${error.position || 'unknown'}`
        }],
        isError: true
      };
    }
  }
);
Request Cancellation
Both clients and servers can cancel in-progress requests:
typescript// Client-side cancellation
const abortController = new AbortController();
setTimeout(() => abortController.abort("Timeout"), 5000);

try {
  const result = await client.readResource(
    { uri: "large-data://dataset.csv" },
    { signal: abortController.signal }
  );
} catch (error) {
  if (error.name === "AbortError") {
    console.log("Request was cancelled:", error.message);
  } else {
    console.error("Request failed:", error);
  }
}

// Server-side cancellation tracking
server.resource(
  "large-data",
  new ResourceTemplate("large-data://{filename}", { list: undefined }),
  async (uri, { filename }, { signal }) => {
    const reader = createStreamReader(filename);
    let content = "";
    
    signal.addEventListener("abort", () => {
      reader.close();
    });
    
    while (!signal.aborted) {
      const chunk = await reader.read();
      if (!chunk) break;
      content += chunk;
      await new Promise(resolve => setTimeout(resolve, 100)); // Simulate slow reading
    }
    
    if (signal.aborted) {
      throw new Error("Resource reading was cancelled");
    }
    
    return {
      contents: [{
        uri: uri.href,
        text: content
      }]
    };
  }
);
Testing and Debugging
In-Memory Transport
For testing, you can use an in-memory transport that doesn't require actual I/O:
typescriptimport { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";

// Create server and client with connected transports
const { clientTransport, serverTransport } = InMemoryTransport.createPair();

const server = new McpServer({
  name: "test-server",
  version: "1.0.0"
});

const client = new Client({
  name: "test-client",
  version: "1.0.0"
});

await server.connect(serverTransport);
await client.connect(clientTransport);

// Now client and server can communicate directly in memory
Logging MCP Messages
To debug protocol messages, add logging to your transport or server/client:
typescriptclass LoggingTransport implements Transport {
  constructor(private baseTransport: Transport) {
    this.baseTransport.onmessage = (message) => {
      console.log("→ Received:", JSON.stringify(message, null, 2));
      this.onmessage?.(message);
    };
    
    this.baseTransport.onclose = () => {
      console.log("Connection closed");
      this.onclose?.();
    };
    
    this.baseTransport.onerror = (error) => {
      console.error("Transport error:", error);
      this.onerror?.(error);
    };
  }
  
  async start(): Promise<void> {
    console.log("Starting transport");
    return this.baseTransport.start();
  }
  
  async close(): Promise<void> {
    console.log("Closing transport");
    return this.baseTransport.close();
  }
  
  async send(message: JSONRPCMessage): Promise<void> {
    console.log("← Sending:", JSON.stringify(message, null, 2));
    return this.baseTransport.send(message);
  }
  
  onclose?: () => void;
  onerror?: (error: Error) => void;
  onmessage?: (message: JSONRPCMessage) => void;
}

// Use the logging transport
const baseTransport = new StdioClientTransport({
  command: "node",
  args: ["server.js"]
});
const loggingTransport = new LoggingTransport(baseTransport);
await client.connect(loggingTransport);
Real-World Use Cases
Code Assistant
Create an MCP server that provides code intelligence:
typescriptconst server = new McpServer({
  name: "code-assistant",
  version: "1.0.0"
});

// Add resources for file access
server.resource(
  "file",
  new ResourceTemplate("file://{path*}", { list: undefined }),
  async (uri, { path }) => {
    const filePath = path.join('/');
    const content = await fs.promises.readFile(filePath, 'utf-8');
    return {
      contents: [{
        uri: uri.href,
        text: content,
        mimeType: getMimeType(filePath)
      }]
    };
  }
);

// Add tools for code analysis
server.tool(
  "find-references",
  { 
    path: z.string(), 
    symbol: z.string(),
    line: z.number().int(),
    column: z.number().int()
  },
  async ({ path, symbol, line, column }) => {
    const references = await findReferences(path, symbol, line, column);
    return {
      content: [{ 
        type: "text", 
        text: JSON.stringify(references, null, 2)
      }]
    };
  }
);

// Add tools for code manipulation
server.tool(
  "apply-refactoring",
  { 
    path: z.string(), 
    operation: z.enum(["rename", "extract-method", "inline"]),
    params: z.record(z.string(), z.unknown())
  },
  async ({ path, operation, params }) => {
    const result = await performRefactoring(path, operation, params);
    return {
      content: [{ 
        type: "text", 
        text: result.message
      }]
    };
  }
);
Data Analysis Platform
Create an MCP server for data analysis:
typescriptconst server = new McpServer({
  name: "data-analysis",
  version: "1.0.0"
});

// Add resources for dataset access
server.resource(
  "dataset",
  new ResourceTemplate("dataset://{name}", { list: undefined }),
  async (uri, { name }) => {
    const dataset = await loadDataset(name);
    return {
      contents: [{
        uri: uri.href,
        text: dataset.preview(),
        mimeType: "text/csv"
      }]
    };
  }
);

// Add tools for data analysis
server.tool(
  "run-analysis",
  { 
    dataset: z.string(),
    analysis: z.enum(["summary", "correlation", "regression", "clustering"]),
    params: z.record(z.string(), z.unknown())
  },
  async ({ dataset, analysis, params }) => {
    const result = await performAnalysis(dataset, analysis, params);
    
    return {
      content: [
        // Text summary
        { 
          type: "text", 
          text: result.summary
        },
        // Image visualization (if available)
        ...(result.visualization ? [{
          type: "image" as const,
          data: result.visualization,
          mimeType: "image/png"
        }] : [])
      ]
    };
  }
);
Deployment
Containerization
Package your MCP server in a Docker container:
dockerfileFROM node:18-slim

WORKDIR /app

COPY package*.json ./
RUN npm ci --production

COPY dist/ ./dist/

EXPOSE 3000

CMD ["node", "dist/server.js"]
Serverless Deployment
Deploy your MCP server as a serverless function:
typescript// For AWS Lambda with API Gateway
import { APIGatewayProxyEvent, APIGatewayProxyResult } from 'aws-lambda';
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { SSEServerTransport } from "@modelcontextprotocol/sdk/server/sse.js";

// Create and configure your server
const server = new McpServer({
  name: "lambda-server",
  version: "1.0.0"
});

// ... add resources, tools, prompts

// Handle API Gateway events
export const handler = async (
  event: APIGatewayProxyEvent
): Promise<APIGatewayProxyResult> => {
  if (event.path === '/sse') {
    // Handle SSE connection
    // Note: This is simplified; actual Lambda + API Gateway SSE requires special handling
    return { 
      statusCode: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive'
      },
      body: '', // Actual implementation would be more complex
      isBase64Encoded: false
    };
  } else if (event.path === '/messages') {
    // Handle incoming messages
    // ... implementation
    return {
      statusCode: 200,
      body: JSON.stringify({ success: true }),
      headers: { 'Content-Type': 'application/json' }
    };
  } else {
    return {
      statusCode: 404,
      body: JSON.stringify({ error: 'Not found' }),
      headers: { 'Content-Type': 'application/json' }
    };
  }
};
Conclusion
The Model Context Protocol (MCP) provides a robust framework for connecting language models with external context and capabilities. By implementing MCP clients and servers, developers can create powerful applications that leverage LLMs while maintaining separation of concerns between the model interaction and the context provisioning.
Key features of MCP include:

Standardized Communication: A clear protocol based on JSON-RPC
Flexible Transport Options: Support for stdio, HTTP+SSE, and custom transports
Rich Resource Model: Static and templated resources with text and binary content
Powerful Tools: Function calls with strong parameter validation
Prompt Templates: Reusable interaction patterns for LLMs
Authentication: OAuth support for secure access
Extensibility: Custom capabilities and versioned protocol support

By following the patterns and techniques outlined in this guide, developers can build robust, interoperable systems that enhance LLM capabilities with external data and functionality.
For the latest updates and details, always refer to the official MCP specification and the TypeScript SDK documentation.RetryClaude does not have the ability to run the code it generates yet. Claude does not have internet access. Links provided may not be accurate or up to date.REWhat are the most important typescript types to be aware of as a client developer?EditAs a client developer working with the MCP TypeScript SDK, you should focus on these key types:
Core Protocol Types

Transport: Interface for communication channels
typescriptinterface Transport {
  onclose?: () => void;
  onerror?: (error: Error) => void;
  onmessage?: (message: JSONRPCMessage) => void;
  start(): Promise<void>;
  close(): Promise<void>;
  send(message: JSONRPCMessage): Promise<void>;
}

Client: Main client class for connecting to servers
typescriptclass Client<RequestT, NotificationT, ResultT> extends Protocol<...> {
  constructor(clientInfo: Implementation, options?: ClientOptions);
  connect(transport: Transport): Promise<void>;
  // Methods for calling MCP operations
}


Request/Response Types

Resource Types: For reading and managing resources
typescripttype ReadResourceRequest = {
  method: "resources/read";
  params: {
    uri: string;
  };
};

type ReadResourceResult = {
  contents: Array<TextResourceContents | BlobResourceContents>;
};

type TextResourceContents = {
  uri: string;
  text: string;
  mimeType?: string;
};

type BlobResourceContents = {
  uri: string;
  blob: string; // base64-encoded
  mimeType?: string;
};

Tool Types: For calling server-side functions
typescripttype CallToolRequest = {
  method: "tools/call";
  params: {
    name: string;
    arguments?: Record<string, unknown>;
  };
};

type CallToolResult = {
  content: Array<TextContent | ImageContent | AudioContent | EmbeddedResource>;
  isError?: boolean;
};

Content Types: Used in tool responses and messages
typescripttype TextContent = {
  type: "text";
  text: string;
};

type ImageContent = {
  type: "image";
  data: string; // base64-encoded
  mimeType: string;
};

type AudioContent = {
  type: "audio";
  data: string; // base64-encoded
  mimeType: string;
};


Client Helpers

RequestOptions: Options for request customization
typescripttype RequestOptions = {
  onprogress?: ProgressCallback;
  signal?: AbortSignal;
  timeout?: number;
  resetTimeoutOnProgress?: boolean;
  maxTotalTimeout?: number;
};

ClientCapabilities: Capabilities to advertise to servers
typescripttype ClientCapabilities = {
  experimental?: Record<string, unknown>;
  sampling?: Record<string, unknown>;
  roots?: {
    listChanged?: boolean;
  };
};


Transport Types

StdioClientTransport: For command-line MCP servers
typescripttype StdioServerParameters = {
  command: string;
  args?: string[];
  env?: Record<string, string>;
  stderr?: IOType | Stream | number;
  cwd?: string;
};

class StdioClientTransport implements Transport {
  constructor(server: StdioServerParameters);
  // Transport implementation
}

SSEClientTransport: For HTTP-based MCP servers
typescripttype SSEClientTransportOptions = {
  baseUrl: string;
  headers?: Record<string, string>;
};

class SSEClientTransport implements Transport {
  constructor(options: SSEClientTransportOptions);
  // Transport implementation
}


Understanding these types will give you a solid foundation for implementing MCP clients. The good news is that most methods in the Client class are strongly typed, so TypeScript will guide you on the correct request/response structures when using the high-level API.