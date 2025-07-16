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

- Full MCP server implementation with rust-mcp-sdk
- CMake project analysis and build directory management (`list_build_dirs`)
- Comprehensive C++ symbol search with project boundary detection (`search_symbols`)
- Deep symbol analysis with inheritance and call hierarchy (`analyze_symbol_context`)
- Clangd LSP client with lifecycle management and indexing progress tracking
- Project vs external symbol filtering using compilation database analysis
- Structured JSON responses with comprehensive error handling
- CI/CD pipeline with build, tests, clippy, and security audit

### 🔄 Current Architecture

```
src/
├── main.rs          // MCP server entry point with stdio transport
├── handler.rs       // MCP request handler implementation
├── logging.rs       // Structured logging and MCP message tracing
├── cmake.rs         // CMake project analysis and build directory detection
├── lsp/             // LSP client implementation
│   ├── mod.rs       // Module exports
│   ├── client.rs    // Clangd LSP client with connection management
│   ├── manager.rs   // LSP lifecycle and file management
│   ├── types.rs     // LSP types and indexing state tracking
│   └── error.rs     // LSP error handling
└── tools/           // MCP tool implementations
    ├── mod.rs       // Tool registration and routing
    ├── cmake_tools.rs        // Build directory analysis
    ├── search_symbols.rs     // C++ symbol search
    ├── analyze_symbols.rs    // Deep symbol analysis
    └── symbol_filtering.rs   // Project boundary and filtering logic
```

### 🎯 Current Capabilities

1. **Build Management**: Automatic CMake build directory detection and configuration analysis
2. **Symbol Search**: Fuzzy search across C++ codebases with project/external filtering
3. **Symbol Analysis**: Deep analysis with inheritance hierarchies, call patterns, and usage examples
4. **Project Intelligence**: Smart filtering between project code and external dependencies
5. **Indexing Management**: Real-time clangd indexing progress tracking and completion detection

## Key Design Principles

1. **Performance First**: Handle large C++ codebases efficiently with smart caching
2. **Robust LSP Integration**: Connection lifecycle, retry logic, graceful degradation
3. **MCP Protocol Compliance**: Use rust-mcp-sdk for proper MCP implementation
4. **Comprehensive Testing**: Unit tests for core logic with CI/CD pipeline
5. **Structured Error Handling**: Use thiserror for MCP-compatible errors

## Development Commands

```bash
# Build the project
cargo build --release

# Run tests with CI pipeline locally
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check

# Run the MCP server
cargo run

# Development with watch mode
cargo watch -x test        # Auto-run tests on file changes
cargo watch -x run         # Auto-restart server on changes
```

## Repository Structure

- `src/`: Rust source code with modular LSP and tool implementations
- `test/`: Test projects and fixtures for validation
  - `test/e2e/`: End-to-end testing framework (Node.js/TypeScript)
  - `test/test-project/`: Base C++ project for testing MCP tools
  - `test/requests/`: Sample MCP request JSON files
- `.github/workflows/`: CI/CD pipeline with parallel job execution
- `Cargo.toml`: Project dependencies and configuration

## Current Tools

### `list_build_dirs`

Analyzes C++ project build environment including:

- Automatic CMake build directory discovery
- Build configuration analysis (generator, build type, compiler settings)
- Compilation database status and file count
- JSON-structured responses with comprehensive project metadata

### `search_symbols`

C++ symbol search with intelligent filtering:

- Fuzzy matching across entire codebase using clangd workspace symbols
- Project boundary detection (project vs external/system symbols)
- Symbol kind filtering (class, function, variable, etc.)
- File-specific search using document symbols
- Configurable result limits and external symbol inclusion

### `analyze_symbol_context`

Deep symbol analysis for comprehensive understanding:

- Symbol definition and type information extraction
- Class inheritance hierarchy analysis
- Function call hierarchy mapping (incoming/outgoing calls)
- Usage pattern analysis with concrete code examples
- Related symbol discovery and disambiguation support

## Implementation Guidelines

### Performance Requirements

- Handle large C++ codebases with complex compilation databases efficiently
- Use smart caching strategies for build artifacts and LSP responses
- Implement indexing progress tracking to avoid blocking operations
- Build incremental analysis to minimize recomputation overhead

### LSP Integration Strategy

- Robust clangd client with full connection lifecycle management
- Real-time indexing progress monitoring with completion detection
- Automatic retry logic and graceful degradation for LSP server failures
- Comprehensive error handling for all LSP communication scenarios
- Efficient file management with automatic opening/closing of documents

### MCP Server Implementation

- Use rust-mcp-sdk for complete MCP protocol compliance
- Implement stdio transport for seamless MCP client integration
- Leverage SDK's resource management and tool registration patterns
- Convert LSP errors to structured MCP responses with proper error codes
- Comprehensive logging for debugging and monitoring

### Code Quality Standards

- Maintain comprehensive unit test coverage for core logic
- Use thiserror for structured error types compatible with MCP SDK
- Include defensive programming practices for all failure modes
- Add structured logging with tracing crate for observability
- Follow Rust best practices with clippy and formatting checks

### CI/CD Pipeline

- Automated build, test, clippy, and security audit on every push
- Parallel job execution for optimal build times
- Smart caching of Cargo registry and build artifacts
- Security vulnerability scanning with cargo audit
- GitHub Actions integration with status badges

## Architecture Overview

### LSP Integration Layer

- **ClangdManager**: Manages clangd process lifecycle, build directory switching, and file operations
- **LspClient**: Handles JSON-RPC communication, request/response management, and notification processing
- **IndexingState**: Tracks real-time indexing progress with completion detection and estimation
- **Error Handling**: Comprehensive LSP error mapping to MCP-compatible responses

### Tool Implementation Layer

- **Symbol Search**: Workspace-wide and file-specific symbol discovery with intelligent filtering
- **Symbol Analysis**: Deep context analysis including inheritance, call hierarchy, and usage patterns
- **Project Management**: Build directory analysis, compilation database parsing, and project boundary detection
- **Filtering Logic**: Smart distinction between project code and external dependencies

### Data Flow Architecture

1. **MCP Request** → Handler parses and validates tool parameters
2. **Build Setup** → Automatic CMake detection and clangd initialization if needed
3. **LSP Communication** → Structured requests to clangd with progress tracking
4. **Response Processing** → Filter, transform, and enrich LSP responses
5. **MCP Response** → Structured JSON output with comprehensive metadata

This implementation provides a complete bridge between MCP clients and C++ semantic analysis, enabling AI agents to work with C++ codebases using the same tools and understanding that human developers rely on.

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
