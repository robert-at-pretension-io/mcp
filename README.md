# MCP Tools

This project provides a set of tools for Anthropic's Model Context Protocol (MCP), allowing AI assistants to securely and effectively interact with external systems, data sources, and utilities.

## What is MCP?

The Model Context Protocol (MCP) is an open standard developed by Anthropic that enables AI assistants to interact with external tools and data sources in a standardized way. It acts as a universal interface between LLMs like Claude and the broader digital ecosystem, similar to how USB-C provides a standardized connection for various devices.

## Installation

### Prerequisites

- Rust (latest stable version)
- Cargo (comes with Rust)

### Steps

1. Clone the repository:
   ```bash
   git clone https://github.com/robert-at-pretension-io/mcp
   cd mcp
   ```

2. Build the project:
   ```bash
   cd mcp_tools && cargo build --release
   ```

3. Set up the required environment variables (see below)


## Environment Variables

The following environment variables are required or optional depending on which tools you enable:

| Variable | Required For | Description |
|----------|-------------|-------------|
| `SCRAPINGBEE_API_KEY` | Web Scraping | API key for ScrapingBee service |
| `BRAVE_API_KEY` | Brave Search | API key for Brave Search API |
| `AIDER_API_KEY` | Aider Tool | Your Anthropic API key for Aider (without the 'anthropic=' prefix) |
| `AIDER_MODEL` | Aider Tool | The model to use (e.g., 'claude-3-opus-20240229', 'claude-3-sonnet-20240229') |

### Additional Tool-Specific Variables

The following tools are available but commented out in the default configuration. Uncomment them in `main.rs` if you need these features:

- **Oracle Database Tool**:
  - `ORACLE_USER`: Oracle database username
  - `ORACLE_PASSWORD`: Oracle database password
  - `ORACLE_CONNECT_STRING`: Oracle connection string

- **Gmail Integration**:
  - `GOOGLE_OAUTH_CLIENT_ID`: Google OAuth client ID
  - `GOOGLE_OAUTH_CLIENT_SECRET`: Google OAuth client secret
  - `GOOGLE_OAUTH_REDIRECT_URI`: Google OAuth redirect URI

- **Email Validation**:
  - `NEVERBOUNCE_API_KEY`: API key for NeverBounce service

## Enabled Tools

The default configuration enables the following tools:

1. **Web Scraping Tool (`scrape_url`)**: Extracts and processes content from websites
2. **Brave Search Tool (`brave_search`)**: Retrieves search results from Brave Search
3. **Quick Bash Tool (`quick_bash`)**: Executes simple shell commands
4. **Aider Tool (`aider`)**: AI pair programming tool for making targeted code changes
5. **Long Running Task Tool (`long_running_tool`)**: Manages background tasks that may take minutes or hours to complete

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Configuring Claude Desktop

To use this MCP tools project with Claude Desktop, you need to create a configuration file that tells Claude Desktop how to connect to your MCP server.

### Configuration File Location

- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

### Example Configuration


```json
  {
    "mcpServers": {
      "tools": {
        "command": "[where ever you write the executable]",
        "args": [],
        "env": {
          "SCRAPINGBEE_API_KEY": "[your key here]",
          "BRAVE_API_KEY": "[your key here]",
          "KNOWLEDGE_GRAPH_DIR": "[knowledge graph directory]",
          "AIDER_API_KEY" : "[only anthropic models now, so only anthropic api key]",
	      "AIDER_MODEL": "anthropic/claude-3-7-sonnet-20250219"  
        }
      }
    }
  }
```


## Troubleshooting

### Common Issues with MCP Servers in Claude Desktop

1. **Connection Refused**
   - **Symptom**: Claude Desktop reports "Connection refused" when trying to connect to the MCP server.
   - **Solution**: Ensure the MCP server is running and listening on the configured port. Check for any firewall rules that might be blocking the connection.

2. **Authentication Failed**
   - **Symptom**: Claude Desktop can connect to the server but reports authentication failures.
   - **Solution**: Verify that any required API keys are correctly set in your environment variables.

3. **Tool Not Found**
   - **Symptom**: Claude attempts to use a tool but receives a "Tool not found" error.
   - **Solution**: Check that the tool is enabled in your MCP server configuration and that all required dependencies for that tool are installed.

4. **Logs Not Appearing**
   - **Symptom**: You're not seeing expected log output.
   - **Solution**: Verify the LOG_DIR environment variable is set correctly and that the directory exists with proper write permissions.

5. **Slow Response Times**
   - **Symptom**: Tools take a long time to respond or time out.
   - **Solution**: Check your internet connection if the tool relies on external services. Consider increasing timeout values in the server configuration.

### Debugging Tips

1. Check the MCP server logs at `$LOG_DIR/mcp-server.log` for detailed error information.
2. Run the server with increased verbosity by setting `RUST_LOG="mcp_tools=trace,debug"`.
3. Use a tool like Postman to test the MCP server API endpoints directly.
4. Verify that all required environment variables are correctly set.
5. Restart both the MCP server and Claude Desktop after making configuration changes.
