# MCP Multi-Client

An improved MCP (Model Context Protocol) client that connects to multiple servers defined in a configuration file and provides both a command-line REPL and a web interface for interaction.

## Features

- Connect to multiple MCP tool servers simultaneously
- Support for multiple AI providers (Anthropic, OpenAI, Google, Mistral, Fireworks)
- Tool execution with proper parsing and handling
- Chat functionality with tool usage
- Response verification against auto-generated criteria
- Interactive REPL interface for command-line usage
- Web interface for browser-based interaction
- Conversation history management and compaction

## Setup

1. Install dependencies:

```bash
npm install
```

2. Configure your MCP servers in `servers.json`:

```json
{
  "mcpServers": {
    "bash": {
      "command": "npx",
      "args": ["-y", "@anthropic/cli-mcp-server-tools", "bash"],
      "env": {}
    },
    "search": {
      "command": "npx",
      "args": ["-y", "@mcp/server-search@latest"],
      "env": {}
    }
    // Add more servers here
  }
  // timeouts removed
}
```

3. Configure your AI providers in `ai_config.json`:

```json
{
  "defaultProvider": "anthropic",
  "providers": {
    "anthropic": {
      "provider": "anthropic",
      "model": "claude-3-5-sonnet-20240620",
      "apiKeyEnvVar": "ANTHROPIC_API_KEY",
      "temperature": 0.7
    },
    "openai": {
      "provider": "openai",
      "model": "gpt-4o-mini",
      "apiKeyEnvVar": "OPENAI_API_KEY"
    }
    // Add more providers here
  }
}
```

4. Set up your API keys as environment variables (e.g., `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc., corresponding to the `apiKeyEnvVar` values in `ai_config.json`).

5. Configure model suggestions in `provider_models.toml` (Optional, used as fallback if `model` is not specified in `ai_config.json`).

## Usage

### Building the Project

```bash
npm run build
```

### REPL Mode (Command Line)

```bash
npm start
# or
npm run dev  # Build and start
```

### Web Interface

```bash
npm run web
```

### Both Modes Simultaneously

```bash
npm run both
```

## REPL Commands

- `chat` - Enter interactive chat mode with the AI agent
- `exit` - Exit chat mode or the REPL
- `quit` - Alias for exit
- `servers` - List all connected servers
- `use <server>` - Select a server for direct tool calls
- `tools [server]` - List tools on current or specified server
- `call <tool> [json]` - Call tool on current/auto-detected server
- `history` - Show conversation history (in chat mode)
- `clear` - Clear conversation history (in chat mode)

## Web Interface

The web interface is available at `http://localhost:3000` when running in web mode. It provides:

- Chat interface for interacting with the AI
- Real-time responses using WebSocket
- Display of available tools
- Connected server information
- Conversation history
- Tool call visualization
- Thinking indicators
- Responsive design with Tailwind CSS

## Architecture

The multi-client implementation follows a modular architecture:

- **ServerManager**: Manages connections to multiple tool servers
- **ConversationManager**: Handles chat logic, tool calls, and verification
- **AI Client Layer**: Provides a unified interface to different AI providers
- **REPL**: Command-line interface for user interaction
- **WebServer**: Web interface with REST API and WebSocket for real-time updates
- **UI Layer**: Responsive web interface using Tailwind CSS

## Custom AI Providers

To add a new AI provider:

1. Add the provider's LangChain integration to `package.json`
2. Update `AiClientFactory.ts` with a new case for the provider
3. Add model suggestions to `provider_models.toml`

## License

MIT
