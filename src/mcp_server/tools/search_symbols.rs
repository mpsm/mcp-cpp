//! Symbol search functionality using session-based API
//!
//! This module provides two search modes for C++ symbols:
//!
//! ## Search Modes
//!
//! ### 1. Workspace Search (when `files` is None)
//! - Uses LSP `workspace/symbol` requests to clangd
//! - **Subject to clangd heuristics**: May not return all matching symbols
//! - Best for discovery and fuzzy matching across the project
//! - Results are ranked by clangd's relevance scoring
//!
//! ### 2. Document Search (when `files` is provided)
//! - Uses LSP `textDocument/documentSymbol` requests for each file
//! - **More predictable**: Returns all symbols in the file matching criteria
//! - Best for comprehensive analysis of specific files
//! - Uses substring matching for symbol names
//!
//! ## Important Notes
//!
//! - **Workspace search results may be incomplete** due to clangd's internal filtering
//! - Document search provides more complete results but requires known file paths
//! - Both modes support kind filtering and project boundary detection

use rmcp::{
    ErrorData,
    model::{CallToolResult, Content},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};

use crate::mcp_server::tools::lsp_helpers::document_symbols::SymbolSearchBuilder;
use crate::mcp_server::tools::lsp_helpers::workspace_symbols::WorkspaceSymbolSearchBuilder;
use crate::mcp_server::tools::utils;
use crate::project::index::IndexStatusView;
use crate::project::{ComponentSession, ProjectComponent, ProjectWorkspace};
use crate::symbol::Symbol;

/// Search result structure for search_symbols tool
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub success: bool,
    pub query: String,
    pub total_matches: usize,
    pub symbols: Vec<Symbol>,
    pub metadata: SearchMetadata,
    /// Index status information when timeout occurred or no indexing wait
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_status: Option<IndexStatusView>,
}

/// Metadata about the search operation
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMetadata {
    pub search_type: String,
    pub build_directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_processed: Option<Vec<FileProcessingResult>>,
}

/// Result of processing a specific file during search
#[derive(Debug, Serialize, Deserialize)]
pub struct FileProcessingResult {
    pub file: String,
    pub status: String,
    pub symbols_found: usize,
}

