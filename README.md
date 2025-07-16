# C++ MCP Server

A high-performance Model Context Protocol (MCP) server providing comprehensive C++ code analysis capabilities through integration with clangd Language Server Protocol (LSP). Designed to enable AI agents to work with C++ codebases with the same semantic understanding as modern IDEs.

## Why This MCP Server?

Modern C++ development relies heavily on advanced tooling to navigate complex codebases with preprocessor macros, template instantiations, and intricate inheritance hierarchies. While humans use IntelliSense-powered IDEs to understand these complexities, most AI agents rely on text-only browsing.

This MCP server bridges that gap by providing AI agents with semantic analysis capabilities comparable to what developers experience in modern IDEs. Unlike existing LSP MCP implementations that offer generic language server integration, this server provides a C++-focused approach with:

- **Dynamic Build Management**: On-the-fly CMake build directory detection and switching
- **C++-Optimized Tool Design**: Tools specifically designed for C++ workflows and symbol patterns
- **Project-Aware Analysis**: Intelligent filtering between project code and external dependencies
- **Future C++ Specialization**: Foundation for advanced C++ features like multi-component analysis

The current implementation focuses on essential C++ development workflows, with plans for future C++-specific optimizations like simultaneous multi-project analysis, advanced template relationship mapping, and C++-aware refactoring patterns.

## Features

### Core C++ Analysis Tools

- **`list_build_dirs`**: Dynamic CMake build environment discovery and configuration switching
- **`search_symbols`**: C++ symbol search with project boundary detection and intelligent filtering
- **`analyze_symbol_context`**: Comprehensive symbol analysis with inheritance and call hierarchy support

### Current Capabilities

- **Dynamic Build Detection**: Automatic discovery and switching between build configurations
- **Project Boundary Intelligence**: Distinguish between project code and external dependencies
- **C++-Focused Search**: Optimized symbol discovery for C++ development patterns
- **Comprehensive Analysis**: Deep symbol context with inheritance and usage patterns

### Planned C++ Enhancements

- **Multi-Component Support**: Simultaneous analysis across multiple C++ libraries and executables
- **Advanced Template Intelligence**: Enhanced template instantiation and specialization analysis
- **C++-Specific Refactoring**: Specialized refactoring patterns for C++ codebases

## Dependencies

### Required

- **clangd 20+**: Language server for C++ semantic analysis
- **Rust 2024 edition**: For building the MCP server
- **CMake**: For generating compilation databases (`compile_commands.json`)

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

### Basic Workflow

1. **Discover Build Configurations**

   ```json
   { "name": "list_build_dirs" }
   ```

2. **Search C++ Symbols**

   ```json
   {
     "name": "search_symbols",
     "arguments": { "query": "std::vector", "include_external": true }
   }
   ```

3. **Analyze Symbol Context**

   ```json
   {
     "name": "analyze_symbol_context",
     "arguments": {
       "symbol": "MyClass::process",
       "include_inheritance": true,
       "include_call_hierarchy": true
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

#### `list_build_dirs`

**Purpose**: Dynamic CMake build environment discovery with multi-configuration support  
**Input**:

- `target_dir` (optional): Specific build directory to analyze or switch to

**Output**: Complete build environment analysis including compiler settings, available configurations, and compilation database status

#### `search_symbols`

**Purpose**: C++ symbol search with project boundary detection and intelligent filtering  
**Input**:

- `query` (required): Symbol name, pattern, or qualified name
- `kinds` (optional): C++ symbol types (class, function, variable, namespace, etc.)
- `files` (optional): Limit search to specific files or directories
- `max_results` (optional): Result limit (1-1000, default 100)
- `include_external` (optional): Include system headers and third-party libraries (default false)

**Output**: Ranked symbol results with project/external classification and context information

#### `analyze_symbol_context`

**Purpose**: Comprehensive C++ symbol analysis with inheritance and call hierarchy support  
**Input**:

- `symbol` (required): C++ symbol name (supports qualified names and operators)
- `location` (optional): Specific location for overload disambiguation
- `include_usage_patterns` (optional): Enable usage statistics and examples
- `include_inheritance` (optional): Include class hierarchy analysis
- `include_call_hierarchy` (optional): Include function call relationships
- `max_usage_examples` (optional): Limit usage examples
- `max_call_depth` (optional): Call hierarchy traversal depth

**Output**: Complete symbol context including definition, inheritance relationships, and usage patterns

## Design Insights & Limitations

### Current Implementation

- **Build Directory Detection**: Automatically discovers existing CMake build directories or uses agent-provided paths
- **CMake Configuration Analysis**: Extracts compiler settings, generator type, and build options from CMake cache
- **Indexing Management**: Monitors clangd indexing progress and waits for completion before returning results
- **Basic Error Handling**: Provides build configuration validation and clangd lifecycle management

### Known Limitations

- **Indexing Wait Strategy**: Currently blocks until indexing completes, which can be slow for large projects
- **Single Configuration Focus**: Works with one build directory at a time, no multi-config support yet
- **Limited Build System Support**: Only handles CMake projects with `compile_commands.json`
- **No Incremental Updates**: Requires full re-indexing when switching build configurations

### Future Considerations

- **Fail-Fast Indexing**: Could provide partial results with indexing progress info, letting AI agents decide whether to wait
- **Multi-Configuration Support**: Enable simultaneous analysis across Debug/Release/custom builds
- **Build System Expansion**: Support for other build systems beyond CMake
- **Incremental Analysis**: Smarter indexing updates when build configurations change

### Architecture Decisions

The current design prioritizes accuracy over speed by waiting for complete indexing. This ensures reliable symbol information but can introduce latency. The trade-off between response time and accuracy may be configurable in future versions, allowing AI agents to choose between fast partial results or complete analysis.
