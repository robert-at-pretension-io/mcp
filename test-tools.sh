#!/bin/bash
set -e

echo "Building mcp_tools..."
cargo build --bin mcp_tools

echo "Starting mcp_tools server..."
# Kill existing processes to avoid conflicts
pkill -f mcp_tools || true
sleep 1

# Start the server
SERVER_PID=""
TOOLS_PATH="$PWD/target/debug/mcp_tools"
$TOOLS_PATH > server.log 2>&1 &
SERVER_PID=$!
echo "Server started with PID: $SERVER_PID"

# Give it time to initialize
sleep 2

echo "Testing server with basic JSON-RPC requests..."
echo "Sending initialize request..."
REQUEST='{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"experimental":null,"roots":null,"sampling":null},"client_info":{"name":"test-repl","version":"1.0.0"},"protocol_version":"2025-03-26"},"id":1}'
echo $REQUEST | nc -U /tmp/mcp-test-socket

echo "Sending tools/list request..."
REQUEST='{"jsonrpc":"2.0","method":"tools/list","params":null,"id":1}'
echo $REQUEST | nc -U /tmp/mcp-test-socket

echo "Stopping server..."
kill $SERVER_PID || true
echo "Done!"