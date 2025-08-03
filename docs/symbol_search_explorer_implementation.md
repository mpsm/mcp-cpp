# Symbol Search & Explorer Implementation Plan

## Overview

The Symbol Search & Explorer is a powerful symbol search tool that provides advanced filtering, pattern matching, and bulk symbol information retrieval across the entire codebase. It builds upon the existing `get_symbols` tool to provide sophisticated search capabilities that AI agents need to effectively explore and understand large C++ codebases.

**Built on `get_symbols` Foundation:**
This tool extends the proven `get_symbols` infrastructure, leveraging the same clangd LSP integration, build directory management, and workspace/document symbol capabilities. The key difference is that `get_symbols` returns all symbols (with optional file filtering), while `search_symbols` adds query-based filtering and enhanced result organization.

**Workspace Scope and Limitations:**

- **Workspace Boundary**: Symbol search is limited to the current CMake project workspace
- **Build Directory Requirement**: Requires a valid CMake build directory with `compile_commands.json`
- **Root Criteria**: Workspace root is determined by the CMake project structure and the configured build directory
- **File Scope**: Only C++ files that are part of the CMake compilation database are indexed and searchable
- **Cross-Project**: Does not search across multiple unrelated projects or system headers outside the project

## Workspace Scope and Search Boundaries

### CMake Project Workspace Definition

The symbol search operates within the boundaries of a CMake project workspace, determined by:

1. **Build Directory Root**: The CMake build directory containing `compile_commands.json`
2. **Source Tree**: All source files referenced in the compilation database
3. **Project Headers**: Headers included by project files (both local and system)
4. **Compilation Units**: Only files that are actually compiled as part of the CMake build

### Search Scope Criteria

```rust
// Workspace boundary determination (inherited from get_symbols):
fn workspace_scope() -> WorkspaceScope {
    WorkspaceScope {
        root_directory: build_directory.parent(),  // CMake source directory
        compilation_database: "compile_commands.json",
        indexed_files: files_from_compile_commands + included_headers,
        exclusions: [
            system_headers_outside_project,
            build_artifacts,
            cmake_generated_files
        ]
    }
}
```

### Symbol Visibility Rules

**Default Scope** (`include_external: false` or not specified):

- ✅ **Compilation Database Files**: Only files referenced in `compile_commands.json`
- ✅ **Project Source Files**: All `.cpp`, `.cc`, `.cxx` files in compilation database
- ✅ **Project Headers**: Headers included by and part of the project compilation
- ✅ **Template Instantiations**: Template specializations within project code
- ❌ **System Headers**: Standard library headers not in compilation database
- ❌ **External Libraries**: Third-party library files not in compilation database
- ❌ **Build Artifacts**: Generated files, object files, CMake cache

**Extended Scope** (`include_external: true`):

- ✅ **All Indexed Symbols**: Everything clangd has indexed, regardless of compilation database
- ✅ **System Headers**: Standard library symbols (`std::vector`, `std::string`, etc.)
- ✅ **External Libraries**: Third-party library symbols visible to clangd
- ✅ **Compiler Builtins**: Compiler-provided symbols and intrinsics

### Multi-Project Limitations

- **Single Project**: Only searches the current CMake project workspace
- **No Cross-Project**: Cannot search across multiple unrelated CMake projects
- **No Global Search**: Cannot search system-wide or across different build roots
- **Build Directory Bound**: Each search session is tied to one specific build directory

## Feature Breakdown

### Input Schema

```json
{
  "tool": "search_symbols",
  "arguments": {
    "query": "string",
    "kinds": ["class|function|variable|namespace|typedef|enum|..."]?,
    "files": ["string"]?,
    "max_results": "number?",
    "include_external": "boolean?"
  }
}
```

### Output Schema

```json
{
  "query": "string",
  "total_matches": "number",
  "symbols": [
    {
      "name": "string",
      "qualified_name": "string",
      "kind": "string",
      "location": {
        "file": "string",
        "range": "Range",
        "line_preview": "string"
      },
      "signature": "string?",
      "documentation": "string?",
      "references_count": "number?"
    }
  ]
}
```

## Incremental Development Plan

### Increment 1: Direct clangd Query Interface

**User Value**: Expose clangd's powerful native search capabilities directly to users
**Deliverable**: Simple, powerful search tool that leverages clangd's full query syntax

**What users get**:

- Direct access to clangd's native query syntax:
  - `MyClass` - fuzzy search for class names
  - `std::vector` - qualified name search
  - `::global_func` - global scope search
  - `MyNamespace::` - all symbols in namespace
  - `get` - fuzzy search for "get" (matches getters, etc.)
