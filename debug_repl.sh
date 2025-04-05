#!/bin/bash
set -e

# Debug script for testing MCP REPL
echo "Starting MCP REPL debug script"

# Build the necessary binaries first
echo "Building mcp_tools..."
cargo build -p mcp_tools

echo "Building mcp_host..."
cargo build -p mcp_host

# Create debug config with absolute paths
echo "Creating debug config..."
cat > debug_config.json << EOF
{
  "mcpServers": {
    "default": {
      "command": "$(pwd)/target/debug/mcp_tools",
      "env": {
        "RUST_LOG": "debug",
        "MCP_TOOLS_ENABLED": "bash,git_integration,google_search,brave_search,scraping_bee,mermaid_chart,regex_replace,long_running_task"
      }
    }
  },
  "ai_provider": {
    "provider": "deepseek",
    "model": "deepseek-chat"
  },
  "timeouts": {
    "request": 120,
    "tool": 300
  }
}
EOF

echo "Config created at $(pwd)/debug_config.json"
echo "Config content:"
cat debug_config.json

# Check that the tool binary exists and is executable
echo "Verifying tool binary..."
if [ -x "$(pwd)/target/debug/mcp_tools" ]; then
  echo "Tool binary exists and is executable"
else
  echo "ERROR: Tool binary not found or not executable"
  ls -la $(pwd)/target/debug/mcp_tools || echo "File does not exist"
  exit 1
fi

# Try running the tools binary directly to verify it works
echo "Testing tool binary directly..."
$(pwd)/target/debug/mcp_tools --help 2>&1 || echo "Note: tool binary doesn't support --help, but it was executed"

# Run the mcp_host binary with our custom debug config
echo "Running REPL with debug config..."
RUST_LOG=debug $(pwd)/target/debug/mcp_repl load_config $(pwd)/debug_config.json > debug_output.log 2>&1 &
REPL_PID=$!

# Wait a bit for it to start
echo "Waiting for REPL to start (PID: $REPL_PID)..."
sleep 2

# Check if it's still running
if kill -0 $REPL_PID 2>/dev/null; then
  echo "REPL is running, checking for errors in log..."
  # Check for errors in the log
  if grep -i error debug_output.log; then
    echo "Found errors in debug_output.log"
  else
    echo "No errors found in debug_output.log"
  fi
  
  # Gracefully terminate the REPL
  echo "Terminating REPL..."
  kill $REPL_PID
else
  echo "REPL process has already terminated"
  echo "Debug log content:"
  cat debug_output.log
fi

echo "Debug script completed"