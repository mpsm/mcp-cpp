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

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};

use crate::clangd::session::ClangdSession;
use crate::mcp_server::tools::lsp_helpers::document_symbols::SymbolSearchBuilder;
use crate::mcp_server::tools::lsp_helpers::workspace_symbols::WorkspaceSymbolSearchBuilder;
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

#[mcp_tool(
    name = "search_symbols",
    description = "Advanced C++ symbol search using clangd LSP with project-aware filtering"
)]
#[derive(Debug, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct SearchSymbolsTool {
    /// Search query using clangd's native syntax
    pub query: String,

    /// Optional symbol kinds to filter results. Use PascalCase names like "Class", "Function", "Method", "Variable", "Enum", etc.
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

    /// Build directory path containing compile_commands.json
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_directory: Option<String>,
}

impl SearchSymbolsTool {
    #[instrument(name = "search_symbols", skip(self, component_session, workspace))]
    pub async fn call_tool(
        &self,
        component_session: Arc<ComponentSession>,
        workspace: &ProjectWorkspace,
    ) -> Result<CallToolResult, CallToolError> {
        // Convert string kinds to SymbolKind enums once at the start
        let symbol_kinds: Option<Vec<lsp_types::SymbolKind>> =
            if let Some(ref kind_names) = self.kinds {
                let mut kinds = Vec::new();
                for kind_name in kind_names {
                    match lsp_types::SymbolKind::try_from(kind_name.as_str()) {
                        Ok(kind) => kinds.push(kind),
                        Err(_) => {
                            return Err(CallToolError::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                format!("Invalid symbol kind: '{}'", kind_name),
                            )));
                        }
                    }
                }
                Some(kinds)
            } else {
                None
            };

        info!(
            "Searching symbols (v2): query='{}', kinds={:?}, max_results={:?}",
            self.query, symbol_kinds, self.max_results
        );

        // Ensure indexing completion before acquiring session lock
        component_session
            .ensure_indexed(std::time::Duration::from_secs(30))
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!("Indexing failed: {}", e)))
            })?;

        // Lock session for LSP operations after indexing is complete
        let session_arc = component_session.clangd_session();
        let mut session_guard = session_arc.lock().await;

        // Get the component for this session's build directory
        let build_dir = component_session.build_dir();
        let component = workspace
            .get_component_by_build_dir(build_dir)
            .ok_or_else(|| {
                CallToolError::new(std::io::Error::other(
                    "Build directory not found in workspace",
                ))
            })?;

        // Determine search scope and delegate to appropriate LSP method.
        // File-specific searches use textDocument/documentSymbol for precise results,
        // while workspace searches use workspace/symbol for broad discovery.
        let result = if let Some(ref files) = self.files {
            // File-specific search using document symbols for targeted analysis
            self.search_in_files(&mut session_guard, files, component, symbol_kinds.as_ref())
                .await?
        } else {
            // Workspace-wide search using workspace symbols for comprehensive discovery
            self.search_workspace_symbols(&mut session_guard, component, symbol_kinds.as_ref())
                .await?
        };

        let output = serde_json::to_string_pretty(&result).map_err(|e| {
            CallToolError::new(std::io::Error::other(format!(
                "Failed to serialize result: {}",
                e
            )))
        })?;

        Ok(CallToolResult::text_content(vec![TextContent::from(
            output,
        )]))
    }

    /// Handle workspace-wide symbol search using LSP helpers
    async fn search_workspace_symbols(
        &self,
        session: &mut ClangdSession,
        component: &ProjectComponent,
        symbol_kinds: Option<&Vec<lsp_types::SymbolKind>>,
    ) -> Result<SearchResult, CallToolError> {
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
            .search(session, component)
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!(
                    "Failed to search symbols: {}",
                    e
                )))
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
        })
    }

    /// Handle file-specific document symbol search
    async fn search_in_files(
        &self,
        session: &mut ClangdSession,
        files: &[String],
        component: &ProjectComponent,
        symbol_kinds: Option<&Vec<lsp_types::SymbolKind>>,
    ) -> Result<SearchResult, CallToolError> {
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
                    return Err(CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "File not found: {} (resolved to {})",
                            file_path,
                            resolved_path.display()
                        ),
                    )));
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
            .search_multiple_files(session, &absolute_files, self.max_results)
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!(
                    "Failed to search files: {}",
                    e
                )))
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
    }
}