- Fast, server-optimized search with clangd's built-in relevance ranking
- No artificial limitations on clangd's search capabilities

**Implementation**:

1. Create `SearchSymbolsTool` that passes query directly to clangd
2. Leverage existing `workspace/symbol` LSP infrastructure from `get_symbols`
3. Reuse build directory resolution and clangd setup logic
4. Add basic symbol kind filtering and external symbol control as client-side filters

**Files to create/modify**:

- `src/tools.rs` - Add `SearchSymbolsTool` extending `GetSymbolsTool` patterns
- `src/handler.rs` - Register the new tool
- Reuse existing LSP infrastructure, build directory resolution, and clangd management from `GetSymbolsTool`

**Query Examples Users Can Use**:

```rust
"vector"           // Fuzzy: finds vector, Vector_impl, etc.
"std::vector"      // Qualified: finds std::vector specifically
"::main"          // Global: finds global main function
"MyClass::"       // Scope: all members of MyClass
"create"          // Fuzzy: createFile, createWindow, etc.
```

### Increment 2: Enhanced Result Organization and Filtering

**User Value**: Well-organized search results with comprehensive symbol information
**Deliverable**: Complete symbol search tool with filtering and all available symbol details

**What users get**:

- Symbol kind filtering (functions vs classes vs variables)
- File/directory filtering for focusing on specific code areas
- Complete symbol information (signatures, documentation, references) when available
- Clean result organization and formatting

**Implementation**:

1. Add client-side filtering for symbol kinds and file patterns
2. Add LSP hover requests for signatures and documentation
3. Add LSP references requests for usage counting
4. Create clean result formatting

**Files to modify**:

- `src/tools.rs` - Add filtering logic and LSP detail requests
- `src/lsp/client.rs` - Add hover and references support
- `src/lsp/types.rs` - Add complete symbol structures

## Development Strategy

This simplified approach focuses on exposing clangd's native power rather than reimplementing it:

### Value Progression:

1. **Direct Query Access** → Users get immediate access to clangd's full search capabilities
2. **Organized Results** → Users get clean, filtered, and grouped search results

### Key Benefits of Direct Query Approach:

- **Simplicity**: Much less code to write and maintain
- **Power**: Users get access to clangd's full query syntax immediately
- **Performance**: No artificial bottlenecks or query translation overhead
- **Reliability**: Leverages clangd's proven search algorithms
- **Extensibility**: Easy to add new features on top of solid foundation

### Integration with Existing Code:

- Leverages existing `ClangdManager` and LSP infrastructure from `get_symbols`
- Builds on proven `get_symbols` workflow and error handling patterns
- Reuses existing build directory detection and clangd setup from `GetSymbolsTool`
- Extends current workspace/symbol LSP integration with query filtering

### User Experience Examples:

```json
// Simple fuzzy search
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "vector"
    }
  }
}

// Qualified name search
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "std::vector"
    }
  }
}

// Namespace exploration
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "MyNamespace::"
    }
  }
}

// Global scope search
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "::main"
    }
  }
}

// Only functions (minimal filtering)
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "create",
      "kinds": ["function"]
    }
  }
}

// Focus on specific files
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "parse",
      "files": ["src/parser/parser.cpp"]
    }
  }
}

// Include external symbols (system headers, libraries)
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "std::vector",
      "include_external": true
    }
  }
}

// Project-only search (default behavior)
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "MyClass"
    }
  }
}
```

## Technical Considerations

### Direct clangd Integration

- **Native Query Passthrough**: Pass user queries directly to clangd's workspace/symbol
- **Zero Translation Overhead**: No query parsing or reconstruction needed
- **Full Feature Access**: Users get clangd's complete query syntax and capabilities
- **Natural Performance**: Clangd's optimized search algorithms work at full speed

### Implementation Strategy for File Filtering

**Simple, clean approach:**

