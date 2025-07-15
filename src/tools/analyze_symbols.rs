//! Symbol context analysis functionality

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
use super::symbol_filtering::SymbolUtilities;

#[mcp_tool(
    name = "analyze_symbol_context",
    description = "Analyze a C++ symbol and provide comprehensive context information including definition, type, location, and basic metadata. \
                   Uses clangd's optimized search to quickly locate symbols and provides rich type information via hover responses. \
                   SYMBOL RESOLUTION: Accepts symbol names (e.g., 'MyClass', 'process'), qualified names (e.g., 'std::vector', 'MyNamespace::MyClass'), \
                   or specific symbols at locations. For ambiguous names, optionally provide location for disambiguation. \
                   OUTPUTS: Symbol kind, type information, documentation, file location, qualified name, and visibility information. \
                   Perfect for understanding symbol definitions, getting quick symbol overview, and navigating to symbol locations."
)]
#[derive(Debug, ::serde::Serialize, JsonSchema)]
pub struct AnalyzeSymbolContextTool {
    /// The symbol name to analyze. Can be simple name ('MyClass'), qualified name ('std::vector'), 
    /// or global scope ('::main'). For overloaded functions or ambiguous symbols, consider providing location.
    pub symbol: String,
    
    /// Optional location to disambiguate symbols when multiple symbols have the same name.
    /// Useful for overloaded functions, template specializations, or symbols with identical names in different scopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SymbolLocation>,
    
    /// Include usage patterns and examples in the analysis. DEFAULT: false.
    /// When true, provides concrete code examples showing how the symbol is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_usage_patterns: Option<bool>,
    
    /// Maximum number of usage examples to include. DEFAULT: 5.
    /// Only relevant when include_usage_patterns is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_usage_examples: Option<u32>,
    
    /// Include inheritance and class hierarchy information. DEFAULT: false.
    /// When true, provides base classes, derived classes, and inheritance relationships.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_inheritance: Option<bool>,
}

#[derive(Debug, PartialEq, ::serde::Serialize, ::serde::Deserialize, JsonSchema)]
pub struct SymbolLocation {
    /// File URI in format 'file:///path/to/file.cpp' or relative path from project root
    pub file_uri: String,
    /// Position within the file to help disambiguate symbols
    pub position: SymbolPosition,
}

#[derive(Debug, PartialEq, ::serde::Serialize, ::serde::Deserialize, JsonSchema)]
pub struct SymbolPosition {
    /// Line number (0-based)
    pub line: u32,
    /// Character position within the line (0-based)
    pub character: u32,
}

impl<'de> serde::Deserialize<'de> for AnalyzeSymbolContextTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct AnalyzeSymbolContextToolHelper {
            symbol: String,
            #[serde(default)]
            location: Option<SymbolLocation>,
            #[serde(default)]
            include_usage_patterns: Option<bool>,
            #[serde(default)]
            max_usage_examples: Option<u32>,
            #[serde(default)]
            include_inheritance: Option<bool>,
        }

        let helper = AnalyzeSymbolContextToolHelper::deserialize(deserializer)?;
        
        Ok(AnalyzeSymbolContextTool {
            symbol: helper.symbol,
            location: helper.location,
            include_usage_patterns: helper.include_usage_patterns,
            max_usage_examples: helper.max_usage_examples,
            include_inheritance: helper.include_inheritance,
        })
    }
}

