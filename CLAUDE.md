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
├── main.rs              // MCP server entry point with stdio transport
├── logging.rs           // Structured logging and MCP message tracing
├── test_utils.rs        // Testing utilities and helpers
├── clangd/              // Clangd LSP integration layer
│   ├── mod.rs           // Module exports
│   ├── session.rs       // Clangd session lifecycle management
│   ├── session_builder.rs // Session configuration and builder pattern
│   ├── file_manager.rs  // LSP document and file operations
│   ├── config.rs        // Clangd configuration management
│   ├── version.rs       // Clangd version detection and compatibility
│   ├── testing.rs       // Testing utilities for clangd integration
│   ├── error.rs         // Clangd-specific error handling
│   └── index/           // Indexing and progress monitoring
│       ├── mod.rs       // Index module exports
│       ├── monitor.rs   // Real-time indexing progress tracking
│       ├── hash.rs      // Index hash computation and validation
│       └── project_index.rs // Project-specific index management
├── lsp/                 // LSP protocol implementation
│   ├── mod.rs           // Module exports
│   ├── client.rs        // JSON-RPC LSP client communication
│   ├── protocol.rs      // LSP types and protocol definitions
│   ├── framing.rs       // JSON-RPC message framing
│   ├── jsonrpc_utils.rs // JSON-RPC utilities and helpers
│   ├── traits.rs        // LSP client traits and interfaces
│   └── testing.rs       // LSP testing utilities
├── io/                  // I/O and transport layer
│   ├── mod.rs           // Module exports
│   ├── process.rs       // Process management and lifecycle
│   └── transport.rs     // Transport abstractions
├── project/             // Multi-provider project analysis
│   ├── mod.rs           // Module exports
│   ├── workspace.rs     // Project workspace management
│   ├── workspace_session.rs // Workspace session state management
│   ├── scanner.rs       // Multi-provider project scanning
│   ├── provider.rs      // Build system provider trait
│   ├── component.rs     // Project component abstraction
│   ├── cmake_provider.rs // CMake project provider
│   ├── meson_provider.rs // Meson project provider
│   ├── compilation_database.rs // Compilation database handling
│   └── error.rs         // Project-specific error handling
└── mcp_server/          // MCP server implementation
    ├── mod.rs           // Module exports
    ├── server.rs        // Core MCP server handler
    ├── server_helpers.rs // Server utility functions
    └── tools/           // MCP tool implementations
        ├── analyze_symbols.rs  // Deep symbol analysis
        ├── search_symbols.rs   // C++ symbol search with filtering
        ├── project_tools.rs    // Project analysis and build configuration
        └── utils.rs            // Tool utility functions

tools/
├── mcp-cli.py           // Standalone Python CLI for MCP server interaction
├── requirements.txt     // Python dependencies (rich>=13.0.0)
├── generate-index.py    // Symbol indexing tool
└── clangd-idx-viewer.py // Clangd index debugging utility
```

### 🎯 Current Capabilities

1. **Multi-Provider Build Management**: Automatic detection and analysis of CMake and Meson projects with comprehensive configuration analysis
2. **Advanced Symbol Search**: Fuzzy search across C++ codebases with intelligent project/external filtering and relevance ranking
3. **Deep Symbol Analysis**: Comprehensive analysis with inheritance hierarchies, call patterns, usage examples, and contextual understanding
4. **Project Intelligence**: Smart filtering between project code and external dependencies using compilation database analysis
5. **Real-time Indexing Management**: Clangd indexing progress tracking with completion detection and hash-based validation
6. **Session Management**: Advanced clangd session lifecycle with configuration builder patterns and automatic recovery
7. **Command-Line Interface**: Complete Python CLI tool for easy terminal-based interaction with rich formatting
8. **Global Compilation Database**: Override support for unified compilation databases across multi-component projects
9. **Workspace Session Management**: Persistent workspace state with multi-provider project scanning and component tracking

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
   # Get comprehensive project details including build configurations and components
   python3 tools/mcp-cli.py get-project-details
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
   # Comprehensive symbol analysis (automatic analysis based on symbol type)
   python3 tools/mcp-cli.py analyze-symbol "Math::factorial" --max-examples 5
   python3 tools/mcp-cli.py analyze-symbol "MyClass" --location-hint "/path/file.cpp:42:15"
   python3 tools/mcp-cli.py analyze-symbol "factorial" --wait-timeout 0 --no-code
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
# 1. Analyze project structure and build configurations
python3 tools/mcp-cli.py get-project-details

# 2. Find symbols of interest
python3 tools/mcp-cli.py search-symbols "Calculator" --kinds class function

# 3. Deep dive into specific symbols
python3 tools/mcp-cli.py analyze-symbol "Calculator::compute" --include-usage-patterns

# 4. Export results for further processing
python3 tools/mcp-cli.py search-symbols "Math::" --raw-output > math_symbols.json
```

