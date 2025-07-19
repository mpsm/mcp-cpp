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
- **Python CLI Tool (`mcp-cli.py`)** - Standalone command-line interface for easy MCP server interaction

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

tools/
├── mcp-cli.py       // Standalone Python CLI for MCP server interaction
├── requirements.txt // Python dependencies (rich>=13.0.0)
└── generate-index.py // Symbol indexing tool
```

### 🎯 Current Capabilities

1. **Build Management**: Automatic CMake build directory detection and configuration analysis
2. **Symbol Search**: Fuzzy search across C++ codebases with project/external filtering
3. **Symbol Analysis**: Deep analysis with inheritance hierarchies, call patterns, and usage examples
4. **Project Intelligence**: Smart filtering between project code and external dependencies
5. **Indexing Management**: Real-time clangd indexing progress tracking and completion detection
6. **Command-Line Interface**: Complete Python CLI tool for easy terminal-based interaction

## Python CLI Tool (`mcp-cli.py`)

### **Why This Tool is Valuable**

The Python CLI tool provides a **convenient command-line interface** to the MCP server, making it accessible for:

- **Quick code exploration** without setting up full MCP clients
- **Scripting and automation** of code analysis tasks
- **Testing and debugging** MCP server functionality
- **Integration** with existing development workflows and shell scripts
- **Educational purposes** to understand C++ codebase structure

### **How I Can Use This Tool**

**Basic Usage Pattern:**
```bash
# Navigate to any C++ project with CMake
cd /path/to/cpp/project

# Set clangd path if needed (for version compatibility)
export CLANGD_PATH=/usr/bin/clangd-20

# Use the CLI tool
python3 /path/to/mcp-cpp/tools/mcp-cli.py [COMMAND] [OPTIONS]
```

**Available Commands:**

1. **Project Analysis:**
   ```bash
   # Analyze build environment and CMake configuration
   python3 tools/mcp-cli.py list-build-dirs
   ```

2. **Symbol Search:**
   ```bash
   # Find symbols quickly
   python3 tools/mcp-cli.py search-symbols "Math"
   python3 tools/mcp-cli.py search-symbols "vector" --max-results 20
   python3 tools/mcp-cli.py search-symbols "std::" --include-external
   ```

3. **Deep Symbol Analysis:**
   ```bash
   # Comprehensive symbol analysis
   python3 tools/mcp-cli.py analyze-symbol "Math::factorial" --include-usage-patterns
   python3 tools/mcp-cli.py analyze-symbol "MyClass" --include-inheritance --include-call-hierarchy
   ```

4. **Tool Discovery:**
   ```bash
   # See all available MCP tools
   python3 tools/mcp-cli.py list-tools
   ```

**Output Modes:**
- **Pretty Mode (default)**: Rich formatted tables, colors, structured display
- **Raw Mode**: Clean JSON for scripting: `--raw-output`

**Key Options:**
- `--server-path`: Specify custom MCP server binary location
- `--raw-output`: Get JSON output instead of pretty formatting
- All tool-specific parameters supported (build directories, filtering, analysis depth, etc.)

### **When I Should Use This Tool**

**Immediate Use Cases:**
- **Code exploration**: Quickly understand unfamiliar C++ codebases
- **Symbol lookup**: Find function definitions, class hierarchies, usage patterns
- **Build troubleshooting**: Analyze CMake configuration and compilation database status
- **Architecture analysis**: Understand class relationships and call patterns
- **Development workflow**: Integrate into shell scripts for automated code analysis

**Advantages Over Direct MCP Server:**
- **No JSON-RPC knowledge required** - simple command-line interface
- **Built-in formatting** - human-readable output without parsing JSON
- **Comprehensive help** - detailed documentation for each command
- **Error handling** - user-friendly error messages
- **Shell integration** - works seamlessly in terminal workflows

**Example Workflow:**
```bash
# 1. Analyze project structure
python3 tools/mcp-cli.py list-build-dirs

# 2. Find symbols of interest
python3 tools/mcp-cli.py search-symbols "Calculator" --kinds class function

# 3. Deep dive into specific symbols
python3 tools/mcp-cli.py analyze-symbol "Calculator::compute" --include-usage-patterns

