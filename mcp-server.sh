#!/bin/bash

# Load additional environment variables for logging
if [ -f .env ]; then
    set -o allexport
    source .env
    set +o allexport
fi

# Set default environment variables if not provided
export BRAVE_API_KEY=${BRAVE_API_KEY:-"none"}
export SCRAPINGBEE_API_KEY=${SCRAPINGBEE_API_KEY:-"none"}
export KNOWLEDGE_GRAPH_DIR=${KNOWLEDGE_GRAPH_DIR:-"$HOME/Developer/.mcp/knowledge_graph"}
export THOUGHTS_DIR=${THOUGHTS_DIR:-"$HOME/Developer/.mcp/thoughts"}

# Set up tracing configuration
export RUST_LOG="mcp_tools=debug,info"
export RUST_BACKTRACE=1

# Set up logging directory
LOG_DIR="$HOME/Developer/.mcp/logs"


# Get the directory where the script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
echo "Starting MCP server with debug tracing..."
echo "Log file: $LOG_DIR/mcp-server.log"

# Run with tracing enabled
RUST_LOG="mcp_tools=debug,info"
cd "$SCRIPT_DIR"
exec "target/debug/mcp_tools" "$@"