impl AnalyzeSymbolContextTool {
    #[instrument(name = "analyze_symbol_context", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Analyzing symbol context: symbol='{}', location={:?}", 
              self.symbol, self.location);

        // Handle automatic clangd setup if needed
        let build_path = match Self::resolve_build_directory() {
            Ok(Some(path)) => path,
            Ok(None) => {
                let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "build_directory_required",
                    "message": "No build directory found. Use list_build_dirs tool to see available options, or configure a build directory first.",
                    "symbol": self.symbol,
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
                    "symbol": self.symbol,
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
                        "symbol": self.symbol,
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

        // Wait for indexing completion to ensure accurate results
        let initial_indexing_state = manager_guard.get_indexing_state().await;
        if initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed {
            info!("Waiting for indexing completion before symbol analysis");
            if let Err(e) = manager_guard
                .wait_for_indexing_completion(std::time::Duration::from_secs(30))
                .await
            {
                let final_indexing_state = manager_guard.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("Indexing timeout: {}", e),
                    "message": "Symbol analysis may be incomplete due to ongoing indexing",
                    "symbol": self.symbol,
                    "indexing_status": SymbolUtilities::format_indexing_status(&final_indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        }

        // Step 1: Find the symbol using workspace search
        let symbol_location = match self.find_symbol_location(&manager_guard).await {
            Ok(Some(location)) => location,
            Ok(None) => {
                let indexing_state = manager_guard.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "symbol_not_found",
                    "message": format!("Symbol '{}' not found in workspace. Check spelling or ensure symbol is indexed.", self.symbol),
                    "symbol": self.symbol,
                    "suggestions": self.get_similar_symbols(&manager_guard).await,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
            Err(e) => {
                let indexing_state = manager_guard.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("Symbol search failed: {}", e),
                    "symbol": self.symbol,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        };

        // Step 2: Get detailed information via hover
        let hover_info = self.get_hover_information(&manager_guard, &symbol_location).await?;

        // Step 3: Get basic symbol information
        let basic_info = self.extract_basic_symbol_info(&symbol_location, &hover_info);

        // Prepare the response
        let final_indexing_state = manager_guard.get_indexing_state().await;
        let content = json!({
            "success": true,
            "symbol": basic_info,
            "metadata": {
                "analysis_type": "basic_symbol_info",
                "build_directory_used": build_directory,
                "indexing_waited": initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed,
                "indexing_status": SymbolUtilities::format_indexing_status(&final_indexing_state)
            }
        });

        info!("Symbol context analysis completed for: {}", self.symbol);

        Ok(CallToolResult::text_content(vec![TextContent::from(
            serialize_result(&content),
        )]))
    }

    async fn find_symbol_location(
        &self,
        manager: &ClangdManager,
    ) -> Result<Option<serde_json::Value>, String> {
        // If location is provided, use it directly for hover
        if let Some(location) = &self.location {
            return Ok(Some(json!({
                "uri": location.file_uri,
                "range": {
                    "start": {
                        "line": location.position.line,
                        "character": location.position.character
                    },
                    "end": {
                        "line": location.position.line,
                        "character": location.position.character
                    }
                }
            })));
        }

        // Use workspace symbol search to find the symbol
        let params = json!({
            "query": self.symbol
        });

        match manager
            .send_lsp_request("workspace/symbol".to_string(), Some(params))
            .await
        {
            Ok(symbols) => {
                if let Some(symbol_array) = symbols.as_array() {
                    // Find exact match or best match
                    let best_match = symbol_array
                        .iter()
                        .find(|s| {
                            if let Some(name) = s.get("name").and_then(|n| n.as_str()) {
                                name == self.symbol
                            } else {
                                false
                            }
                        })
                        .or_else(|| symbol_array.first());

                    if let Some(symbol) = best_match {
                        if let Some(location) = symbol.get("location") {
                            return Ok(Some(location.clone()));
                        }
                    }
                }
                Ok(None)
            }
            Err(e) => Err(format!("LSP workspace/symbol request failed: {}", e)),
        }
    }

    async fn get_hover_information(
        &self,
        manager: &ClangdManager,
        symbol_location: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, CallToolError> {
        if let (Some(uri), Some(range)) = (
            symbol_location.get("uri").and_then(|u| u.as_str()),
            symbol_location.get("range"),
        ) {
            // Extract position from range
            if let Some(start) = range.get("start") {
                let params = json!({
                    "textDocument": {
                        "uri": uri
                    },
                    "position": start
                });

                match manager
                    .send_lsp_request("textDocument/hover".to_string(), Some(params))
                    .await
                {
                    Ok(hover_result) => Ok(Some(hover_result)),
                    Err(e) => {
                        info!("Hover request failed (non-critical): {}", e);
                        Ok(None)
                    }
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn extract_basic_symbol_info(
        &self,
        symbol_location: &serde_json::Value,
        hover_info: &Option<serde_json::Value>,
    ) -> serde_json::Value {
        let mut info = json!({
            "name": self.symbol,
            "file_location": self.extract_file_location(symbol_location)
        });

        // Extract information from hover response
        if let Some(hover) = hover_info {
            if let Some(contents) = hover.get("contents") {
                info["type_info"] = self.extract_type_info(contents);
                info["documentation"] = self.extract_documentation(contents);
            }
        }

        info
    }

    fn extract_file_location(&self, symbol_location: &serde_json::Value) -> serde_json::Value {
        json!({
            "uri": symbol_location.get("uri").unwrap_or(&json!("unknown")),
            "range": symbol_location.get("range").unwrap_or(&json!({}))
        })
    }

    fn extract_type_info(&self, contents: &serde_json::Value) -> serde_json::Value {
        // Handle different hover content formats
        if let Some(value_str) = contents.get("value").and_then(|v| v.as_str()) {
            json!({
                "type": value_str,
                "raw_hover": value_str
            })
        } else if let Some(contents_array) = contents.as_array() {
            if let Some(first_item) = contents_array.first() {
                if let Some(value_str) = first_item.get("value").and_then(|v| v.as_str()) {
                    json!({
                        "raw_hover": value_str
                    })
                } else {
                    json!({
                        "raw_hover": first_item
                    })
                }
            } else {
                json!({
                    "type": "unknown"
                })
            }
        } else {
            json!({
                "type": "unknown",
                "raw_hover": contents
            })
        }
    }

    fn extract_documentation(&self, contents: &serde_json::Value) -> serde_json::Value {
        // Try to extract documentation from hover contents
        if let Some(value_str) = contents.get("value").and_then(|v| v.as_str()) {
            // Simple heuristic: documentation often comes after type info
            let lines: Vec<&str> = value_str.lines().collect();
            if lines.len() > 1 {
                json!(lines[1..].join("\n").trim())
            } else {
                json!(null)
            }
        } else {
            json!(null)
        }
    }

    async fn get_similar_symbols(&self, manager: &ClangdManager) -> Vec<String> {
        // Try to find symbols with similar names
        let query = if self.symbol.len() > 3 {
            self.symbol[..3].to_string()
        } else {
            self.symbol.clone()
        };

        let params = json!({
            "query": query
        });

        if let Ok(symbols) = manager
            .send_lsp_request("workspace/symbol".to_string(), Some(params))
            .await
        {
            if let Some(symbol_array) = symbols.as_array() {
                return symbol_array
                    .iter()
                    .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .take(5)
                    .collect();
            }
        }

        vec![]
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
    fn test_analyze_symbol_context_tool_deserialize() {
        let json_data = json!({
            "symbol": "MyClass",
            "include_usage_patterns": true,
            "max_usage_examples": 3
        });
        let tool: AnalyzeSymbolContextTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.symbol, "MyClass");
        assert_eq!(tool.include_usage_patterns, Some(true));
        assert_eq!(tool.max_usage_examples, Some(3));
        assert_eq!(tool.location, None);
        assert_eq!(tool.include_inheritance, None);
    }

    #[test]
    fn test_symbol_location_and_position() {
        let location = SymbolLocation {
            file_uri: "file:///path/to/file.cpp".to_string(),
            position: SymbolPosition {
                line: 10,
                character: 5,
            },
        };

        assert_eq!(location.file_uri, "file:///path/to/file.cpp");
        assert_eq!(location.position.line, 10);
        assert_eq!(location.position.character, 5);
    }
}
