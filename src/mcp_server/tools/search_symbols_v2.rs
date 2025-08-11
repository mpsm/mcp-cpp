//! Symbol search functionality v2 using new session-based API

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument};

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
// SymbolKind functionality now comes directly from lsp-types
use crate::lsp::traits::LspClientTrait;
use crate::project::{ProjectComponent, ProjectWorkspace};

#[mcp_tool(
    name = "search_symbols",
    description = "Advanced C++ symbol search using clangd LSP with project-aware filtering"
)]
#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct SearchSymbolsTool {
    /// Search query using clangd's native syntax
    pub query: String,

    /// Optional symbol kinds to filter results
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

impl<'de> serde::Deserialize<'de> for SearchSymbolsTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Helper {
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

        let helper = Helper::deserialize(deserializer)?;
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
    #[instrument(name = "search_symbols_v2", skip(self, session, workspace))]
    pub async fn call_tool_v2(
        &self,
        session: Arc<Mutex<ClangdSession>>,
        workspace: &ProjectWorkspace,
    ) -> Result<CallToolResult, CallToolError> {
        info!(
            "Searching symbols (v2): query='{}', kinds={:?}, max_results={:?}",
            self.query, self.kinds, self.max_results
        );

        let mut session_guard = session.lock().await;

        // Wait for clangd indexing to complete before searching
        super::utils::wait_for_indexing(session_guard.index_monitor(), None).await;

        // Get the component for this session's build directory
        let build_dir = session_guard.build_directory();
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
            self.search_in_files(session_guard.client_mut(), files, component)
                .await?
        } else {
            // Workspace-wide search using workspace symbols for comprehensive discovery
            self.search_workspace_symbols(session_guard.client_mut(), component)
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

    /// Handle workspace-wide symbol search
    async fn search_workspace_symbols(
        &self,
        client: &mut impl LspClientTrait,
        component: &ProjectComponent,
    ) -> Result<serde_json::Value, CallToolError> {
        let symbols = client
            .workspace_symbols(self.query.clone())
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!(
                    "Failed to search symbols: {}",
                    e
                )))
            })?;

        let filtered_symbols = self.filter_workspace_symbols(symbols, component);
        let limited_symbols = self.apply_result_limit(filtered_symbols);

        let symbols_json: Vec<_> = limited_symbols
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "kind": s.kind,
                    "location": s.location,
                    "containerName": s.container_name
                })
            })
            .collect();

        // Convert LSP numeric symbol kinds to string representations for MCP client compatibility.
        // This ensures consistent symbol type representation across different MCP client implementations.
        let converted_symbols = Self::convert_symbol_kinds(symbols_json);

        Ok(json!({
            "success": true,
            "query": self.query,
            "total_matches": converted_symbols.len(),
            "symbols": converted_symbols,
            "metadata": {
                "search_type": "workspace",
                "build_directory": component.build_dir_path.display().to_string(),
            }
        }))
    }

    /// Handle file-specific document symbol search
    async fn search_in_files(
        &self,
        client: &mut impl LspClientTrait,
        files: &[String],
        component: &ProjectComponent,
    ) -> Result<serde_json::Value, CallToolError> {
        let mut all_symbols = Vec::new();
        let mut processed_files = Vec::new();

        for file_path in files {
            let file_result = self.process_single_file(client, file_path).await;
            match file_result {
                Ok(symbols) => {
                    processed_files.push(json!({
                        "file": file_path,
                        "status": "success",
                        "symbols_found": symbols.len()
                    }));
                    all_symbols.extend(symbols);
                }
                Err(error_msg) => {
                    processed_files.push(json!({
                        "file": file_path,
                        "status": "error",
                        "error": error_msg
                    }));
                }
            }
        }

        let filtered_symbols = self.filter_file_symbols(all_symbols, component);
        let limited_symbols = self.apply_json_result_limit(filtered_symbols);

        // Convert LSP numeric symbol kinds to string representations for MCP client compatibility.
        // This ensures consistent symbol type representation across different MCP client implementations.
        let converted_symbols = Self::convert_symbol_kinds(limited_symbols);

        Ok(json!({
            "success": true,
            "query": self.query,
            "total_matches": converted_symbols.len(),
            "symbols": converted_symbols,
            "metadata": {
                "search_type": "file_specific",
                "files_processed": processed_files,
                "build_directory": component.build_dir_path.display().to_string(),
            }
        }))
    }

    /// Process a single file for document symbols
    async fn process_single_file(
        &self,
        client: &mut impl LspClientTrait,
        file_path: &str,
    ) -> Result<Vec<serde_json::Value>, String> {
        let file_uri = self.normalize_file_uri(file_path);

        let response = client
            .text_document_document_symbol(file_uri)
            .await
            .map_err(|e| format!("Failed to get symbols: {}", e))?;

        self.extract_symbols_from_response(response)
    }

    /// Normalize file path to proper URI format
    fn normalize_file_uri(&self, file_path: &str) -> String {
        if file_path.starts_with("file://") {
            file_path.to_string()
        } else {
            format!("file://{}", file_path)
        }
    }

    /// Extract symbols from document symbol response
    fn extract_symbols_from_response(
        &self,
        response: lsp_types::DocumentSymbolResponse,
    ) -> Result<Vec<serde_json::Value>, String> {
        let mut symbols = Vec::new();

        match response {
            lsp_types::DocumentSymbolResponse::Flat(symbol_infos) => {
                for symbol in symbol_infos {
                    if self.symbol_matches_criteria(&symbol.name, symbol.kind) {
                        symbols.push(json!({
                            "name": symbol.name,
                            "kind": format!("{:?}", symbol.kind).to_lowercase(),
                            "location": symbol.location,
                            "containerName": symbol.container_name
                        }));
                    }
                }
            }
            lsp_types::DocumentSymbolResponse::Nested(document_symbols) => {
                self.extract_from_nested_symbols(&document_symbols, &mut symbols);
            }
        }

        Ok(symbols)
    }

    /// Extract symbols from nested document symbol structure
    fn extract_from_nested_symbols(
        &self,
        symbols: &[lsp_types::DocumentSymbol],
        output: &mut Vec<serde_json::Value>,
    ) {
        for symbol in symbols {
            if self.symbol_matches_criteria(&symbol.name, symbol.kind) {
                output.push(json!({
                    "name": symbol.name,
                    "kind": format!("{:?}", symbol.kind).to_lowercase(),
                    "range": symbol.range,
                    "selection_range": symbol.selection_range,
                    "detail": symbol.detail
                }));
            }

            // Recursively process children
            if let Some(children) = &symbol.children {
                self.extract_from_nested_symbols(children, output);
            }
        }
    }

    /// Validates symbol against configured search filters.
    /// Applies both name pattern matching and symbol kind filtering.
    fn symbol_matches_criteria(&self, name: &str, kind: lsp_types::SymbolKind) -> bool {
        self.name_matches_query(name) && self.kind_matches_filter(kind)
    }

    /// Performs case-insensitive substring matching against the search query.
    /// Returns true if the symbol name contains the query string as a substring.
    fn name_matches_query(&self, name: &str) -> bool {
        let query_lower = self.query.to_lowercase();
        let name_lower = name.to_lowercase();
        name_lower.contains(&query_lower)
    }

    /// Validates symbol kind against configured filter criteria.
    /// Returns true if no kind filter is specified or if the symbol kind
    /// is included in the allowed kinds set.
    fn kind_matches_filter(&self, kind: lsp_types::SymbolKind) -> bool {
        if let Some(kinds) = &self.kinds {
            let kind_str = format!("{:?}", kind).to_lowercase();
            kinds.iter().any(|k| k.eq_ignore_ascii_case(&kind_str))
        } else {
            true // No filter means all symbol kinds are accepted
        }
    }

    /// Apply result limit to workspace symbols
    fn apply_result_limit(
        &self,
        symbols: Vec<lsp_types::WorkspaceSymbol>,
    ) -> Vec<lsp_types::WorkspaceSymbol> {
        let max_results = self.max_results.unwrap_or(100).min(1000) as usize;
        symbols.into_iter().take(max_results).collect()
    }

    /// Apply result limit to JSON symbols
    fn apply_json_result_limit(&self, symbols: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        let max_results = self.max_results.unwrap_or(100).min(1000) as usize;
        symbols.into_iter().take(max_results).collect()
    }

    /// Filter workspace symbols based on project boundaries and kind
    fn filter_workspace_symbols(
        &self,
        symbols: Vec<lsp_types::WorkspaceSymbol>,
        component: &ProjectComponent,
    ) -> Vec<lsp_types::WorkspaceSymbol> {
        symbols
            .into_iter()
            .filter(|symbol| self.workspace_symbol_passes_filters(symbol, component))
            .collect()
    }

    /// Filter file symbols based on project boundaries
    fn filter_file_symbols(
        &self,
        symbols: Vec<serde_json::Value>,
        component: &ProjectComponent,
    ) -> Vec<serde_json::Value> {
        if self.include_external.unwrap_or(false) {
            return symbols;
        }

        symbols
            .into_iter()
            .filter(|symbol| self.file_symbol_is_in_project(symbol, component))
            .collect()
    }

    /// Applies project boundary and external library filtering rules.
    /// Determines whether a workspace symbol should be included in results
    /// based on location and external dependency policies.
    fn workspace_symbol_passes_filters(
        &self,
        symbol: &lsp_types::WorkspaceSymbol,
        component: &ProjectComponent,
    ) -> bool {
        // Apply symbol kind filtering if specified
        if let Some(kinds) = &self.kinds {
            let kind_str = format!("{:?}", symbol.kind).to_lowercase();
            if !kinds.iter().any(|k| k.eq_ignore_ascii_case(&kind_str)) {
                return false;
            }
        }

        // Apply project boundary filtering unless external symbols are explicitly included
        if !self.include_external.unwrap_or(false) {
            let uri_str = match &symbol.location {
                lsp_types::OneOf::Left(location) => location.uri.as_str(),
                lsp_types::OneOf::Right(workspace_location) => workspace_location.uri.as_str(),
            };

            if let Some(path) = uri_str.strip_prefix("file://")
                && !self.is_project_file(path, component)
            {
                return false;
            }
        }

        true
    }

    /// Validates whether a file-based symbol belongs to the project scope.
    /// Uses URI parsing and project boundary detection to filter external dependencies.
    fn file_symbol_is_in_project(
        &self,
        symbol: &serde_json::Value,
        component: &ProjectComponent,
    ) -> bool {
        if let Some(location) = symbol.get("location")
            && let Some(uri) = location.get("uri").and_then(|u| u.as_str())
            && let Some(path) = uri.strip_prefix("file://")
        {
            return self.is_project_file(path, component);
        }
        true // Default to inclusion when file path parsing fails to prevent inadvertent filtering
    }

    /// Determines if a file path belongs to the project source tree.
    /// Uses canonical path resolution to handle symlinks and relative paths correctly.
    fn is_project_file(&self, path: &str, component: &ProjectComponent) -> bool {
        let file_path = std::path::PathBuf::from(path);

        // Verify file is within the project source root boundary
        if let Ok(canonical_file) = file_path.canonicalize()
            && canonical_file.starts_with(&component.source_root_path)
        {
            return true;
        }

        false
    }

    /// Converts LSP numeric symbol kinds to string representations for MCP client compatibility.
    /// Uses serde deserialization to maintain compatibility with LSP specification
    /// while providing type safety for subsequent filtering operations.
    fn convert_symbol_kinds(symbols: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        symbols
            .into_iter()
            .map(|mut symbol| {
                if let Some(kind_num) = symbol.get("kind").and_then(|k| k.as_u64()) {
                    // Convert numeric kind to strongly-typed SymbolKind enum via serde
                    if let Ok(kind_enum) = serde_json::from_value::<lsp_types::SymbolKind>(
                        serde_json::Value::Number(serde_json::Number::from(kind_num)),
                    ) {
                        let kind_str = format!("{:?}", kind_enum).to_lowercase();
                        symbol["kind"] = serde_json::Value::String(kind_str);
                    }
                }
                symbol
            })
            .collect()
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
