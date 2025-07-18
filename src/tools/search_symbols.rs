//! Symbol search functionality

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument, warn};

use super::serialize_result;
use super::symbol_filtering::{SymbolFilter, SymbolUtilities};
use crate::cmake::CmakeProjectStatus;
use crate::lsp::ClangdManager;

#[mcp_tool(
    name = "search_symbols",
    description = "Advanced C++ symbol discovery and exploration engine using clangd LSP server with intelligent \
                   filtering, scope control, and project boundary detection. Perfect for rapid code navigation \
                   and comprehensive symbol analysis across large C++ codebases.

                   🔍 QUERY SYNTAX CAPABILITIES:
                   • Fuzzy matching: 'vector' → finds std::vector, vector_impl, my_vector
                   • Qualified names: 'std::vector', 'MyNamespace::MyClass' 
                   • Namespace exploration: 'MyNamespace::' → lists all namespace members
                   • Global scope: '::main', '::global_function' → global symbols only
                   • Prefix matching: 'get_' → finds all getter methods
                   • Partial matching: 'Math' → MathUtils, BasicMath, etc.

                   ⚠️ WILDCARD LIMITATIONS:
                   • Traditional wildcards NOT supported: 'Math*' does NOT find symbols starting with 'Math'
                   • Bare '*' returns empty results (clangd evaluates as false)
                   • Trailing '*' is ignored: 'get*' searches for 'get' only
                   • Use fuzzy search instead: 'Math' finds Math, MathUtils, MathClass, etc.
                   • For broader searches use: namespace patterns ('std::', 'MyNS::') or short prefixes

                   📋 SYMBOL KIND TAXONOMY:
                   Comprehensive support for all C++ constructs: classes • structs • enums • functions
                   • methods • variables • fields • namespaces • typedefs • macros • constructors
                   • destructors • operators • interfaces • properties • events • constants • arrays
                   • type parameters. Each result includes precise kind classification.

                   🎯 INTELLIGENT SCOPE CONTROL:
                   • PROJECT SCOPE (default): Searches only project files and headers
                   • EXTERNAL SCOPE: Enable include_external=true for system libraries (std::, boost::)
                   • Project boundaries auto-detected via CMake compilation database
                   • Common ancestor analysis for multi-module projects

                   📁 FILE FILTERING SYSTEM:
                   • Relative paths: 'src/math.cpp', 'include/utils/*.h'
                   • Absolute paths: '/home/project/src/main.cpp'
                   • Directory filtering: Target specific modules or components
                   • When specified, uses document symbols for detailed per-file results

                   📊 RICH OUTPUT FORMAT:
                   Each result includes: symbol name • kind classification • qualified name
                   • precise file location • line/column coordinates • container scope
                   • signature information • documentation snippets

                   🚀 PERFORMANCE & FEATURES:
                   • Leverages clangd's high-speed indexing for sub-second searches
                   • Results ranked by relevance and usage frequency  
                   • Configurable result limits (default: 100, max: 1000)
                   • Automatic build directory detection and clangd initialization

                   🎯 PRIMARY USE CASES:
                   Code exploration • API discovery • Refactoring preparation • Dependency analysis
                   • Navigation in unfamiliar codebases • Architecture understanding • Symbol validation

                   INPUT REQUIREMENTS:
                   • query: Required search string using above syntax
                   • kinds: Optional array to filter by symbol types  
                   • include_external: Optional boolean for system library inclusion
                   • files: Optional array for file-specific searches
                   • max_results: Optional limit (1-1000, default 100)"
)]
#[derive(Debug, ::serde::Serialize, JsonSchema)]
pub struct SearchSymbolsTool {
    /// Search query using clangd's native syntax. REQUIRED.
    ///
    /// SYNTAX OPTIONS:
    /// • Fuzzy matching: "vector" → finds std::vector, my_vector, vector_impl
    /// • Qualified names: "std::vector", "MyNamespace::MyClass"
    /// • Namespace exploration: "MyNamespace::" → lists all namespace members  
    /// • Global scope: "::main", "::global_var" → global symbols only
    /// • Prefix matching: "get_" → finds all getters, "set_" → all setters
    /// • Class methods: "MyClass::" → all class members
    ///
    /// WILDCARD LIMITATIONS:
    /// • Traditional wildcards NOT supported: "Math*" does NOT work as expected
    /// • Bare "*" returns empty results - use fuzzy search instead
    /// • Use "Math" to find Math, MathUtils, MathClass, etc.
    pub query: String,

