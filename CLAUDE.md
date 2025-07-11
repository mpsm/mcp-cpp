# C++ MCP Server Project

## Project Overview

This is a **C++ MCP (Model Context Protocol) server** implemented in Rust that bridges AI agents with C++ LSP tools (primarily clangd). The goal is to provide AI agents with the same semantic code understanding capabilities that human C++ developers rely on through intellisense.

## Project Context & Rationale

- **Problem**: AI agents use different approaches to browse code - some rely on text search, others on LSP integration
- **Target**: Large C++ codebases with heavy preprocessor usage where humans rely on intellisense
- **Solution**: Bridge AI agents with C++ LSP tools to provide semantic understanding beyond text search
- **Technology Choice**: Rust for resource efficiency when handling large compilation databases

## Current Implementation Status

### ✅ Completed
- Initial MCP server foundation with rust-mcp-sdk
- CMake project analysis tool (`cpp_project_status`)
- CMake build directory detection and parsing
- Structured JSON responses for tool integration
- Error handling for missing CMakeLists.txt and corrupted cache files

### 🔄 Current Architecture
```
src/
├── main.rs      // MCP server entry point with stdio transport
├── handler.rs   // MCP request handler implementation
├── tools.rs     // Tool implementations (currently: CppProjectStatusTool)
├── cmake.rs     // CMake project analysis logic
└── Cargo.toml   // Dependencies and project configuration
```

### 🎯 Next Steps
1. **LSP Integration**: Implement clangd client for semantic analysis
2. **Navigation Tools**: textDocument/definition, references, implementation
3. **Symbol Tools**: documentSymbol, workspace/symbol, hover
4. **Analysis Tools**: semanticTokens, diagnostics
5. **Hierarchy Tools**: callHierarchy, typeHierarchy

## Key Design Principles

1. **Performance First**: Handle large C++ codebases efficiently
2. **Robust LSP Integration**: Connection lifecycle, retry logic, graceful degradation
3. **MCP Protocol Compliance**: Use rust-mcp-sdk for proper MCP implementation
4. **80% Test Coverage**: Comprehensive testing for reliability
5. **Structured Error Handling**: Use thiserror for MCP-compatible errors

## Development Commands

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run the MCP server
cargo run

# Check code quality
cargo clippy
cargo fmt --check
```

## Repository Structure

- `src/`: Rust source code
- `test/`: Test projects and fixtures for validation
  - `test/e2e/`: End-to-end testing framework (Node.js/TypeScript)
  - `test/test-project/`: Base C++ project for testing
- `Cargo.toml`: Project dependencies and configuration

## Implementation Guidelines

### Performance Requirements
- Handle large C++ codebases with complex compilation databases
- Prioritize efficient algorithms and minimize memory allocations
- Use streaming parsers for large JSON compilation databases
- Build incremental indexes to avoid full recomputation

### LSP Integration Strategy
- Implement robust clangd client with connection lifecycle management
- Include retry logic and graceful degradation for LSP server failures
- Follow JSON-RPC 2.0 and LSP specifications strictly
- Use async operations with tokio to avoid blocking

### MCP Server Implementation
- Use rust-mcp-sdk for proper MCP protocol compliance
- Implement local IO transport (stdin/stdout) for MCP communication
- Leverage SDK's resource management and tool registration patterns
- Convert LSP errors to MCP responses using SDK error handling

### Code Quality Standards
- Maintain 80% test coverage minimum
- Use thiserror for structured error types compatible with MCP SDK
- Include comprehensive error handling for all failure modes
- Add structured logging with tracing crate for observability

### Priority LSP Commands
1. **Navigation**: textDocument/definition, references, implementation
2. **Understanding**: documentSymbol, workspace/symbol, hover
3. **Analysis**: semanticTokens, diagnostics
4. **Hierarchy**: callHierarchy, typeHierarchy

## Current Tool: cpp_project_status

Analyzes C++ project status including:
- CMake project detection
- Build directory scanning
- Configuration parsing (generator, build type, options)
- JSON-structured responses with project metadata

This serves as the foundation template for adding additional C++ development tools.

## Testing Framework

### End-to-End Testing
The project includes a comprehensive E2E testing framework in `test/e2e/`:

**Technology Stack:**
- **Vitest**: Modern, fast test runner with TypeScript support
- **ESLint**: Modern .mjs configuration for code quality
- **Prettier**: External formatting tool (not a module dependency)

**Framework Components:**
- **McpClient**: JSON-RPC communication with MCP server over stdio
- **TestProject**: Test project manipulation and CMake operations
- **TestRunner**: Test execution with isolation and cleanup
- **Assertions**: Rich JSON response validation

**Key Features:**
- Isolated test execution with fresh project copies
- Parallel test execution for speed
- Comprehensive fixture system for different project configurations
- MCP protocol compliance validation
- Support for testing server lifecycle and error states

**Running E2E Tests:**
```bash
# First, build the MCP server (required)
cargo build

cd test/e2e
npm test           # Run all tests
npm run test:ui    # Run with UI interface
npm run test:coverage  # Run with coverage report
npm run lint       # Check code quality
```

**Note**: E2E tests require the MCP server binary to be built first. Tests will fail fast with a clear error message if the binary is not found.