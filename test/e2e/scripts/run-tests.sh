#!/bin/bash

# Script to run e2e tests locally
# This script ensures the MCP server is built before running tests

set -e

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
E2E_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== MCP C++ E2E Test Runner ==="
echo "Project root: $PROJECT_ROOT"
echo "E2E test directory: $E2E_DIR"

# Function to check if binary exists
check_binary() {
    local binary_path="$1"
    if [ -f "$binary_path" ]; then
        echo "✓ Found MCP server binary: $binary_path"
        return 0
    else
        echo "✗ MCP server binary not found: $binary_path"
        return 1
    fi
}

# Build the MCP server if not already built
echo
echo "=== Building MCP Server ==="
cd "$PROJECT_ROOT"

# Try to find existing binary first
RELEASE_BINARY="$PROJECT_ROOT/target/release/mcp-cpp-server"
DEBUG_BINARY="$PROJECT_ROOT/target/debug/mcp-cpp-server"

if check_binary "$RELEASE_BINARY"; then
    export MCP_SERVER_PATH="$RELEASE_BINARY"
elif check_binary "$DEBUG_BINARY"; then
    export MCP_SERVER_PATH="$DEBUG_BINARY"
else
    echo "No existing binary found. Building release binary..."
    cargo build --release
    if check_binary "$RELEASE_BINARY"; then
        export MCP_SERVER_PATH="$RELEASE_BINARY"
    else
        echo "ERROR: Failed to build MCP server binary"
        exit 1
    fi
fi

echo "Using MCP server binary: $MCP_SERVER_PATH"

# Install dependencies if needed
echo
echo "=== Installing E2E Test Dependencies ==="
cd "$E2E_DIR"

if [ ! -d "node_modules" ] || [ "package.json" -nt "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm ci
else
    echo "Dependencies are up to date"
fi

# Build test project if needed
echo
echo "=== Setting up C++ Test Project ==="
cd "$PROJECT_ROOT/test/test-project"

if [ ! -d "build" ] || [ "CMakeLists.txt" -nt "build" ]; then
    echo "Building C++ test project..."
    mkdir -p build
    cd build
    cmake .. \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
        -G Ninja 2>/dev/null || cmake .. \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
    
    if command -v ninja >/dev/null 2>&1; then
        ninja -j$(nproc 2>/dev/null || echo 4)
    else
        make -j$(nproc 2>/dev/null || echo 4)
    fi
else
    echo "C++ test project is up to date"
fi

# Run the tests
echo
echo "=== Running E2E Tests ==="
cd "$E2E_DIR"

echo "Environment:"
echo "  MCP_SERVER_PATH=$MCP_SERVER_PATH"
echo "  Working directory: $(pwd)"
echo

# Run tests with proper environment
npm test

echo
echo "=== E2E Tests Completed Successfully! ==="
