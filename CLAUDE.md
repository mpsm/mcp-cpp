# C++ MCP Server Project

## 🤝 Collaboration Style Preference

**Technical Peer Collaboration Mode:**
- **Direct, honest technical opinions** - Call out anti-patterns and architectural issues directly
- **Equal footing partnership** - Co-architects debating design decisions and building consensus
- **Constructive disagreement encouraged** - Push back on current approaches while offering concrete alternatives
- **Shared problem-solving** - Build on each other's ideas collaboratively rather than just responding to requests
- **Practical trade-off focus** - Stay grounded in real maintainability vs complexity decisions
- **Casual but substantive tone** - Technical depth with informal, enthusiastic communication

**What works:** Instead of helpful assistant mode, engage as **technical teammate** who can disagree, offer strong opinions, and get excited when ideas align. Focus on sustainable architecture decisions together.

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
- **FIXED: Result limiting architecture** - Proper client-side limiting preserving clangd ranking

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
    ├── search_symbols.rs     // C++ symbol search with FIXED result limiting
    ├── analyze_symbols.rs    // Deep symbol analysis
    └── symbol_filtering.rs   // Project boundary and filtering logic with comprehensive tests

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
7. **Proper Result Limiting**: Fixed clangd communication issue for predictable symbol counts

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
7. **Quality Standards**: Always run cargo fmt and clippy after Rust changes

## Development Commands

```bash
# Build the project
cargo build --release

# Run tests with CI pipeline locally
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check

# IMPORTANT: Always run after Rust code changes
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings

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

### `search_symbols` - **FIXED Result Limiting Architecture**

C++ symbol search with intelligent filtering:

- Fuzzy matching across entire codebase using clangd workspace symbols
- Project boundary detection (project vs external/system symbols)
- Symbol kind filtering (class, function, variable, etc.)
- File-specific search using document symbols
- **FIXED: Proper result limiting** - clangd queried with 2000 limit, user max_results applied client-side
- **Build directory parameter support**: Specify custom build directory or use auto-detection

**Critical Fix Applied:**

- **Problem**: `max_results` was passed directly to clangd, causing premature limiting
- **Solution**: Fixed 2000 limit to clangd, user's `max_results` applied in post-processing
- **Result**: Predictable symbol counts, preserved clangd relevance ranking
- **Testing**: 10 comprehensive unit tests added for all edge cases

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
- **Proper result limiting**: Use fixed large limits for clangd, apply user limits client-side

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
- **ALWAYS run `cargo fmt` and `cargo clippy` after Rust code changes**
- **Add unit tests for any new functionality that provides value to project quality**

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
- **Result Limiting**: Fixed 2000-limit clangd queries with client-side user limit application

### Data Flow Architecture

1. **MCP Request** → Handler parses and validates tool parameters
2. **Build Setup** → Automatic CMake detection and clangd initialization if needed
3. **LSP Communication** → Structured requests to clangd with fixed large limits (2000) for comprehensive results
4. **Response Processing** → Filter, transform, and enrich LSP responses, apply user limits client-side
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

## **🔍 ADVANCED E2E TEST DEBUGGING SYSTEM**

### **Automatic Failure Preservation**

The E2E framework automatically preserves failed test environments for debugging:

#### **✅ Automatic Behavior (No Manual Intervention Required):**

- **Failed tests are automatically detected** via Vitest context
- **Test folders are preserved** with complete environment and logs
- **Specific test case information** is captured in metadata
- **Rich debugging information** is included for analysis

#### **🎯 Enhanced Test Case Identification:**

When tests fail, you get detailed information about the specific failing test:

```bash
npm run inspect:verbose
```

**Sample Output:**

```
🔍 search-symbols-test-83fd7fb3
   🔍 PRESERVED FOR DEBUGGING
   🎯 Failed Test Case: should handle non-existent file gracefully
   📄 Test File: src/tests/search-symbols.test.ts
   📍 Full Path: File-specific search > should handle non-existent file gracefully
   ❌ Error: expected 1 to be +0 // Object.is equality
   ⏱️  Duration: 2130ms
```

### **🛠️ Debugging Workflow**

#### **1. Automatic Detection & Preservation**

- Tests failing? **No manual action needed** - folders are preserved automatically
- Multiple test failures? **Each gets its own preserved folder** with specific test case details
- **Rich metadata** includes error messages, test hierarchy, and execution context

#### **2. Identify Failed Tests**

```bash
cd test/e2e
npm run inspect:verbose  # Shows all preserved test failures with specific details
```

#### **3. Investigate Specific Failures**

Each preserved folder contains:

- **`.test-info.json`** - Complete test environment metadata
- **`.debug-preserved.json`** - Specific failure information with test case details
- **`mcp-cpp-server-*.log`** - MCP server logs for this specific test
- **`build-debug/mcp-cpp-clangd.log`** - clangd LSP logs in the build directory
- **Complete project files** - Exact state when test failed
- **Build artifacts** - Including `compile_commands.json` and CMake cache

#### **4. Analyze Failure Context**

```typescript
// Example debug metadata structure:
{
  "testCase": {
    "testCase": "should handle non-existent file gracefully",
    "testFile": "src/tests/search-symbols.test.ts",
    "fullName": "File-specific search > should handle non-existent file gracefully",
    "errors": ["expected 1 to be +0 // Object.is equality"],
    "duration": 2130
  },
  "preservedAt": "2025-07-19T19:30:12.721Z",
  "reason": "Test case \"should handle non-existent file gracefully\" failed - folder preserved automatically"
}
```

#### **5. Advanced Log Analysis**

**MCP Server Logs:**

```bash
# View test-specific server logs
cat temp/search-symbols-test-*/mcp-cpp-server-search-symbols-test.*.log
```

**clangd LSP Logs (Located in Build Directory):**

```bash
# clangd logs are in the build directory of the preserved test folder
cat temp/search-symbols-test-*/build-debug/mcp-cpp-clangd.log