/// Advanced C++ symbol search engine with intelligent dual-mode operation for comprehensive
/// codebase exploration. Leverages clangd LSP for semantic understanding and provides
/// both broad workspace discovery and precise file-specific analysis capabilities.
///
/// 🚀 RECOMMENDED WORKFLOW FOR AI AGENTS:
/// 1. ALWAYS call get_project_details first to discover available build directories
/// 2. Use the ABSOLUTE build directory paths from get_project_details output
/// 3. Then call search_symbols with the build_directory parameter
///
/// Example workflow:
/// • get_project_details {} → Returns: {"/home/project/build-debug": {...}}
/// • search_symbols {"query": "Math", "build_directory": "/home/project/build-debug"}
///
/// ⚡ WHY USE THESE TOOLS:
/// • MUCH FASTER than filesystem reads (ls, find, grep commands)
/// • SEMANTIC AWARENESS: Understands C++ syntax, templates, namespaces
/// • PROJECT INTELLIGENCE: Filters out system/external symbols automatically
/// • LSP INTEGRATION: Uses same semantic understanding as IDEs
///
/// 🔍 DUAL SEARCH MODES:
/// • Workspace Search (default): Fuzzy matching across entire codebase using clangd workspace symbols
/// • Document Search (with files parameter): Comprehensive symbol enumeration within specific files
/// • Smart mode selection based on parameters for optimal results
///
/// 📋 SYMBOL OVERVIEW CAPABILITY:
/// • Use empty query ("") with files parameter to list ALL symbols in specified files
/// • Use empty query ("") without files for workspace-wide symbol discovery (subject to clangd heuristics)
/// • Perfect for getting complete symbol inventory of headers or source files
/// • Ideal for API exploration and codebase familiarization
/// • No search filtering - shows comprehensive symbol catalog
/// • ⚠️ Note: Workspace-wide empty queries may not return all symbols due to clangd's internal filtering
///
/// 🎯 INTELLIGENT FILTERING:
/// • Symbol kinds: Class, Function, Method, Variable, Enum, Namespace, Constructor, Field, Interface, Struct
/// • Project boundary detection (exclude external/system symbols by default)
/// • Fuzzy matching with clangd's relevance ranking preserved
/// • Configurable result limits with smart client-side application
///
/// ⚡ PERFORMANCE & RELIABILITY:
/// • Fixed 2000-symbol queries to clangd with client-side limiting for consistent ranking
/// • Indexing progress tracking with configurable timeout control
/// • Automatic build directory detection and validation
/// • Graceful handling of large codebases with intelligent result capping
///
/// 🏗️ BUILD SYSTEM INTEGRATION:
/// • Multi-provider support (CMake, Meson, extensible architecture)
/// • Automatic compilation database discovery and validation
/// • Custom build directory specification for multi-component projects
/// • Project vs external symbol classification using compilation database analysis
///
/// 🎮 USAGE PATTERNS:
/// • Discovery: search_symbols {"query": "vector", "max_results": 10}
/// • Type filtering: search_symbols {"query": "Process", "kinds": ["Class", "Struct"]}
/// • File overview: search_symbols {"query": "", "files": ["include/api.h"]}
/// • PROJECT EXPLORATION: search_symbols {"query": "", "max_results": 100, "build_directory": "/abs/path"}
///   → Returns top symbols to understand what the project does (classes, main functions, key APIs)
/// • Workspace overview: search_symbols {"query": "", "max_results": 500} (limited by clangd)
/// • External symbols: search_symbols {"query": "std::", "include_external": true}
///
/// INPUT PARAMETERS:
/// • query: C++ symbol name to search (NOT file paths!) - use "" when unsure to explore first
/// • files: Optional file paths for document-specific search
/// • kinds: Optional symbol type filtering (PascalCase names)
/// • max_results: Result limit (default: 100, max: 1000)
/// • include_external: Include system/library symbols (default: false)
/// • build_directory: Custom build directory path (STRONGLY PREFER ABSOLUTE PATHS from get_project_details)
/// • wait_timeout: Indexing completion timeout in seconds (default: 20s, 0 = no wait)
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchSymbolsTool {
    /// Search query to match C++ SYMBOL NAMES (class names, function names, variable names, etc.).
    /// This is NOT for file paths, component names, or directory names - only code symbol names.
    ///
    /// EXAMPLES:
    /// • "Math" (matches class/namespace named Math)
    /// • "factorial" (matches functions named factorial)
    /// • "std::vector" (matches the vector class from std namespace)
    /// • "" (EMPTY STRING for PROJECT EXPLORATION - see all important symbols)
    ///
    /// WHEN UNSURE: Use empty string ("") first to explore what symbols exist, then search for specific ones.
    ///
    /// USE CASES FOR EMPTY QUERY:
    /// • WITH files parameter: Complete symbol listing for specific files
    /// • WITHOUT files parameter: PROJECT EXPLORATION - discover key symbols to understand project purpose
    /// Note: Workspace-wide empty queries subject to clangd heuristics - may not return all symbols.
    pub query: String,

    /// Optional symbol kinds to filter results. Supported PascalCase names: "Class", "Function", "Method", "Variable", "Enum", "Namespace", "Constructor", "Field", "Interface", "Struct". Can specify multiple kinds for combined filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,

    /// Optional file paths to limit search scope
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,

    /// Maximum number of results (default: 100, max: 1000)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,

    /// Include external symbols from system libraries (default: false)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_external: Option<bool>,

    /// Build directory path containing compile_commands.json. STRONGLY RECOMMENDED: Use absolute paths from get_project_details output.
    ///
    /// WORKFLOW:
    /// 1. Call get_project_details to see available build directories with absolute paths
    /// 2. Copy the absolute path from that output (e.g., "/home/project/build-debug")
    /// 3. Use that absolute path here to avoid path concatenation issues
    ///
    /// EXAMPLES:
    /// • GOOD: "/home/project/build-debug", "/absolute/path/to/build"
    /// • AVOID: "build", "../build" (relative paths can cause concatenation issues)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_directory: Option<String>,

    /// Timeout in seconds to wait for indexing completion (default: 20s, 0 = no wait)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_timeout: Option<u64>,
}

