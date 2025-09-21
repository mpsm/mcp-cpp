# C++ MCP Server

[![CI](https://github.com/mpsm/mcp-cpp/actions/workflows/ci.yml/badge.svg)](https://github.com/mpsm/mcp-cpp/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/mpsm/mcp-cpp)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024%2B-orange.svg)](https://www.rust-lang.org)
[![Crates.io](https://img.shields.io/crates/v/mcp-cpp-server?label=crates.io)](https://crates.io/crates/mcp-cpp-server)

A high-performance Model Context Protocol (MCP) server providing comprehensive C++ code analysis capabilities through integration with clangd Language Server Protocol (LSP). Designed to enable AI agents to work with C++ codebases with the same semantic understanding as modern IDEs.

## Why This MCP Server?

Modern C++ development relies heavily on advanced tooling to navigate complex codebases with preprocessor macros, template instantiations, and intricate inheritance hierarchies. While humans use IntelliSense-powered IDEs to understand these complexities, most AI agents rely on text-only browsing.

This MCP server bridges that gap by providing AI agents with semantic analysis capabilities comparable to what developers experience in modern IDEs. Unlike existing LSP MCP implementations that offer generic language server integration, this server provides a C++-focused approach with:

- **Dynamic Build Management**: On-the-fly CMake and Meson build directory detection and switching
- **C++-Optimized Tool Design**: Tools specifically designed for C++ workflows and symbol patterns
- **Project-Aware Analysis**: Intelligent filtering between project code and external dependencies
- **Future C++ Specialization**: Foundation for advanced C++ features like multi-component analysis

The current implementation focuses on essential C++ development workflows, with plans for future C++-specific optimizations like simultaneous multi-project analysis, advanced template relationship mapping, and C++-aware refactoring patterns.

## Features

### Core C++ Analysis Tools

- **`get_project_details`**: Dynamic CMake and Meson build environment discovery and configuration switching
- **`search_symbols`**: C++ symbol search with project boundary detection and intelligent filtering
- **`analyze_symbol_context`**: Comprehensive symbol analysis with inheritance and call hierarchy support

### Current Capabilities

- **Multi-Build System Support**: Works with both CMake and Meson projects seamlessly
- **Multi-Component Projects**: Handle projects with multiple libraries and executables
- **Project Boundary Intelligence**: Distinguish between project code and external dependencies
- **Dynamic Build Detection**: Automatic discovery and switching between build configurations
- **Comprehensive Analysis**: Deep symbol context with inheritance and usage patterns
- **Python CLI Tool**: Easy command-line interface for quick symbol exploration

### Planned C++ Enhancements

- **Advanced Template Intelligence**: Enhanced template instantiation and specialization analysis
- **C++-Specific Refactoring**: Specialized refactoring patterns for C++ codebases

## Dependencies

### Required

- **clangd 11+**: Language server for C++ semantic analysis (clangd 20+ recommended)
- **Rust 2024 edition**: For building the MCP server
- **CMake or Meson**: For generating compilation databases (`compile_commands.json`)

### Optional

- **CLANGD_PATH**: Environment variable to specify custom clangd binary location

## Installation & Build

```bash
# Clone the repository
git clone https://github.com/mpsm/mcp-cpp.git
cd mcp-cpp

# Build the server
cargo build --release

# The binary will be available at target/release/mcp-cpp-server
```

## Usage

### Claude Desktop Integration

Add to your Claude Desktop configuration file (`.mcp.json`):

```json
{
  "mcpServers": {
    "cpp-tools": {
      "command": "/path/to/mcp-cpp/target/release/mcp-cpp-server",
      "env": {
        "CLANGD_PATH": "/usr/bin/clangd-20"
      }
    }
  }
}
```

### Quick Start with Python CLI

For easier interaction, use the included Python CLI:

```bash
# Install CLI dependencies
pip install -r tools/requirements.txt

# Search for symbols
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

### Code Exploration & Navigation

- **Symbol Discovery**: Find functions, classes, and variables across large codebases
- **Dependency Analysis**: Understand relationships between code components
- **API Exploration**: Navigate system libraries and third-party dependencies

### Code Analysis & Review

- **Symbol Context**: Comprehensive analysis of symbol usage, inheritance, and relationships
- **Architecture Understanding**: Explore class hierarchies and call patterns
- **Refactoring Preparation**: Identify all usages and dependencies before changes

### Development Assistance

- **Dynamic Build Management**: Seamless switching between Debug, Release, and custom build configurations
- **Project-Focused Analysis**: Clear separation between project symbols and external library symbols
- **Efficient Symbol Discovery**: Fast navigation through large C++ codebases with intelligent filtering
- **Comprehensive Context**: Complete symbol analysis including inheritance hierarchies and call patterns
- **Cross-Reference Generation**: Find all references, implementations, and related symbols

## Tool Reference

### C++ Analysis Tools

#### `get_project_details`

**Purpose**: Multi-provider build system analysis and project workspace discovery
**Input**:

- `path` (optional): Project root path to scan (triggers fresh scan if different from server default)
- `depth` (optional): Scan depth for component discovery (0-10 levels, triggers fresh scan if different)

**Output**: Complete project analysis including build configurations, components, compilation database status, and multi-provider discovery (CMake, Meson, etc.)

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

## Design Insights & Limitations

### Current Implementation

- **Build Directory Detection**: Automatically discovers existing CMake and Meson build directories or uses agent-provided paths
- **Build Configuration Analysis**: Extracts compiler settings, generator type, and build options from build systems
- **Indexing Management**: Monitors clangd indexing progress and waits for completion before returning results
- **Basic Error Handling**: Provides build configuration validation and clangd lifecycle management

### Known Limitations

- **Indexing Wait Strategy**: Currently blocks until indexing completes, which can be slow for large projects
- **Single Configuration Focus**: Works with one build directory at a time, no multi-config support yet
- **Modern Build Systems**: Works with CMake and Meson projects that generate `compile_commands.json`
- **No Incremental Updates**: Requires full re-indexing when switching build configurations

### Future Considerations

- **Fail-Fast Indexing**: Could provide partial results with indexing progress info, letting AI agents decide whether to wait
- **Multi-Configuration Support**: Enable simultaneous analysis across Debug/Release/custom builds
- **Build System Expansion**: Support for other build systems beyond CMake
- **Incremental Analysis**: Smarter indexing updates when build configurations change

### Architecture Decisions

The current design prioritizes accuracy over speed by waiting for complete indexing. This ensures reliable symbol information but can introduce latency. The trade-off between response time and accuracy may be configurable in future versions, allowing AI agents to choose between fast partial results or complete analysis.
