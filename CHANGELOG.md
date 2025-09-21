# Changelog

## [0.2.0] - pending

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