    /// Optional symbol kinds to filter results by type. DEFAULT: all kinds.
    ///
    /// SUPPORTED KINDS: "class", "struct", "enum", "function", "method", "variable",
    /// "field", "namespace", "typedef", "macro", "constructor", "destructor", "operator",
    /// "interface", "property", "event", "constant", "array", "boolean", "key", "null",
    /// "number", "object", "string", "enumMember", "typeParameter"
    ///
    /// EXAMPLES: ["class", "struct"] → only classes and structs
    ///           ["function", "method"] → only callable symbols
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,

    /// Optional file paths to limit search scope. DEFAULT: entire workspace.
    ///
    /// FORMATS ACCEPTED:
    /// • Relative paths: "src/math.cpp", "include/utils.h"
    /// • Absolute paths: "/home/project/src/main.cpp"
    /// • Directory patterns: "src/", "include/math/"
    ///
    /// BEHAVIOR: When specified, uses document symbol search for detailed
    /// per-file results instead of workspace-wide search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,

    /// Maximum number of results to return. DEFAULT: 100. RANGE: 1-1000.
    ///
    /// Use lower values (10-50) for quick exploration, higher values (200-1000)
    /// for comprehensive analysis. Results are ranked by clangd relevance scoring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,

    /// Include external symbols from system libraries. DEFAULT: false.
    ///
    /// PROJECT SCOPE (false): Only project files and project-specific headers
    /// EXTERNAL SCOPE (true): Includes std::, boost::, system headers, third-party libs
    ///
    /// PERFORMANCE NOTE: External scope adds ~1-3 seconds for comprehensive searches.
    /// Project boundaries auto-detected via CMake compilation database analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_external: Option<bool>,

    /// Build directory path containing compile_commands.json. OPTIONAL.
    ///
    /// FORMATS ACCEPTED:
    /// • Relative path: "build", "build-debug", "out/Debug"
    /// • Absolute path: "/home/project/build", "/path/to/build-dir"
    ///
    /// BEHAVIOR: When specified, uses this build directory instead of auto-detection.
    /// The build directory must contain compile_commands.json for clangd integration.
    /// 
    /// AUTO-DETECTION (when not specified): Attempts to find single build directory
    /// in current workspace. Fails if multiple or zero build directories found.
    ///
    /// CLANGD SETUP: clangd CWD will be set to project root (from CMAKE_SOURCE_DIR),
    /// and build directory will be passed via --compile-commands-dir argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_directory: Option<String>,
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
            #[serde(default)]
            build_directory: Option<String>,
        }

        let helper = SearchSymbolsToolHelper::deserialize(deserializer)?;

        Ok(SearchSymbolsTool {
            query: helper.query,
            kinds: helper.kinds,
            files: helper.files,
            max_results: helper.max_results,
            include_external: helper.include_external,
            build_directory: helper.build_directory,
        })
    }
}