```rust
// Clean implementation approach:
async fn search_symbols(request: SearchRequest) -> Result<Vec<Symbol>> {
    let symbols = match request.files {
        // No files specified: use workspace search
        None => {
            clangd.workspace_symbol(&request.query).await
        }

        // Any files specified: use per-file search
        Some(files) => {
            let mut all_symbols = Vec::new();
            for file_path in files {
                if let Ok(symbols) = clangd.document_symbol(file_path).await {
                    // Apply query filtering to document symbols
                    let filtered = symbols.into_iter()
                        .filter(|sym| matches_query(&sym.name, &request.query))
                        .collect();
                    all_symbols.extend(filtered);
                }
            }
            all_symbols
        }
    }?;

    // Apply external symbol filtering (default: exclude external)
    let filtered_symbols = if request.include_external.unwrap_or(false) {
        symbols  // Include all symbols
    } else {
        symbols.into_iter()
            .filter(|sym| is_project_symbol(sym, &compilation_database))
            .collect()
    };

    Ok(filtered_symbols)
}

fn is_project_symbol(symbol: &Symbol, compilation_database: &CompilationDatabase) -> bool {
    // Only include symbols from files that are part of the compilation database
    // This correctly excludes system headers, external libraries, and build artifacts
    // while including all project files (source + project headers)
    compilation_database.contains_file(&symbol.location.file)
}
```

**Decision matrix:**

- **No files specified**: Use `workspace/symbol` (broad exploration)
- **Any files specified**: Use `textDocument/documentSymbol` per file (focused search)

**Why this clean approach:**

- **Predictable**: Users know exactly what they get
- **No edge cases**: Simple binary decision, no special cases
- **Agent-friendly**: Perfect for AI agents working in large codebases
- **Maintainable**: Easy to understand, debug, and extend

**Trade-offs accepted:**

- ✅ **Simple and predictable**: No complex logic or edge cases
- ✅ **Agent-optimized**: Perfect for AI agents in large codebases
- ✅ **Fast for file-specific searches**: Minimal data transfer when files specified
- ✅ **Easy to debug**: Clear execution path, no heuristics
- ❌ **No cross-file ranking when files specified**: Acceptable trade-off for simplicity

**Usage patterns:**

```json
// Project-only search (default): excludes system headers
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "vector"
    }
  }
}

// Include external symbols: finds std::vector, boost::vector, etc.
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "vector",
      "include_external": true
    }
  }
}

// Focused file search: use textDocument/documentSymbol per file
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "parse",
      "files": ["src/parser.cpp"]
    }
  }
}

// Multiple files: still use textDocument per file
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "create",
      "files": ["src/core/a.cpp", "src/core/b.cpp", "src/gui/c.cpp"]
    }
  }
}

// Directory patterns: user provides specific file list
{
  "method": "tools/call",
  "params": {
    "name": "search_symbols",
    "arguments": {
      "query": "init",
      "files": ["src/core/init.cpp", "src/gui/init.cpp"]
    }
  }
}
```

### Minimal Client-Side Processing

- **Symbol Kind Filtering**: Only filter by symbol type when specifically requested
- **File Path Filtering**: Basic file/directory filtering when users want to focus search scope
- **External Symbol Control**: Filter out system headers and external libraries by default (`include_external: false`)
- **Result Limiting**: Respect max_results to avoid overwhelming responses
- **Complete Symbol Information**: Always provide signature, documentation, and references when available

### Performance Optimizations

- **Caching**: Cache workspace symbols and search results
- **Batching**: Batch LSP requests for multiple symbols
- **Concurrency**: Parallel processing of symbol details
- **Incremental**: Update caches incrementally as code changes

### Search Quality

- **Relevance Scoring**: Rank results by relevance to query
- **Result Limiting**: Efficient handling of large result sets
- **Query Optimization**: Optimize complex filter combinations
- **User Experience**: Fast response times with progress indicators

### C++ Language Support

- **Template Specializations**: Handle template instantiations properly
- **Overloaded Functions**: Group and distinguish overloads
- **Namespace Resolution**: Proper qualified name handling
- **Modern C++ Features**: Support for concepts, modules, etc.

## Dependencies

### Existing Dependencies

- Leverage current Rust MCP SDK integration
- Build on existing clangd LSP infrastructure
- Use current JSON schema and serde framework

### Potential New Dependencies

- **regex**: For advanced pattern matching
- **fuzzy-matcher**: For fuzzy search capabilities
- **rayon**: For parallel result processing
- **dashmap**: For concurrent caching

## Success Criteria

1. **Functionality**: Comprehensive symbol search with rich filtering
2. **Performance**: Sub-second response for typical queries
3. **Scalability**: Handles large codebases (100k+ symbols)
4. **Usability**: Intuitive query syntax and result organization
5. **Integration**: Seamless extension of existing tools

## Future Enhancements

1. **AI-Powered Search**: Semantic search using embeddings
2. **Visual Search Results**: Generate code maps and diagrams
3. **Search History**: Track and suggest frequent search patterns
4. **Cross-Project Search**: Search across multiple related projects
5. **Real-time Search**: Live search results as user types
