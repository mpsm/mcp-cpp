# Development Context

## Build & Test Commands

```bash
# Rust development workflow
cargo build --release      # Build the project (release mode recommended)
cargo test                 # Run unit tests
cargo run                  # Start MCP server
cargo clippy --all-targets --all-features -- -D warnings  # Lint code (CI standard)
cargo fmt                  # Format code
cargo fmt --check          # Check formatting (CI)

# Development with watch mode
cargo watch -x test        # Auto-run tests on file changes
cargo watch -x run         # Auto-restart server on changes

# CI pipeline locally
cargo build && cargo test && cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --check

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

- **Coverage**: Each module has comprehensive test coverage for core logic
- **Focus**: Symbol filtering, LSP client behavior, error handling, data structures
- **CI Integration**: Automated testing on every push with parallel execution

### End-to-End Tests (Node.js/TypeScript)

- **Framework**: Vitest with TypeScript support and modern tooling
- **Coverage**: Full MCP protocol compliance, server lifecycle, and tool integration
- **Features**:
  - Isolated test execution with fresh project copies
  - Parallel test execution for optimal speed
  - MCP server subprocess management with proper cleanup
  - JSON-RPC protocol validation and compliance testing
  - Test project manipulation (CMake configs, file operations)
  - Real clangd integration testing
- **Tooling**: ESLint (modern .mjs), Prettier (external), comprehensive linting

### CI/CD Pipeline

- **GitHub Actions**: Automated build, test, clippy, and security audit
- **Parallel Jobs**: Format check, build, clippy/test (parallel), security audit
- **Caching**: Smart caching of Cargo registry and build artifacts for speed
- **Security**: Regular dependency vulnerability scanning with cargo audit

## Code Quality Standards

- Use `thiserror` for error handling that integrates with MCP SDK
- Implement `#[instrument]` for tracing in async functions
- Follow LSP and MCP protocol specifications strictly
- Use structured logging with `tracing` crate for debugging and monitoring
- Maintain CI compliance with clippy warnings as errors
- Comprehensive error handling for all LSP communication scenarios
- Smart caching strategies for performance optimization

## Current Dependencies

```toml
async-trait = "0.1.88"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4.5.27", features = ["derive"] }
rust-mcp-sdk = { version = "0.5.0", features = ["macros"] }
serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.140"
thiserror = "2.0.12"
tokio = { version = "1.46.1", features = ["full"] }
tokio-util = { version = "0.7.15", features = ["codec"] }
tracing = "0.1.41"
tracing-subscriber = { version = "0.3.19", features = ["json", "env-filter"] }
walkdir = "2.5.0"
uuid = { version = "1.17.0", features = ["v4", "serde"] }
regex = "1.11.1"
sha2 = "0.10"
```

## Feature Development Workflow

1. **Write tests first** - Define expected behavior with comprehensive test cases
2. **Implement minimal viable version** - Core functionality with error handling
3. **Add instrumentation** - Structured logging and performance monitoring
4. **Performance optimization** - Verify with large codebases and implement caching
5. **Integration testing** - Test with real clangd, various build configurations, and MCP clients
6. **CI validation** - Ensure all pipeline checks pass (build, test, clippy, security)

## Architecture Notes

- **MCP Protocol**: Uses rust-mcp-sdk 0.5.0 with macro support for stdin/stdout transport
- **Error Handling**: Comprehensive `LspError` type with MCP-compatible conversions
- **Async Architecture**: Built on tokio with non-blocking LSP communication and lifecycle management
- **Modularity**: Separate modules for LSP client, tools, symbol filtering, and project management
- **Performance**: Smart caching, indexing progress tracking, and efficient file management

## Current Implementation Status

The project currently implements a complete C++ semantic analysis server:

### Core Tools

- **`list_build_dirs`**: CMake build environment discovery and configuration analysis
- **`search_symbols`**: Comprehensive C++ symbol search with project boundary detection
- **`analyze_symbol_context`**: Deep symbol analysis with inheritance, call hierarchy, and usage patterns

### Infrastructure

- **LSP Integration**: Full clangd client with lifecycle management and real-time indexing progress
- **Project Intelligence**: Smart filtering between project code and external dependencies
- **Build Management**: Automatic CMake detection, compilation database parsing, and build switching
- **Error Handling**: Comprehensive error mapping from LSP to MCP with structured responses
- **Performance**: Efficient caching strategies and indexing completion detection

### Quality Assurance

- **CI/CD Pipeline**: Automated testing, linting, security scanning, and artifact generation
- **Testing Framework**: Comprehensive unit tests and E2E testing with real clangd integration
- **Documentation**: Complete API documentation and usage examples

This implementation provides production-ready C++ semantic analysis capabilities for AI agents through the MCP protocol.