# 4. Export results for further processing
python3 tools/mcp-cli.py search-symbols "Math::" --raw-output > math_symbols.json
```

This tool essentially **democratizes access** to the powerful MCP server capabilities, making semantic C++ code analysis available through simple command-line operations.

## Key Design Principles

1. **Performance First**: Handle large C++ codebases efficiently with smart caching
2. **Robust LSP Integration**: Connection lifecycle, retry logic, graceful degradation
3. **MCP Protocol Compliance**: Use rust-mcp-sdk for proper MCP implementation
4. **Comprehensive Testing**: Unit tests for core logic with CI/CD pipeline
5. **Structured Error Handling**: Use thiserror for MCP-compatible errors
6. **Accessible Interface**: Python CLI tool for easy command-line interaction

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

# Use the Python CLI tool
cd tools && pip3 install -r requirements.txt  # Install dependencies
python3 mcp-cli.py --help   # Get help
python3 mcp-cli.py list-tools
```

## Repository Structure

- `src/`: Rust source code with modular LSP and tool implementations
- `tools/`: Utility tools and CLI interfaces
  - `mcp-cli.py`: **Standalone Python CLI tool for easy MCP server interaction**
  - `requirements.txt`: Python dependencies for the CLI tool
  - `generate-index.py`: Symbol indexing tool
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
- **Build directory parameter support**: Specify custom build directory or use auto-detection

### `analyze_symbol_context`

Deep symbol analysis for comprehensive understanding:

- Symbol definition and type information extraction
- Class inheritance hierarchy analysis
- Function call hierarchy mapping (incoming/outgoing calls)
- Usage pattern analysis with concrete code examples
- Related symbol discovery and disambiguation support
- **Build directory parameter support**: Specify custom build directory or use auto-detection
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

## Build Directory Management

### Auto-Detection vs Explicit Configuration

Both `search_symbols` and `analyze_symbol_context` tools support flexible build directory configuration:

**Auto-Detection (Default Behavior):**

- Automatically discovers single build directory in current workspace
- Analyzes CMake cache files and compilation database status
- Fails gracefully when multiple or zero build directories found
- Uses `list_build_dirs` tool logic for discovery

**Explicit Configuration:**

- `build_directory` parameter accepts relative or absolute paths
- Validates compile_commands.json presence before proceeding
- Enables working with multiple build configurations
- Supports custom build directory locations outside standard patterns

### clangd Process Management

**Enhanced Startup Behavior:**

- Sets clangd working directory to project root (from CMAKE_SOURCE_DIR)
- Passes build directory via `--compile-commands-dir` argument
- Logs clangd output to `<build_directory>/mcp-cpp-clangd.log`
- Issues warning when changing between non-empty build directories

**Build Directory Changes:**

- Automatically shuts down existing clangd session on directory change
- Warns about potential state inconsistencies during directory switching
- Preserves project root detection from CMake cache analysis
- Maintains indexing progress tracking across sessions

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

**Enhanced Test Identification System:**

The E2E framework now includes a comprehensive test identification system that addresses the issue of UUID-based temp folder names:

- **Descriptive Folder Names**: `list-build-dirs-test-029e7d5a` instead of `test-project-uuid`
- **Test-Aware Logging**: `mcp-cpp-server-list-build-dirs-test.log` instead of generic log names
- **Test Metadata**: Each temp folder contains `.test-info.json` with test context
- **Debug Preservation**: Failed tests can preserve their folders for investigation

**Test Directory Inspector:**

```bash
cd test/e2e
npm run inspect          # View all test directories and metadata
npm run inspect:verbose  # Detailed view with full metadata
npm run inspect:logs     # Include log file analysis
npm run cleanup:dry      # Preview cleanup without removing
npm run cleanup          # Remove all test directories
```

**Complete documentation**: See `test/e2e/README.md` for comprehensive usage guide.

**IMPORTANT E2E Test Debugging Workflow:**

When facing issues with E2E tests, follow this systematic approach:

1. **First, inspect test directories** to identify the failed test run:

   ```bash
   cd test/e2e
   npm run inspect:verbose
   ```

2. **Locate the corresponding logs** for the failed test (look for test-specific log files like `mcp-cpp-server-test-name.log`)

3. **Examine the logs** to understand what actually failed - don't change random things

4. **Use the metadata** in `.test-info.json` to get full context about the test environment

5. **If needed, preserve the test folder** for deeper investigation:

   ```typescript
   await TestHelpers.preserveForDebugging(project, "Reason for investigation");
   ```

6. **Cross-reference clangd logs** when in doubt about MCP server behavior:
   - Check both `mcp-cpp-server-test-name.log` and `mcp-cpp-clangd-test-name.log` from the same test run
   - Clangd logs can validate whether the MCP server is communicating correctly with the LSP
   - Look for LSP request/response patterns, indexing progress, and error messages in clangd logs
   - This helps distinguish between MCP server issues vs LSP/clangd issues

This systematic approach prevents random changes and focuses on actual root causes identified through logs and metadata.

# important-instruction-reminders
Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.
ALWAYS prefer editing an existing file to creating a new one.
NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the User.