//! Symbol search functionality using session-based API

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument};

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::mcp_server::tools::lsp_helpers::document_symbols::SymbolSearchBuilder;
use crate::mcp_server::tools::lsp_helpers::workspace_symbols::WorkspaceSymbolSearchBuilder;
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
    pub kinds: Option<Vec<lsp_types::SymbolKind>>,

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
            kinds: Option<Vec<lsp_types::SymbolKind>>,
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
    #[instrument(name = "search_symbols", skip(self, session, workspace))]
    pub async fn call_tool(
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
            self.search_in_files(&mut session_guard, files, component)
                .await?
        } else {
            // Workspace-wide search using workspace symbols for comprehensive discovery
            self.search_workspace_symbols(&mut session_guard, component)
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
    ) -> Result<serde_json::Value, CallToolError> {
        // Build the search using the new helper's builder pattern
        let mut search_builder = WorkspaceSymbolSearchBuilder::new(self.query.clone())
            .include_external(self.include_external.unwrap_or(false));

        // Add kind filtering if specified
        if let Some(ref kinds) = self.kinds {
            search_builder = search_builder.with_kinds(kinds.clone());
        }

        // Add result limiting
        if let Some(max) = self.max_results {
            search_builder = search_builder.with_max_results(max);
        }

        // Execute the search
        let symbols = search_builder
            .search(session, component)
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!(
                    "Failed to search symbols: {}",
                    e
                )))
            })?;

        // Convert to JSON format
        let symbols_json: Vec<_> = symbols
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
        let converted_symbols = super::utils::convert_symbol_kinds(symbols_json);

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
        session: &mut ClangdSession,
        files: &[String],
        component: &ProjectComponent,
    ) -> Result<serde_json::Value, CallToolError> {
        // Build the search using the document symbols helper's builder pattern
        let mut search_builder = SymbolSearchBuilder::new().with_name(&self.query);

        // Add kind filtering if specified
        if let Some(ref kinds) = self.kinds {
            search_builder = search_builder.with_kinds(kinds);
        }

        // Execute the search with top-level limiting
        let file_results = search_builder
            .search_multiple_files(session, files, self.max_results)
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!(
                    "Failed to search files: {}",
                    e
                )))
            })?;

        // Convert to the expected JSON format for backward compatibility
        let mut all_symbols = Vec::new();
        let mut processed_files = Vec::new();

        for (file_path, symbols) in file_results {
            processed_files.push(json!({
                "file": file_path,
                "status": "success",
                "symbols_found": symbols.len()
            }));

            // Convert DocumentSymbol to JSON format
            for symbol in symbols {
                all_symbols.push(json!({
                    "name": symbol.name,
                    "kind": format!("{:?}", symbol.kind).to_lowercase(),
                    "range": symbol.range,
                    "selection_range": symbol.selection_range,
                    "detail": symbol.detail
                }));
            }
        }

        // Convert LSP numeric symbol kinds to string representations for MCP client compatibility
        let converted_symbols = super::utils::convert_symbol_kinds(all_symbols);

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_search_symbols_deserialize() {
        let json_data = json!({
            "query": "vector",
            "kinds": [5, 12], // CLASS = 5, FUNCTION = 12
            "max_results": 50
        });
        let tool: SearchSymbolsTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.query, "vector");
        assert_eq!(
            tool.kinds,
            Some(vec![
                lsp_types::SymbolKind::CLASS,
                lsp_types::SymbolKind::FUNCTION
            ])
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