# Or use find to locate clangd logs
find temp/search-symbols-test-* -name "mcp-cpp-clangd.log" -exec cat {} \;
```

**Key Log Analysis Points:**

- **MCP server logs** show request/response handling, tool execution, errors
- **clangd logs** show LSP communication, indexing progress, compilation issues
- **Build directory state** shows compilation database and CMake configuration
- **Cross-reference timestamps** between server and clangd logs for correlation

#### **6. Build Environment Investigation**

```bash
# Check compilation database in preserved test
cat temp/search-symbols-test-*/build-debug/compile_commands.json

# Check CMake configuration
cat temp/search-symbols-test-*/build-debug/CMakeCache.txt

# Verify build directory structure
ls -la temp/search-symbols-test-*/build-debug/
```

### **🎯 Test Categories & Preservation Strategy**

#### **E2E Tests (Automatic Preservation):**

- `search-symbols.test.ts` - MCP server symbol search functionality
- `list-build-dirs.test.ts` - MCP server build directory analysis
- `example-with-context.test.ts` - Example E2E test patterns

**✅ These automatically preserve on failure** - no manual intervention needed

#### **Framework Unit Tests (Standard Cleanup):**

- `TestProject.test.ts` - Framework unit tests

**❌ These use standard cleanup** - no preservation needed for framework testing

### **🧹 Cleanup Management**

```bash
# View all preserved test failures
npm run inspect

# Clean up after investigation
npm run cleanup

# Preview what would be cleaned
npm run cleanup:dry
```

### **📋 Manual Preservation (Advanced)**

For custom debugging scenarios:

```typescript
// In test code - preserve folder manually
await project.preserveForDebugging("Custom investigation reason");

// Or via TestHelpers
await TestHelpers.preserveForDebugging(project, "Debugging specific scenario");
```

### **🚀 Best Practices**

1. **Let the system work automatically** - most failures are preserved without intervention
2. **Use `npm run inspect:verbose`** to identify specific failing test cases
3. **Check error messages first** - often the clearest indicator of root cause
4. **Check clangd logs in build directory** - `build-debug/mcp-cpp-clangd.log` for LSP issues
5. **Correlate server and clangd logs** to distinguish MCP vs LSP issues
6. **Examine build artifacts** - compilation database and CMake state in build directories
7. **Don't change random things** - use preserved state to understand actual problems

This comprehensive debugging system ensures **no test failure goes uninvestigated** and provides **complete context** for understanding and fixing issues.

## Important Development Practices

### **CRITICAL: Always Run Quality Checks After Rust Changes**

After making any changes to Rust code, **ALWAYS** run these commands in order:

```bash
# 1. Format code
cargo fmt

# 2. Check for lint issues
cargo clippy --all-targets --all-features -- -D warnings

# 3. Run relevant tests
cargo test [specific_test_pattern]

# 4. Build to verify compilation
cargo build
```

### **Unit Testing Philosophy**

- **Add unit tests for any changes that provide value to project quality**
- Focus on edge cases, boundary conditions, and error scenarios
- Test public APIs and critical internal logic
- Ensure tests are fast, isolated, and deterministic
- Use descriptive test names that explain the scenario being tested
- **Don't test obvious things** - Skip trivial getters, simple constructors, and basic collection operations
- **Design tests that are not fragile** - Test behavioral contracts, not implementation details; should withstand future changes
- **Focus on non-obvious business logic** - Deduplication algorithms, grouping logic, error handling paths, and complex interactions

### **Result Limiting Architecture (FIXED)**

**Previous Issue:**

- `max_results` parameter was incorrectly passed directly to clangd LSP requests
- This caused clangd to limit its internal response, potentially returning fewer symbols than available
- User could request 50 symbols but only get 30 if clangd's filtering reduced results

**Current Solution:**

- clangd is always queried with a fixed 2000 symbol limit for comprehensive results
- User's `max_results` parameter is applied in post-processing on the MCP server side
- This preserves clangd's relevance ranking while ensuring predictable result counts
- Implementation thoroughly tested with 10 comprehensive unit tests

**Key Learning:**

- LSP communication should use large fixed limits to get comprehensive data
- Client-side filtering should handle user preferences and result limiting
- This pattern applies to any tool that bridges between LSP and MCP protocols

# important-instruction-reminders

Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.
ALWAYS prefer editing an existing file to creating a new one.
NEVER proactively create documentation files (\*.md) or README files. Only create documentation files if explicitly requested by the User.

# important-instruction-reminders

Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.

      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context or otherwise consider it in your response unless it is highly relevant to your task. Most of the time, it is not relevant.
