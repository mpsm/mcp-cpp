//! Symbol search functionality

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument};

use crate::cmake::CmakeProjectStatus;
use crate::lsp::ClangdManager;
use super::serialize_result;
use super::symbol_filtering::{SymbolFilter, SymbolUtilities};

#[mcp_tool(
    name = "search_symbols",
    description = "Search C++ symbols using clangd LSP server with comprehensive filtering and scope control. \
                   QUERY SYNTAX: Supports fuzzy search ('vector'), qualified names ('std::vector'), namespace exploration ('MyNamespace::'), \
                   global scope ('::main'), and partial matches ('get_'). \
                   SYMBOL KINDS: class, struct, enum, function, method, variable, field, namespace, typedef, macro, constructor, destructor, \
                   operator, interface, property, event, constant, array, boolean, key, null, number, object, string, enumMember, typeParameter. \
                   SCOPE CONTROL: By default searches project symbols only (source files + headers in project directory tree). \
                   Use include_external=true for system headers and libraries. \
                   FILE FILTERING: Accepts relative paths from project root or absolute paths. \
                   PROJECT DETECTION: Automatically detects project boundaries using CMake compilation database and common ancestor analysis. \
                   OUTPUT: Returns JSON with symbol name, kind (human-readable), qualified name, file location, line/column, container scope, and detail information. \
                   Perfect for code exploration, navigation, refactoring, and understanding large codebases."
)]
#[derive(Debug, ::serde::Serialize, JsonSchema)]
pub struct SearchSymbolsTool {
    /// Search query using clangd's native syntax. 
    /// EXAMPLES: 'vector' (fuzzy match), 'std::vector' (qualified name), 'MyNamespace::' (explore namespace contents), 
    /// '::main' (global scope), 'get_' (prefix match), 'Math' (class name)
    pub query: String,
    
    /// Optional symbol kinds to filter results. 
    /// SUPPORTED KINDS: "class", "struct", "enum", "function", "method", "variable", "field", "namespace", "typedef", 
    /// "macro", "constructor", "destructor", "operator", "interface", "property", "event", "constant", "array", 
    /// "boolean", "key", "null", "number", "object", "string", "enumMember", "typeParameter". 
    /// Multiple kinds can be specified: ["class", "struct", "enum"]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,
    
    /// Optional file paths to limit search scope to specific files. 
    /// Accepts relative paths from project root (e.g., "src/math.cpp") or absolute paths. 
    /// When specified, uses document symbol search instead of workspace search for more detailed results within those files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    
    /// Maximum number of results to return. DEFAULT: 100. 
    /// Useful for limiting output when searching broad terms or exploring large codebases. 
    /// Results are returned in relevance order from clangd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,
    
    /// Include external symbols from system headers and libraries (e.g., std::vector, stdio functions). 
    /// DEFAULT: false (project-only scope). When true, includes symbols from outside the detected project boundaries. 
    /// Project boundaries are determined by CMake compilation database and directory tree analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_external: Option<bool>,
}

impl<'de> serde::Deserialize<'de> for SearchSymbolsTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct SearchSymbolsToolHelper {
            query: String,
            #[serde(default)]
            kinds: Option<Vec<String>>,
            #[serde(default)]
            files: Option<Vec<String>>,
            #[serde(default)]
            max_results: Option<u32>,
            #[serde(default)]
            include_external: Option<bool>,
        }

        let helper = SearchSymbolsToolHelper::deserialize(deserializer)?;
        
        Ok(SearchSymbolsTool {
            query: helper.query,
            kinds: helper.kinds,
            files: helper.files,
            max_results: helper.max_results,
            include_external: helper.include_external,
        })
    }
}