impl SearchSymbolsTool {
    #[instrument(name = "search_symbols", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!(
            "Searching symbols: query='{}', kinds={:?}, files={:?}, max_results={:?}, include_external={:?}, build_directory={:?}",
            self.query, self.kinds, self.files, self.max_results, self.include_external, self.build_directory
        );

        // Handle build directory parameter or automatic clangd setup
        let build_path = match &self.build_directory {
            Some(build_dir) => {
                let path = std::path::PathBuf::from(build_dir);
                if !path.exists() {
                    let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                    let content = json!({
                        "success": false,
                        "error": "build_directory_not_found",
                        "message": format!("Specified build directory '{}' does not exist.", build_dir),
                        "query": self.query,
                        "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                    });

                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]));
                }
                info!("Using provided build directory: {}", path.display());
                path
            }
            None => {
                // Use automatic build directory resolution
                match Self::resolve_build_directory() {
                    Ok(Some(path)) => {
                        info!("Auto-resolved build directory: {}", path.display());
                        path
                    }
                    Ok(None) => {
                        let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                        let content = json!({
                            "success": false,
                            "error": "build_directory_required",
                            "message": "No build directory found. Use list_build_dirs tool to see available options, or specify build_directory parameter.",
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
                            "message": "Failed to analyze build directories. Use list_build_dirs tool to see available options, or specify build_directory parameter.",
                            "query": self.query,
                            "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                        });

                        return Ok(CallToolResult::text_content(vec![TextContent::from(
                            serialize_result(&content),
                        )]));
                    }
                }
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
                format!("file://{file_path}")
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
                                        SymbolUtilities::matches_query_and_filters(
                                            symbol,
                                            &self.query,
                                            &self.kinds,
                                        )
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
        let filtered_symbols =
            SymbolFilter::filter_symbols(all_symbols, include_external, &self.kinds, manager).await;
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
        info!(
            "🔍 SearchSymbolsTool::search_workspace() - Initial indexing state: {:?}, is_indexing: {}, message: {:?}",
            initial_indexing_state.status,
            initial_indexing_state.is_indexing(),
            initial_indexing_state.message
        );

        // Wait for indexing to complete if not already completed
        if initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed {
            info!(
                "⏳ SearchSymbolsTool::search_workspace() - Waiting for indexing completion before workspace symbol search (current status: {:?})",
                initial_indexing_state.status
            );
            if let Err(e) = manager
                .wait_for_indexing_completion(std::time::Duration::from_secs(30))
                .await
            {
                let final_indexing_state = manager.get_indexing_state().await;
                warn!(
                    "⏰ SearchSymbolsTool::search_workspace() - Indexing wait timed out: {}, final state: {:?}",
                    e, final_indexing_state
                );
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
        } else {
            info!(
                "✅ SearchSymbolsTool::search_workspace() - Indexing already completed, proceeding with symbol search"
            );
        }

        // Send workspace symbol request with user's query
        let params = json!({
            "query": self.query
        });

        info!(
            "📡 SearchSymbolsTool::search_workspace() - Sending workspace/symbol LSP request with query: '{}'",
            self.query
        );
        match manager
            .send_lsp_request("workspace/symbol".to_string(), Some(params))
            .await
        {
            Ok(symbols) => {
                info!(
                    "✅ SearchSymbolsTool::search_workspace() - LSP request succeeded, received response"
                );
                let symbol_array = symbols.as_array().unwrap_or(&vec![]).clone();
                info!(
                    "📊 SearchSymbolsTool::search_workspace() - Raw symbols count: {}",
                    symbol_array.len()
                );

                // Apply external filtering and other filters
                let include_external = self.include_external.unwrap_or(false);
                info!(
                    "🔍 SearchSymbolsTool::search_workspace() - Applying filtering: include_external={}, kinds={:?}",
                    include_external, self.kinds
                );
                let filtered_symbols = SymbolFilter::filter_symbols(
                    symbol_array,
                    include_external,
                    &self.kinds,
                    manager,
                )
                .await;
                info!(
                    "📊 SearchSymbolsTool::search_workspace() - Filtered symbols count: {}",
                    filtered_symbols.len()
                );

                let limited_symbols =
                    SymbolUtilities::limit_results(filtered_symbols, self.max_results);
                info!(
                    "📊 SearchSymbolsTool::search_workspace() - Limited symbols count: {}",
                    limited_symbols.len()
                );

                let converted_symbols = SymbolUtilities::convert_symbol_kinds(limited_symbols);
                info!(
                    "📊 SearchSymbolsTool::search_workspace() - Final symbols count: {}",
                    converted_symbols.len()
                );

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

                info!("🎉 SearchSymbolsTool::search_workspace() - Successfully prepared response");
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(e) => {
                warn!(
                    "❌ SearchSymbolsTool::search_workspace() - LSP request failed: {}",
                    e
                );
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
                Err(format!("Failed to analyze build directories: {e}"))
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
        assert_eq!(
            tool.kinds,
            Some(vec!["class".to_string(), "function".to_string()])
        );
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