impl SearchSymbolsTool {
    #[instrument(name = "search_symbols", skip(self, component_session, workspace))]
    pub async fn call_tool(
        &self,
        component_session: Arc<ComponentSession>,
        workspace: &ProjectWorkspace,
    ) -> Result<CallToolResult, ErrorData> {
        // Convert string kinds to SymbolKind enums once at the start
        let symbol_kinds: Option<Vec<lsp_types::SymbolKind>> =
            if let Some(ref kind_names) = self.kinds {
                let mut kinds = Vec::new();
                for kind_name in kind_names {
                    match lsp_types::SymbolKind::try_from(kind_name.as_str()) {
                        Ok(kind) => kinds.push(kind),
                        Err(_) => {
                            return Err(ErrorData::invalid_params(
                                format!("Invalid symbol kind: '{}'", kind_name),
                                None,
                            ));
                        }
                    }
                }
                Some(kinds)
            } else {
                None
            };

        info!(
            "Searching symbols (v2): query='{}', kinds={:?}, max_results={:?}, wait_timeout={:?}",
            self.query, symbol_kinds, self.max_results, self.wait_timeout
        );

        // Selective indexing wait logic based on search type
        let index_status = utils::handle_selective_indexing_wait(
            &component_session,
            self.files.is_some(), // Skip indexing for document search (files specified)
            self.wait_timeout,
            if self.files.is_some() {
                "Document search"
            } else {
                "Workspace search"
            },
        )
        .await;

        // Get the component for this session's build directory
        let build_dir = component_session.build_dir();
        let component = workspace
            .get_component_by_build_dir(build_dir)
            .ok_or_else(|| {
                ErrorData::invalid_params(
                    "Build directory not found in workspace".to_string(),
                    None,
                )
            })?;

        // Determine search scope and delegate to appropriate LSP method.
        // File-specific searches use textDocument/documentSymbol for precise results,
        // while workspace searches use workspace/symbol for broad discovery.
        let mut result = if let Some(ref files) = self.files {
            // File-specific search using document symbols for targeted analysis
            self.search_in_files(&component_session, files, component, symbol_kinds.as_ref())
                .await?
        } else {
            // Workspace-wide search using workspace symbols for comprehensive discovery
            self.search_workspace_symbols(&component_session, component, symbol_kinds.as_ref())
                .await?
        };

        // Include index status if available
        result.index_status = index_status;

        let output = serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(format!("Failed to serialize result: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Handle workspace-wide symbol search using LSP helpers
    async fn search_workspace_symbols(
        &self,
        component_session: &ComponentSession,
        component: &ProjectComponent,
        symbol_kinds: Option<&Vec<lsp_types::SymbolKind>>,
    ) -> Result<SearchResult, ErrorData> {
        // Build the search using the new helper's builder pattern
        let mut search_builder = WorkspaceSymbolSearchBuilder::new(self.query.clone())
            .include_external(self.include_external.unwrap_or(false));

        // Add kind filtering if specified
        if let Some(kinds) = symbol_kinds {
            search_builder = search_builder.with_kinds(kinds.clone());
        }

        // Add result limiting
        if let Some(max) = self.max_results {
            search_builder = search_builder.with_max_results(max);
        }

        // Execute the search
        let workspace_symbols = search_builder
            .search(component_session, component)
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to search symbols: {}", e), None)
            })?;

        // Convert WorkspaceSymbol to Symbol using the From trait
        let symbols: Vec<Symbol> = workspace_symbols.into_iter().map(Symbol::from).collect();

        Ok(SearchResult {
            success: true,
            query: self.query.clone(),
            total_matches: symbols.len(),
            symbols,
            metadata: SearchMetadata {
                search_type: "workspace".to_string(),
                build_directory: component.build_dir_path.display().to_string(),
                files_processed: None,
            },
            index_status: None, // Will be set by caller
        })
    }

    /// Handle file-specific document symbol search
    async fn search_in_files(
        &self,
        component_session: &ComponentSession,
        files: &[String],
        component: &ProjectComponent,
        symbol_kinds: Option<&Vec<lsp_types::SymbolKind>>,
    ) -> Result<SearchResult, ErrorData> {
        info!(
            "Document search: query='{}', files={:?}, kinds={:?}",
            self.query, files, symbol_kinds
        );

        // Resolve relative file paths to absolute paths using project root
        let project_root = &component.source_root_path;
        let mut absolute_files = Vec::new();
        for file_path in files {
            let absolute_path = if std::path::Path::new(file_path).is_absolute() {
                file_path.clone()
            } else {
                let resolved_path = project_root.join(file_path);
                // Check if file exists and return error if not
                if !resolved_path.exists() {
                    return Err(ErrorData::invalid_params(
                        format!(
                            "File not found: {} (resolved to {})",
                            file_path,
                            resolved_path.display()
                        ),
                        None,
                    ));
                }
                resolved_path.to_string_lossy().to_string()
            };
            absolute_files.push(absolute_path);
        }

        info!("Resolved files: {:?}", absolute_files);

        // Build the search using the document symbols helper's builder pattern
        let mut search_builder = SymbolSearchBuilder::new();

        // Only add name filter if query is not empty - this allows listing all symbols in files
        if !self.query.is_empty() {
            search_builder = search_builder.with_name(&self.query);
        }

        // Add kind filtering if specified
        if let Some(kinds) = symbol_kinds {
            search_builder = search_builder.with_kinds(kinds);
        }

        info!("Created search builder: {:?}", search_builder);

        // Execute the search with top-level limiting using absolute file paths
        let file_results = search_builder
            .search_multiple_files(component_session, &absolute_files, self.max_results)
            .await
            .map_err(|e| {
                ErrorData::internal_error(format!("Failed to search files: {}", e), None)
            })?;

        info!(
            "File search results: {} files processed",
            file_results.len()
        );

        // Convert DocumentSymbol results to Symbol structs
        let mut all_symbols = Vec::new();
        let mut processed_files = Vec::new();

        for (file_path, symbols) in file_results {
            processed_files.push(FileProcessingResult {
                file: file_path.clone(),
                status: "success".to_string(),
                symbols_found: symbols.len(),
            });

            // Convert DocumentSymbol to Symbol using the From trait
            for symbol in symbols {
                let path = std::path::PathBuf::from(&file_path);
                let converted_symbol = Symbol::from((&symbol, path.as_path()));
                all_symbols.push(converted_symbol);
            }
        }

        Ok(SearchResult {
            success: true,
            query: self.query.clone(),
            total_matches: all_symbols.len(),
            symbols: all_symbols,
            metadata: SearchMetadata {
                search_type: "file_specific".to_string(),
                build_directory: component.build_dir_path.display().to_string(),
                files_processed: Some(processed_files),
            },
            index_status: None, // Will be set by caller
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_search_symbols_deserialize() {
        let json_data = json!({
            "query": "vector",
            "kinds": ["Class", "Function"],
            "max_results": 50
        });
        let tool: SearchSymbolsTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.query, "vector");
        assert_eq!(
            tool.kinds,
            Some(vec!["Class".to_string(), "Function".to_string()])
        );
        assert_eq!(tool.max_results, Some(50));
        assert_eq!(tool.wait_timeout, None);
    }

    #[test]
    fn test_search_symbols_minimal() {
        let json_data = json!({
            "query": "main"
        });
        let tool: SearchSymbolsTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.query, "main");
        assert_eq!(tool.kinds, None);
        assert_eq!(tool.max_results, None);
        assert_eq!(tool.wait_timeout, None);
    }
}