This tool essentially **democratizes access** to the powerful MCP server capabilities, making semantic C++ code analysis available through simple command-line operations.

## Development Workflow Integration

### Mandatory Subagent Workflows

This project uses specialized subagents for specific engineering workflows:

- **software-architect**: (MANDATORY for all feature design/refactoring) Structural design using SOLID principles, dependency management, separation of concerns, testability-first design, integration patterns, refactoring strategies for maintainability

- **code-quality-engineer**: (MANDATORY for all implementation reviews) Quality enforcement with zero tolerance for compromises, anti-pattern detection, code standards compliance, performance assessment, security best practices validation, technical debt identification

- **quality-engineer**: (MANDATORY for troubleshooting beyond basic issues) Systematic root cause analysis without modifying test subjects, infrastructure assessment, testing gap identification, observability evaluation, validation harness analysis, comprehensive testing insights for difficult problems

- **code-committer**: (MANDATORY for commit preparation) Meaningful commit messages focusing on why first, what second, proper authorship attribution, co-author credits - ALWAYS display commit message to user for acceptance before committing

### Quality Gates (Non-Negotiable)

- `cargo fmt` - Code formatting
- `cargo clippy --all-targets --all-features -- -D warnings` - Static analysis
- `cargo test` - Unit test validation
- `cargo build` - Compilation verification

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

### `get_project_details`

Comprehensive multi-provider project analysis including:

- **Multi-Provider Discovery**: Automatic detection of CMake and Meson projects with extensible architecture
- **Build Configuration Analysis**: Generator type, build type, compiler settings, and feature flags
- **Component Management**: Unified component representation across all build systems
- **Compilation Database Intelligence**: Global and per-component compile_commands.json analysis
- **Project Structure Mapping**: Build directories, source roots, and provider type identification
- **Workspace Overview**: Project name, root directory, component count, and discovery metadata
- **LSP Integration Assessment**: Compatibility validation across all discovered build systems
- **Fresh Scan Support**: Optional `path` and `depth` parameters trigger fresh scans when different from cached values

### `search_symbols` - **FIXED Result Limiting Architecture**

Advanced C++ symbol search with dual-mode operation:

- **Workspace Search Mode** (default): Fuzzy matching across entire codebase using clangd workspace symbols (may be incomplete due to clangd heuristics)
- **Document Search Mode** (with `files` parameter): Comprehensive symbol search within specific files using document symbols (predictable results)
- Project boundary detection (project vs external/system symbols)
- Symbol kind filtering (Class, Function, Method, Variable, etc. - PascalCase)
- **FIXED: Proper result limiting** - clangd queried with 2000 limit, user max_results applied client-side preserving ranking
- **Build directory parameter support**: Specify custom build directory or use auto-detection
- **Indexing timeout control**: `wait_timeout` parameter (default 20s, 0 = no wait)
- **Empty query support**: Allowed only with `files` parameter for comprehensive file symbol listing

**Critical Fix Applied:**

