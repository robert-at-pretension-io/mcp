#!/bin/bash

# Script to run MCP host with access to all tools

# Build the projects
echo "Building MCP host and tools..."
cargo build --package mcp_host
cargo build --package mcp_tools

# Create a config file for MCP with all tools
echo "Creating MCP config file..."
cat > config.json << 'EOF'
{
  "mcpServers": {
    "default": {
      "command": "cargo run --package mcp_tools --bin mcp_tools",
      "env": {
        "MCP_TOOLS_ENABLED": "bash,git_integration,google_search,brave_search,scraping_bee,email_validator,oracle_tool,mermaid_chart,regex_replace,long_running_task"
      }
    }
  }
}
EOF

# Build a standalone executable
echo "Building standalone executable..."
cargo build --package mcp_host --bin mcp_host

# Run MCP host with the config
echo "Starting MCP host with all tools..."
RUST_LOG=warn ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-dummy_key}" ./target/debug/mcp_host load_config config.json

# Clean up
rm config.json