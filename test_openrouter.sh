#!/bin/bash

# Set OpenRouter API key
export OPENROUTER_API_KEY="$1" # Pass API key as first argument

# Run MCP REPL with OpenRouter provider
RUST_LOG=debug DISABLE_TRACING=1 RUSTYLINE_LOG=off cargo run --package mcp_host --bin mcp_repl load_config debug_config.json

# Usage: ./test_openrouter.sh "your-openrouter-api-key-here"