- **Problem**: `max_results` was passed directly to clangd, causing premature limiting
- **Solution**: Fixed 2000 limit to clangd, user's `max_results` applied in post-processing
- **Result**: Predictable symbol counts, preserved clangd relevance ranking
- **Testing**: 10 comprehensive unit tests added for all edge cases

**Usage Examples:**

```bash
# Workspace search - find symbols across entire codebase
search_symbols {"query": "vector", "max_results": 10}
search_symbols {"query": "Math::factorial", "kinds": ["Function", "Method"]}

# Document search - comprehensive symbol overview of specific files
search_symbols {"query": "", "files": ["include/Math.hpp"], "max_results": 20}
search_symbols {"query": "", "files": ["src/calculator.cpp"], "kinds": ["Function", "Class"]}

# Type filtering - find specific symbol types
search_symbols {"query": "Process", "kinds": ["Class", "Struct", "Interface"]}
search_symbols {"query": "", "files": ["include/"], "kinds": ["Constructor", "Method"]}

# External symbols - include system/library symbols
search_symbols {"query": "std::", "include_external": true, "max_results": 5}
```

### `analyze_symbol_context`

Comprehensive C++ symbol analysis with automatic multi-dimensional context extraction:

- **Automatic Analysis** (always included when applicable):
  - Symbol definition and type information extraction
  - Usage examples from code references (configurable via `max_examples`)
  - Type hierarchy analysis for classes/structs/interfaces (automatic)
  - Call hierarchy analysis for functions/methods/constructors (automatic)
  - Class member enumeration for structural types (automatic)
- **Advanced Disambiguation**: `location_hint` parameter for overloaded symbols (format: "/path/file.cpp:line:column")
- **Build directory parameter support**: Specify custom build directory or use auto-detection
- **Indexing timeout control**: `wait_timeout` parameter (default 20s, 0 = no wait)
- **Symbol Resolution Modes**: Workspace symbol resolution (default) or location-specific resolution (with location_hint)

**Note**: The tool automatically determines what analysis to perform based on symbol type - no manual flags required for inheritance, call hierarchy, or usage patterns.

## Implementation Guidelines

### Performance Requirements

- Handle large C++ codebases with complex compilation databases efficiently
- Use smart caching strategies for build artifacts and LSP responses
- Implement indexing progress tracking to avoid blocking operations
- Build incremental analysis to minimize recomputation overhead

### Clangd Integration Strategy (`clangd/` module)

- **Session Builder Pattern**: Use `session_builder.rs` for flexible clangd configuration and startup
- **Index Monitoring**: Leverage `index/monitor.rs` for real-time indexing progress tracking with hash validation
- **File Management**: Utilize `file_manager.rs` for LSP document operations with automatic state tracking
- **Version Compatibility**: Use `version.rs` for clangd version detection and compatibility checks
- **Error Recovery**: Implement automatic session restart on failures with comprehensive error handling

### Multi-Provider Project Management (`project/` module)

- **Unified Scanning**: Use `scanner.rs` with multiple providers for comprehensive project discovery
- **Provider Architecture**: Implement new build systems using the `provider.rs` trait pattern
- **Workspace Sessions**: Leverage `workspace_session.rs` for persistent state management across components
- **Component Abstraction**: Use `component.rs` for unified representation across all build systems
- **Compilation Database**: Utilize `compilation_database.rs` for global and per-component analysis

### MCP Server Implementation (`mcp_server/` module)

- **Tool Registration**: Use `server.rs` for MCP protocol compliance and tool routing
- **Result Processing**: Implement intelligent filtering in `tools/utils.rs` with client-side limiting
- **Error Handling**: Leverage `server_helpers.rs` for structured MCP error responses
- **Workspace Integration**: Connect MCP tools with project workspace for context-aware analysis

### LSP Protocol Layer (`lsp/` module)

- **JSON-RPC Communication**: Use `client.rs` for reliable LSP communication with proper framing
- **Protocol Compliance**: Leverage `protocol.rs` for complete LSP type definitions
- **Message Framing**: Utilize `framing.rs` for robust JSON-RPC message handling
- **Testing**: Use `testing.rs` utilities for comprehensive LSP integration testing

