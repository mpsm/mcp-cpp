# C++ MCP Server

[![CI](https://github.com/mpsm/mcp-cpp/actions/workflows/ci.yml/badge.svg)](https://github.com/mpsm/mcp-cpp/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/mpsm/mcp-cpp)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024%2B-orange.svg)](https://www.rust-lang.org)
[![Crates.io](https://img.shields.io/crates/v/mcp-cpp-server?label=crates.io)](https://crates.io/crates/mcp-cpp-server)

A Model Context Protocol (MCP) server that provides C++ code analysis capabilities through clangd LSP integration. Enables AI agents to work with C++ codebases using semantic understanding similar to modern IDEs.

## Why This MCP Server?

Modern C++ development relies heavily on advanced tooling to navigate complex codebases with preprocessor macros, template instantiations, and intricate inheritance hierarchies. While humans use IntelliSense-powered IDEs to understand these complexities, most AI agents rely on text-only browsing.

This MCP server bridges that gap by providing AI agents with semantic analysis capabilities similar to what developers experience in modern IDEs. Unlike generic LSP MCP implementations, this server focuses specifically on C++ workflows.

The server can handle multiple C++ projects simultaneously, which is particularly useful for complex scenarios like embedded Linux development where understanding interactions between individual components is crucial. It supports both CMake and Meson build systems with automatic build directory detection and switching.

Advanced indexing monitoring tracks both clangd's index state and logs to ensure complete symbol coverage, while intelligent filtering distinguishes between project code and external dependencies.

## Features

The server provides three core analysis tools for C++ development. The `get_project_details` tool performs dynamic CMake and Meson build environment discovery and enables configuration switching. For symbol exploration, `search_symbols` offers C++ symbol search with project boundary detection and intelligent filtering. When deeper analysis is needed, `analyze_symbol_context` provides symbol analysis with inheritance and call hierarchy support.

The implementation works with both CMake and Meson projects, handling projects with multiple libraries and executables simultaneously. Advanced indexing monitors clangd's index state and logs to ensure complete symbol coverage, while intelligent filtering distinguishes between project code and external dependencies. The server automatically discovers and switches between build configurations, and includes a Python CLI tool for quick symbol exploration.

## Component Discovery

The MCP server automatically looks for components in the current working directory, scanning 2 levels below by default. This scan depth can be changed using tool options. When an AI agent requests analysis using a build directory outside the project, the MCP server will use that hint path and create a component from it, allowing flexible project analysis beyond the default scanning scope.

## Dependencies

The server requires clangd 11 or later for C++ semantic analysis (clangd 20+ recommended), and Rust 2024 edition for building. Your project must use CMake or Meson to generate compilation databases (`compile_commands.json`).

You can optionally set the `CLANGD_PATH` environment variable to specify a custom clangd binary location.

## Installation

### Install from Crates.io (Recommended)

```bash
# Install directly from the registry
cargo install mcp-cpp-server

# The binary will be available in your cargo bin directory
# (usually ~/.cargo/bin/mcp-cpp-server)
```

### Install from Source

```bash
# Clone the repository
git clone https://github.com/mpsm/mcp-cpp.git
cd mcp-cpp

# Install from source
cargo install --path .

# The binary will be available in your cargo bin directory
# (usually ~/.cargo/bin/mcp-cpp-server)
```

### Docker Installation

```bash
# Build the Docker image
docker build -t mcp-cpp-server .

# Run with your C++ project mounted
docker run -i --rm -v /path/to/your/cpp-project:/workspace mcp-cpp-server
```

The Docker image includes:
- mcp-cpp-server binary
- clangd-20 for C++ semantic analysis
- Minimal Ubuntu-based runtime

## Usage

### Claude CLI Integration (Tested)

For Claude CLI, create or update your MCP configuration file (`~/.config/claude-cli/mcp_servers.json`):

```json
{
  "mcpServers": {
    "cpp-tools": {
      "command": "mcp-cpp-server",
      "env": {
        "CLANGD_PATH": "/usr/bin/clangd-20"
      }
    }
  }
}
```

### Amazon Q Developer CLI Integration (Tested)

For Amazon Q Developer CLI, add to your MCP configuration:

```json
{
  "mcpServers": {
    "cpp-tools": {
      "command": "mcp-cpp-server",
      "env": {
        "CLANGD_PATH": "/usr/bin/clangd-20"
      }
    }
  }
}
```

### Claude Desktop Integration

Add to your Claude Desktop configuration file (`~/.claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "cpp-tools": {
      "command": "mcp-cpp-server",
      "env": {
        "CLANGD_PATH": "/usr/bin/clangd-20"
      }
    }
  }
}
```

### Claude Code Integration (VS Code Extension)

Claude Code uses a different configuration file location and requires explicit permissions. Add to your Claude Code configuration file (`~/.claude.json`):

```json
{
  "mcpServers": {
    "cpp": {
      "type": "stdio",
      "command": "~/.cargo/bin/mcp-cpp-server",
      "args": [],
      "env": {
        "CLANGD_PATH": "/opt/homebrew/opt/llvm/bin/clangd"
      }
    }
  },
  "permissions": {
    "allow": [
      "mcp__cpp__search_symbols",
      "mcp__cpp__analyze_symbol_context",
      "mcp__cpp__get_project_details"
    ]
  }
}
```

**Notes:**
- Claude Code reads `~/.claude.json`, not `~/.claude/mcp_servers.json`
- The `permissions` section is required to enable the MCP tools
- Adjust the `command` path to match your cargo installation (use `which mcp-cpp-server` to find it)
- Adjust `CLANGD_PATH` to your clangd installation (use `which clangd` to find it, or omit if clangd is in your PATH)
- Tools are prefixed with `mcp__cpp__` in Claude Code (e.g., `mcp__cpp__search_symbols`)

### Docker Usage with MCP Clients

**Claude Desktop/CLI:**

```json
{
  "mcpServers": {
    "cpp-tools": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-v", "/path/to/cpp-project:/workspace",
        "mcp-cpp-server",
        "--root", "/workspace"
      ]
    }
  }
}
```

**Note:** Arguments after the image name are passed to mcp-cpp-server. Use `-e RUST_LOG=debug` for verbose logging.

## Platform Support

Tested on:

- **Windows with WSL2 Ubuntu**
- **Ubuntu (native)**
- **macOS**

## Configuration

### CLI Options

The server supports the following command-line options:

```bash
mcp-cpp-server --help

# Options:
--root <DIR>             Project root directory to scan for build configurations (defaults to current directory)
--clangd-path <PATH>     Path to clangd executable (overrides CLANGD_PATH env var)
--log-level <LEVEL>      Log level (overrides RUST_LOG env var) 
--log-file <FILE>        Log file path (overrides MCP_LOG_FILE env var)
```

### Environment Variables

- **`CLANGD_PATH`**: Path to clangd executable (default: "clangd")
- **`RUST_LOG`**: Log level - trace, debug, info, warn, error (default: "info")
- **`MCP_LOG_FILE`**: Path to log file (default: logs to stderr only)
- **`MCP_LOG_UNIQUE`**: Set to "true" to append process ID to log filename

### Python CLI for Debugging

The Python CLI helps you understand what your AI agent sees from the MCP server, making it useful for debugging interactions. Note that this tool is not included in the distributed package and must be used directly from the repository:

```bash
# Clone the repository if you haven't already
git clone https://github.com/mpsm/mcp-cpp.git
cd mcp-cpp

# Install CLI dependencies
pip install -r tools/requirements.txt

# Search for symbols (see what the agent would see)
python3 tools/mcp-cli.py search-symbols "MyClass"

# Get complete API overview of a header file
python3 tools/mcp-cli.py search-symbols "" --files include/api.h

# Analyze a symbol with examples
python3 tools/mcp-cli.py analyze-symbol "MyClass::process"

# Get project overview
python3 tools/mcp-cli.py get-project-details
```

### Basic Workflow

1. **Get Project Details**

   ```json
   { "name": "get_project_details" }
   ```

   With custom scan parameters:

   ```json
   {
     "name": "get_project_details",
     "arguments": {
       "path": "/path/to/project",
       "depth": 5
     }
   }
   ```

2. **Search C++ Symbols**

   ```json
   {
     "name": "search_symbols",
     "arguments": { "query": "std::vector", "include_external": true }
   }
   ```

   File-specific search with custom build directory:

   ```json
   {
     "name": "search_symbols",
     "arguments": {
       "query": "MyClass",
       "files": ["include/MyClass.hpp"],
       "build_directory": "build-debug",
       "wait_timeout": 30
     }
   }
   ```

3. **Analyze Symbol Context**

   ```json
   {
     "name": "analyze_symbol_context",
     "arguments": {
       "symbol": "MyClass::process",
       "max_examples": 5
     }
   }
   ```

   With location hint for disambiguation:

   ```json
   {
     "name": "analyze_symbol_context",
     "arguments": {
       "symbol": "factorial",
       "build_directory": "/path/to/build",
       "location_hint": "/path/to/file.cpp:42:15",
       "wait_timeout": 0
     }
   }
   ```

## Use Cases

The server excels at code exploration and navigation, helping you find functions, classes, and variables across large codebases. It can analyze relationships between code components and navigate system libraries and third-party dependencies to understand how different parts of your project interact.

For code analysis and review, the server provides detailed symbol context including usage patterns, inheritance relationships, and call hierarchies. This helps you explore class hierarchies and call patterns, making it easier to understand unfamiliar code or prepare for refactoring by identifying all usages and dependencies.

The server also assists with development workflows by enabling switching between Debug, Release, and custom build configurations. It provides clear separation between project symbols and external library symbols, making navigation through large C++ codebases more efficient. The cross-reference generation helps you find all references, implementations, and related symbols quickly.

## Tool Reference

### C++ Analysis Tools

#### `get_project_details`

**Purpose**: Multi-provider build system analysis and project workspace discovery

**Options**:
- `path` (optional): Project root path to scan. If different from server default, triggers fresh scan
- `depth` (optional): Scan depth for component discovery (0-10 levels, default: 2). Controls how many directory levels below the current working directory to search for CMake/Meson components

**Output**: Complete project analysis including build configurations, components, compilation database status, and multi-provider discovery (CMake, Meson, etc.)

**Component Discovery**: By default, scans 2 levels below the current working directory for components. When AI agents specify build directories outside this scope, the server creates components from those hint paths automatically.

#### `search_symbols`

**Purpose**: Find C++ symbols across your codebase or get complete API overviews

**Key Capabilities**:

- **Symbol Discovery**: Find functions, classes, variables by name or pattern
- **Complete File Overview**: Use empty query (`""`) with file parameter to list all symbols in any file
- **API Exploration**: Perfect for understanding unfamiliar headers or source files
- **Smart Filtering**: Filter by symbol types (Class, Function, Method, etc.) and exclude external libraries

**Common Use Cases**:

```bash
# Find all vector-related symbols
search_symbols {"query": "vector"}

# Get complete overview of a header file
search_symbols {"query": "", "files": ["include/api.h"]}

# Find only classes and structs
search_symbols {"query": "Process", "kinds": ["Class", "Struct"]}
```

#### `analyze_symbol_context`

**Purpose**: Deep dive analysis of any C++ symbol with comprehensive context

**What You Get**:

- **Symbol Definition**: Complete type information, location, documentation
- **Usage Examples**: Real code showing how the symbol is used
- **Class Members**: All methods, fields, constructors (for classes)
- **Inheritance Tree**: Base classes and derived classes (for classes)
- **Call Relationships**: What calls this function and what it calls (for functions)

**Perfect For**:

- Understanding unfamiliar code
- Finding all usages before refactoring
- Exploring class hierarchies and relationships
- Learning how to use a function or class

```bash
# Analyze a class and its members
analyze_symbol_context {"symbol": "MyClass"}

# Deep dive into a specific method
analyze_symbol_context {"symbol": "MyClass::process", "max_examples": 3}
```

## Limitations

- Requires CMake or Meson projects that generate `compile_commands.json`
- First-time indexing can take time on large projects (configurable timeout, default 20s)
