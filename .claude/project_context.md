# Development Context

## Build & Test Commands

```bash
# Rust development workflow
cargo build                 # Build the project
cargo test                  # Run unit tests
cargo run                   # Start MCP server
cargo clippy               # Lint code
cargo fmt                  # Format code

# Development with watch mode
cargo watch -x test        # Auto-run tests on file changes
cargo watch -x run         # Auto-restart server on changes

# End-to-end testing (requires Node.js and built MCP server)
cargo build                 # Build MCP server first (required)
cd test/e2e                 # Navigate to E2E framework
npm test                    # Run all E2E tests
npm run test:ui             # Run with UI interface
npm run test:coverage       # Run with coverage report
npm run lint                # Check TypeScript code quality
npm run lint:fix            # Fix linting issues
```

## Testing Strategy

### Unit Tests (Rust)
- **Coverage**: Each module (cmake.rs, tools.rs, handler.rs) has comprehensive test coverage
- **Target**: 80% minimum test coverage for Rust code
- **Focus**: Core logic, error handling, data structures

### End-to-End Tests (Node.js/TypeScript)
- **Framework**: Vitest with TypeScript support
- **Coverage**: Full MCP protocol compliance and server lifecycle
- **Features**: 
  - Isolated test execution with fresh project copies
  - Parallel test execution for speed
  - MCP server subprocess management
  - JSON-RPC protocol validation
  - Test project manipulation (CMake configs, file operations)
- **Tooling**: ESLint (modern .mjs), Prettier (external)

## Code Quality Standards

- Use `thiserror` for error handling that integrates with MCP SDK
- Implement `#[instrument]` for tracing in async functions
- Follow LSP and MCP protocol specifications strictly
- Use structured logging with `tracing` crate

## Current Dependencies

```toml
async-trait = "0.1.88"
rust-mcp-sdk = { git = "https://github.com/rust-mcp-stack/rust-mcp-sdk", version = "0.5.0" }
schemars = "1.0.4"
serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.140"
thiserror = "2.0.12"
tokio = { version = "1.46.1", features = ["full"] }
tracing = "0.1.41"
tracing-subscriber = "0.3.19"
walkdir = "2.5.0"
```

## Feature Development Workflow

1. **Write tests first** - Define expected behavior
2. **Implement minimal viable version** - Core functionality
3. **Add error handling** - Handle all failure modes
4. **Add instrumentation** - Logging and metrics
5. **Performance test** - Verify with large codebases
6. **Integration test** - Test with real clangd and MCP clients

## Architecture Notes

- **MCP Protocol**: Uses rust-mcp-sdk for stdin/stdout transport
- **Error Handling**: Custom `LspBridgeError` type converts to MCP errors
- **Async**: Built on tokio for non-blocking LSP communication
- **Modularity**: Separate modules for LSP, tools, resources, compilation handling

## Current Implementation Status

The project currently implements:
- **CMake Analysis Tool**: Scans build directories, parses CMakeCache.txt
- **MCP Server Foundation**: Basic server setup with tool registration
- **Error Handling**: Structured error types with MCP integration
- **JSON Responses**: Comprehensive project status information

This foundation enables adding additional C++ development tools following the established patterns.