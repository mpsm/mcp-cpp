//! MCP Tool implementations for C++ code analysis
//! 
//! This module contains all the MCP tools that provide C++ code analysis capabilities
//! using clangd LSP server integration.

pub mod cmake_tools;
pub mod lsp_tools;
pub mod symbol_filtering;
pub mod search_symbols;
pub mod analyze_symbols;

use rust_mcp_sdk::schema::{CallToolResult, schema_utils::CallToolError};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::lsp::ClangdManager;

pub use cmake_tools::ListBuildDirsTool;
pub use lsp_tools::LspRequestTool;
pub use search_symbols::SearchSymbolsTool;
pub use analyze_symbols::AnalyzeSymbolContextTool;

/// Helper function to serialize JSON content and handle errors gracefully
pub fn serialize_result(content: &serde_json::Value) -> String {
    serde_json::to_string_pretty(content)
        .unwrap_or_else(|e| format!("Error serializing result: {}", e))
}

/// MCP Tool Design Guidelines
///
/// **Async vs Sync Pattern:**
/// - Tools that interact with external processes, file I/O, or network should be **async**
/// - Tools that perform pure computation or analysis on in-memory data should be **sync**
///
/// **Current Tool Classifications:**
/// - `ListBuildDirsTool` - **sync** (file system analysis, no process interaction)
/// - `LspRequestTool` - **async** (sends requests to LSP process)
/// - `SearchSymbolsTool` - **async** (uses LSP for symbol search)
/// - `AnalyzeSymbolContextTool` - **async** (uses LSP for symbol analysis)
///
/// **Error Handling Pattern:**
/// - All tools use `CallToolResult::text_content()` for responses
/// - All tools use `serialize_result()` helper for consistent JSON formatting
/// - Errors are logged with appropriate level (error, warn, info) before returning

// Tool definitions using mcp_tool! macro for automatic schema generation
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "name")]
pub enum CppTools {
    #[serde(rename = "list_build_dirs")]
    ListBuildDirs(ListBuildDirsTool),
    #[serde(rename = "lsp_request")]
    LspRequest(LspRequestTool),
    #[serde(rename = "search_symbols")]
    SearchSymbols(SearchSymbolsTool),
    #[serde(rename = "analyze_symbol_context")]
    AnalyzeSymbolContext(AnalyzeSymbolContextTool),
}

impl CppTools {
    pub fn tools() -> Vec<rust_mcp_sdk::schema::Tool> {
        vec![
            ListBuildDirsTool::tool(),
            LspRequestTool::tool(),
            SearchSymbolsTool::tool(),
            AnalyzeSymbolContextTool::tool(),
        ]
    }

    pub async fn handle_call(
        tool_name: &str,
        arguments: serde_json::Value,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        use tracing::info;
        
        info!("Handling tool call: {}", tool_name);

        match tool_name {
            "list_build_dirs" => {
                let tool: ListBuildDirsTool = serde_json::from_value(arguments).map_err(|e| {
                    CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to deserialize list_build_dirs arguments: {}", e)
                    ))
                })?;
                tool.call_tool()
            }
            "lsp_request" => {
                let tool: LspRequestTool = serde_json::from_value(arguments).map_err(|e| {
                    CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to deserialize lsp_request arguments: {}", e)
                    ))
                })?;
                tool.call_tool(clangd_manager).await
            }
            "search_symbols" => {
                let tool: SearchSymbolsTool = serde_json::from_value(arguments).map_err(|e| {
                    CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to deserialize search_symbols arguments: {}", e)
                    ))
                })?;
                tool.call_tool(clangd_manager).await
            }
            "analyze_symbol_context" => {
                let tool: AnalyzeSymbolContextTool = serde_json::from_value(arguments).map_err(|e| {
                    CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to deserialize analyze_symbol_context arguments: {}", e)
                    ))
                })?;
                tool.call_tool(clangd_manager).await
            }
            _ => Err(CallToolError::unknown_tool(format!(
                "Unknown tool: {}",
                tool_name
            ))),
        }
    }
}