### CI/CD Pipeline

- Automated build, test, clippy, and security audit on every push
- Parallel job execution for optimal build times
- Smart caching of Cargo registry and build artifacts
- Security vulnerability scanning with cargo audit
- GitHub Actions integration with status badges

## Multi-Provider Build System Management

### Comprehensive Project Discovery

The system provides unified build system support through a multi-provider architecture:

**Supported Build Systems:**

- **CMake**: Full CMake project analysis with build directory discovery and cache parsing
- **Meson**: Native Meson project support with build configuration analysis
- **Extensible Architecture**: Ready for Bazel, Buck, xmake, and other build systems

**Auto-Detection Behavior:**

- Automatically discovers all build system types in current workspace
- Analyzes configuration files (CMakeLists.txt, meson.build, etc.) and build directories
- Unified component representation across all providers
- Uses `get_project_details` tool logic for comprehensive discovery

**Multi-Component Projects:**

- `build_directory` parameter accepts relative or absolute paths for specific components
- Validates compile_commands.json presence across all discovered build systems
- Enables working with mixed build configurations (CMake + Meson in same project)
- Global compilation database override for unified analysis across all components

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

### Clangd Integration Layer (`clangd/`)

- **Session Management**: Advanced clangd session lifecycle with builder patterns and automatic recovery
- **Index Monitoring**: Real-time indexing progress tracking with hash-based validation and completion detection
- **File Management**: LSP document operations with automatic opening/closing and state tracking
- **Configuration**: Clangd-specific configuration management with version compatibility checks
- **Error Handling**: Comprehensive clangd error mapping to structured responses

### LSP Protocol Layer (`lsp/`)

- **JSON-RPC Client**: Full LSP client implementation with message framing and protocol compliance
- **Protocol Types**: Complete LSP type definitions and protocol abstractions
- **Transport**: Reliable JSON-RPC communication with request/response management
- **Testing**: Comprehensive LSP testing utilities and mock implementations

### Project Analysis Layer (`project/`)

- **Multi-Provider Scanning**: Unified project discovery across CMake, Meson, and extensible providers
- **Workspace Management**: Project workspace state with multi-component support and session persistence
- **Component Abstraction**: Unified representation of build system components across all providers
- **Compilation Database**: Global and per-component compilation database analysis and validation

### MCP Server Layer (`mcp_server/`)

- **Tool Implementation**: Advanced MCP tools for symbol search, analysis, and project management
- **Server Handler**: Core MCP protocol implementation with tool registration and routing
- **Result Processing**: Intelligent filtering, ranking preservation, and client-side result limiting

### Data Flow Architecture

1. **MCP Request** → Server handler parses and validates tool parameters with comprehensive error handling
2. **Project Discovery** → Multi-provider workspace scanning (CMake + Meson) with automatic component detection
3. **Session Management** → Clangd session initialization with configuration builder patterns and indexing monitoring
4. **LSP Communication** → Structured requests to clangd with fixed large limits (2000) for comprehensive results
5. **Response Processing** → Intelligent filtering, project boundary detection, and client-side user limit application
6. **MCP Response** → Structured JSON output with comprehensive metadata and workspace context

This implementation provides a complete bridge between MCP clients and C++ semantic analysis, enabling AI agents to work with C++ codebases using the same tools and understanding that human developers rely on.

## Advanced Session Management

### Clangd Session Architecture

The system implements sophisticated clangd session management through the `clangd/` module:

**Session Builder Pattern:**
- **Configuration Builder**: Flexible clangd configuration with version compatibility checks
- **Process Management**: Robust process lifecycle with automatic recovery and cleanup
- **Index State Tracking**: Real-time monitoring of clangd indexing progress with hash validation
- **Resource Management**: Automatic cleanup of processes, file handles, and temporary resources

