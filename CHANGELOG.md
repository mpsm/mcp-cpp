# Changelog

## [0.2.1] - 2025-01-22

Improved AI agent guidance and error handling for C++ codebase navigation.

### AI Agent Workflow Guidance

Tools now guide agents to call `get_project_details` first for absolute build
directory paths, then use those paths with `search_symbols` and `analyze_symbol_context`.
Eliminates relative vs absolute path confusion and concatenation errors.

Search tools distinguish C++ symbol names from file paths in query parameters.
Empty query exploration recommended for unfamiliar codebases.

### Enhanced Error Messages

Path resolution errors now include scan root directory, available build directories,
and path resolution details. Errors guide agents toward correct usage instead of
providing cryptic failures.

### Tool Performance Documentation

Documentation emphasizes semantic-aware tools outperform filesystem operations
(`find`, `grep`, `cat`) with C++ syntax understanding, template awareness, and
automatic project boundary detection.

## [0.2.0] - 2025-01-21

Complete rewrite of the initial MVP release. Redesigned to provide a stable
foundation for further development with improved stability and functionality.

### Indexing Monitoring

Improved index handling with configurable timeouts (default: 20 seconds). The agent
is informed about indexing progress when operations exceed the timeout, eliminating
guesswork about incomplete results. Indexing is tracked at the file level, with the
application reading both clangd's index directly and tracking clangd logs to ensure
maximum accuracy when clangd doesn't cover all compilation database entries during
initial indexing.

Since clangd is conservative with indexing, the new logic ensures complete coverage
by tracking individual files and opening any that remain unindexed after clangd
reports completion. This delivers more accurate results when querying workspace
symbols.

### Multi-Component Project Support

Added support for handling multiple clangd sessions simultaneously. The MCP server
can now work with multiple C/C++ projects concurrently, which is useful for complex
projects like embedded Linux where understanding dynamics between individual
components matters. Both Meson and CMake build systems are supported.

### Symbol Search and Analysis Improvements

Various refinements for more concise and dense output. The search tool now supports
file location hints to disambiguate symbols with identical names across different
contexts. The analyze tool features improved call tree and inheritance hierarchy
resolution, showing symbol relationships and usage patterns more clearly.

### Testing Framework

The complete rewrite enabled a better testing approach built on Rust integration
tests rather than end-to-end testing. This targeted testing strategy provides more
precise debugging capabilities and faster development cycles.

## [0.1.0] - 2025-07-20

Initial release.
