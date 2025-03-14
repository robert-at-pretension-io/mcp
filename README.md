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
   cargo build --release
   ```

3. Set up the required environment variables (see below)


## Environment Variables

The following environment variables are required or optional depending on which tools you enable:

| Variable | Required For | Description |
|----------|-------------|-------------|
| `LOG_DIR` | All | Directory for log files (default: `~/Developer/mcp/logs`) |
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

## License

[Specify the license here]