**Advanced Features:**
- **Session Persistence**: Maintains session state across tool invocations for performance
- **Build Directory Switching**: Seamless switching between build configurations with state preservation
- **Version Detection**: Automatic clangd version detection with compatibility validation
- **Error Recovery**: Automatic session restart on failures with comprehensive error handling

### Workspace Session Management

The `project/workspace_session.rs` module provides persistent workspace state management:

**Key Capabilities:**
- **Multi-Provider State**: Maintains state across CMake and Meson components simultaneously
- **Component Tracking**: Tracks build configuration changes across all discovered components
- **Global Compilation Database**: Unified compilation database management across mixed build systems
- **Session Lifecycle**: Persistent workspace sessions with automatic state recovery and validation

**Workspace Intelligence:**
- **Provider Discovery**: Automatic detection and tracking of all build system types in workspace
- **Component Relationships**: Understanding of inter-component dependencies and build order
- **Configuration Validation**: Continuous validation of build configurations and compilation databases
- **State Synchronization**: Keeps workspace state synchronized with filesystem changes

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

## Workflow Practices

### Development Cycle

1. **Feature Design/Refactoring**: MANDATORY use of software-architect for all architectural decisions
2. **Implementation**: Follow project patterns and quality standards
3. **Implementation Review**: MANDATORY use of code-quality-engineer for every feature implementation
4. **Troubleshooting**: Use quality-engineer for validation issues beyond basic approach
5. **Testability Analysis**: Use both software-architect (high-level) and quality-engineer (implementation)
6. **Commit Preparation**: MANDATORY use of code-committer - ALWAYS display message for user acceptance

### Command Patterns

```bash
# Local development validation
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test

# E2E testing workflow
cargo build && cd test/e2e && npm test

# CLI tool validation
python3 tools/mcp-cli.py get-project-details
python3 tools/mcp-cli.py search-symbols "pattern"

# Debug preserved test failures
cd test/e2e && npm run inspect:verbose
```

### Commit Workflow Policy

**CRITICAL: Never commit changes unless explicitly requested by the user.**

The code-committer subagent assists with:

- Analyzing staged changes and drafting meaningful commit messages
- Ensuring proper authorship attribution and co-author credits
- Following project commit message standards (why first, what second)
- Checking git identity and adding appropriate sign-offs

**Workflow:**

1. User requests commit preparation: "Create a commit for these changes"
2. code-committer analyzes changes and suggests commit message
3. **MANDATORY: ALWAYS display the commit message to user for acceptance**
4. User explicitly approves: "Go ahead and commit" or "Create that commit"
5. Only then execute the actual git commit command

**Never assume permission to commit.** Always wait for explicit user instruction.

### Key Technical Patterns

#### Multi-Provider Architecture Pattern

- **Unified Component Model**: Abstract build system differences through `component.rs` interface
- **Provider Extensibility**: Add new build systems via `provider.rs` trait implementation
- **Global Compilation Database**: Override per-component databases for unified analysis
- **Workspace Session Persistence**: Maintain state across multi-provider project scanning

#### Clangd Session Management Pattern

- **Builder Configuration**: Use session builder for flexible clangd startup with version checks
- **Index State Tracking**: Monitor indexing progress with hash-based validation and completion detection
- **Automatic Recovery**: Restart sessions on failures with comprehensive error handling
- **Resource Lifecycle**: Explicit management of processes, file handles, and temporary resources

#### LSP-MCP Bridge Pattern

- **Large LSP Queries**: Always query clangd with fixed 2000 limit for comprehensive results
- **Client-side Filtering**: Apply user limits in post-processing to preserve ranking
- **Project Boundary Detection**: Smart filtering between project code and external dependencies
- **Context-Aware Analysis**: Integrate workspace intelligence with LSP responses for enhanced context

# important-instruction-reminders

Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.
ALWAYS prefer editing an existing file to creating a new one.
NEVER proactively create documentation files (\*.md) or README files. Only create documentation files if explicitly requested by the User.

# important-instruction-reminders

Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.

      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context or otherwise consider it in your response unless it is highly relevant to your task. Most of the time, it is not relevant.