impl SearchSymbolsTool {
    #[instrument(name = "search_symbols", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Searching symbols: query='{}', kinds={:?}, files={:?}, max_results={:?}, include_external={:?}", 
              self.query, self.kinds, self.files, self.max_results, self.include_external);

        // Handle automatic clangd setup if needed
        let build_path = match Self::resolve_build_directory() {
            Ok(Some(path)) => path,
            Ok(None) => {
                let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "build_directory_required",
                    "message": "No build directory found. Use list_build_dirs tool to see available options, or configure a build directory first.",
                    "query": self.query,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
            Err(_) => {
                let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "build_directory_analysis_failed",
                    "message": "Failed to analyze build directories. Use list_build_dirs tool to see available options.",
                    "query": self.query,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        };

        // Ensure clangd is setup
        {
            let current_build_dir = clangd_manager
                .lock()
                .await
                .get_current_build_directory()
                .await;
            let needs_setup = match current_build_dir {
                Some(current) => current != build_path,
                None => true,
            };

            if needs_setup {
                info!(
                    "Setting up clangd for build directory: {}",
                    build_path.display()
                );
                let manager_guard = clangd_manager.lock().await;
                if let Err(e) = manager_guard.setup_clangd(build_path.clone()).await {
                    let indexing_state = manager_guard.get_indexing_state().await;
                    let content = json!({
                        "success": false,
                        "error": format!("Failed to setup clangd: {}", e),
                        "build_directory_attempted": build_path.display().to_string(),
                        "query": self.query,
                        "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                    });

                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]));
                }
            }
        }

        let manager_guard = clangd_manager.lock().await;
        let build_directory = manager_guard
            .get_current_build_directory()
            .await
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".to_string());

        // Determine search strategy based on files parameter
        if let Some(files) = &self.files {
            // File-specific search using document symbols
            self.search_in_files(files, &manager_guard, &build_directory)
                .await
        } else {
            // Workspace-wide search using workspace symbols
            self.search_workspace(&manager_guard, &build_directory)
                .await
        }
    }

    async fn search_in_files(
        &self,
        files: &[String],
        manager: &ClangdManager,
        build_directory: &str,
    ) -> Result<CallToolResult, CallToolError> {
        let mut all_symbols = Vec::new();
        let mut processed_files = Vec::new();

        for file_path in files {
            // Convert to URI format if needed
            let file_uri = if file_path.starts_with("file://") {
                file_path.clone()
            } else {
                format!("file://{}", file_path)
            };

            // Convert URI to file path for opening
            let path = if let Some(stripped) = file_uri.strip_prefix("file://") {
                PathBuf::from(stripped)
            } else {
                PathBuf::from(file_path)
            };

            // Open file if needed
            match manager.open_file_if_needed(&path).await {
                Ok(file_opened) => {
                    processed_files.push(json!({
                        "file": file_path,
                        "file_opened": file_opened,
                        "status": "success"
                    }));

                    // Get document symbols
                    let params = json!({
                        "textDocument": {
                            "uri": file_uri
                        }
                    });

                    match manager
                        .send_lsp_request("textDocument/documentSymbol".to_string(), Some(params))
                        .await
                    {
                        Ok(symbols) => {
                            if let Some(symbol_array) = symbols.as_array() {
                                // Apply file-specific filtering
                                let matching_symbols: Vec<_> = symbol_array
                                    .iter()
                                    .filter(|symbol| {
                                        SymbolUtilities::matches_query_and_filters(symbol, &self.query, &self.kinds)
                                    })
                                    .cloned()
                                    .collect();
                                
                                all_symbols.extend(matching_symbols);
                            }
                        }
                        Err(e) => {
                            processed_files.push(json!({
                                "file": file_path,
                                "file_opened": file_opened,
                                "status": "error",
                                "error": format!("Failed to get document symbols: {}", e)
                            }));
                        }
                    }
                }
                Err(e) => {
                    processed_files.push(json!({
                        "file": file_path,
                        "file_opened": false,
                        "status": "error",
                        "error": format!("Failed to open file: {}", e)
                    }));
                }
            }
        }

        // Apply external filtering and limit results
        let include_external = self.include_external.unwrap_or(false);
        let filtered_symbols = SymbolFilter::filter_symbols(all_symbols, include_external, &self.kinds, manager).await;
        let limited_symbols = SymbolUtilities::limit_results(filtered_symbols, self.max_results);
        let converted_symbols = SymbolUtilities::convert_symbol_kinds(limited_symbols);

        let indexing_state = manager.get_indexing_state().await;
        let opened_files_count = manager.get_opened_files_count().await;

        let content = json!({
            "success": true,
            "query": self.query,
            "total_matches": converted_symbols.len(),
            "symbols": converted_symbols,
            "metadata": {
                "search_type": "file_specific",
                "files_processed": processed_files,
                "opened_files_count": opened_files_count,
                "build_directory_used": build_directory,
                "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
            }
        });

        Ok(CallToolResult::text_content(vec![TextContent::from(
            serialize_result(&content),
        )]))
    }

    async fn search_workspace(
        &self,
        manager: &ClangdManager,
        build_directory: &str,
    ) -> Result<CallToolResult, CallToolError> {
        // Check current indexing state before waiting
        let initial_indexing_state = manager.get_indexing_state().await;
        info!("Initial indexing state: {:?}", initial_indexing_state.status);
        
        // Wait for indexing to complete if not already completed
        if initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed {
            info!("Waiting for indexing completion before workspace symbol search (current status: {:?})", initial_indexing_state.status);
            if let Err(e) = manager
                .wait_for_indexing_completion(std::time::Duration::from_secs(30))
                .await
            {
                let final_indexing_state = manager.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("Indexing timeout: {}", e),
                    "message": "Symbol search may be incomplete due to ongoing indexing",
                    "query": self.query,
                    "initial_status": match initial_indexing_state.status {
                        crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                        crate::lsp::types::IndexingStatus::InProgress => "in_progress", 
                        crate::lsp::types::IndexingStatus::Completed => "completed",
                    },
                    "indexing_status": SymbolUtilities::format_indexing_status(&final_indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        }

        // Send workspace symbol request with user's query
        let params = json!({
            "query": self.query
        });

        match manager
            .send_lsp_request("workspace/symbol".to_string(), Some(params))
            .await
        {
            Ok(symbols) => {
                let symbol_array = symbols.as_array().unwrap_or(&vec![]).clone();
                
                // Apply external filtering and other filters
                let include_external = self.include_external.unwrap_or(false);
                let filtered_symbols = SymbolFilter::filter_symbols(symbol_array, include_external, &self.kinds, manager).await;
                let limited_symbols = SymbolUtilities::limit_results(filtered_symbols, self.max_results);
                let converted_symbols = SymbolUtilities::convert_symbol_kinds(limited_symbols);

                let final_indexing_state = manager.get_indexing_state().await;
                let opened_files_count = manager.get_opened_files_count().await;

                let content = json!({
                    "success": true,
                    "query": self.query,
                    "total_matches": converted_symbols.len(),
                    "symbols": converted_symbols,
                    "metadata": {
                        "search_type": "workspace",
                        "opened_files_count": opened_files_count,
                        "build_directory_used": build_directory,
                        "indexing_waited": initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed,
                        "indexing_status": SymbolUtilities::format_indexing_status(&final_indexing_state)
                    }
                });

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(e) => {
                let indexing_state = manager.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("LSP request failed: {}", e),
                    "query": self.query,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
        }
    }

    fn resolve_build_directory() -> Result<Option<PathBuf>, String> {
        match CmakeProjectStatus::analyze_current_directory() {
            Ok(status) => match status.build_directories.len() {
                1 => {
                    let build_dir = &status.build_directories[0];
                    info!(
                        "Auto-resolved to single build directory: {}",
                        build_dir.path.display()
                    );
                    Ok(Some(build_dir.path.clone()))
                }
                0 => {
                    info!("No build directories found");
                    Ok(None)
                }
                _ => {
                    info!("Multiple build directories found, requiring explicit selection");
                    Ok(None)
                }
            },
            Err(e) => {
                info!("Not a CMake project or failed to analyze: {}", e);
                Err(format!("Failed to analyze build directories: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_search_symbols_tool_deserialize() {
        let json_data = json!({
            "query": "vector",
            "kinds": ["class", "function"],
            "max_results": 50
        });
        let tool: SearchSymbolsTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.query, "vector");
        assert_eq!(tool.kinds, Some(vec!["class".to_string(), "function".to_string()]));
        assert_eq!(tool.max_results, Some(50));
        assert_eq!(tool.include_external, None);
        assert_eq!(tool.files, None);
    }

    #[test]
    fn test_search_symbols_tool_minimal() {
        let json_data = json!({
            "query": "main"
        });
        let tool: SearchSymbolsTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.query, "main");
        assert_eq!(tool.kinds, None);
        assert_eq!(tool.max_results, None);
        assert_eq!(tool.include_external, None);
        assert_eq!(tool.files, None);
    }
}
