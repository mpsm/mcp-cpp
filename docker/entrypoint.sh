#!/bin/bash
set -e

# Default clangd path (can be overridden by environment variable)
export CLANGD_PATH="${CLANGD_PATH:-/usr/bin/clangd-20}"

# Validate clangd is available
if [ ! -x "$CLANGD_PATH" ]; then
    echo "ERROR: clangd not found at $CLANGD_PATH" >&2
    exit 1
fi

# Check if running in interactive mode (helpful warning)
if [ -t 0 ]; then
    echo "WARNING: MCP server expects JSON-RPC over stdio. Running in interactive mode." >&2
    echo "Tip: Mount your C++ project at /workspace and use an MCP client to connect." >&2
    echo "" >&2
fi

# Check if workspace directory exists and has content
if [ -d "/workspace" ]; then
    if [ -z "$(ls -A /workspace 2>/dev/null)" ]; then
        echo "WARNING: /workspace is empty. Mount your C++ project here." >&2
    fi
else
    echo "WARNING: /workspace not mounted. Mount your C++ project with: -v /path/to/project:/workspace" >&2
fi

# Execute mcp-cpp-server with all arguments passed through
exec /usr/local/bin/mcp-cpp-server "$@"
