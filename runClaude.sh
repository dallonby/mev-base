#!/bin/bash

# Start Serena MCP server with Claude Code
# Usage: ./start-serena.sh [project_directory]

# Use provided directory or current directory
PROJECT_DIR="${1:-$(pwd)}"

# Check if directory exists
if [ ! -d "$PROJECT_DIR" ]; then
    echo "Error: Directory '$PROJECT_DIR' does not exist"
    exit 1
fi

echo "Starting Serena MCP server for project: $PROJECT_DIR"

# Run the claude mcp command
claude mcp add serena -- uvx --from git+https://github.com/oraios/serena serena start-mcp-server --context ide-assistant --project "$PROJECT_DIR"

