//! Symbol filtering utilities and shared logic

use serde_json::json;
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::debug;

use crate::lsp::ClangdManager;

/// Symbol filtering utilities for project boundary detection and kind filtering
pub struct SymbolFilter;

impl SymbolFilter {
    /// Apply kind filtering to a list of symbols
    pub fn apply_kind_filter(
        symbols: Vec<serde_json::Value>,
        kinds: &Option<Vec<String>>,
    ) -> Vec<serde_json::Value> {
        if let Some(kinds) = kinds {
            symbols
                .into_iter()
                .filter(|symbol| {
                    if let Some(kind) = symbol.get("kind").and_then(|k| k.as_u64()) {
                        let kind_name = SymbolUtilities::symbol_kind_to_string(kind);
                        kinds
                            .iter()
                            .any(|k| k.to_lowercase() == kind_name.to_lowercase())
                    } else {
                        false
                    }
                })
                .collect()
        } else {
            symbols
        }
    }

    /// Determine if a symbol is part of the project (not external)
    pub fn is_project_symbol(
        symbol: &serde_json::Value,
        compilation_database: &Option<HashSet<PathBuf>>,
        project_root: &Option<PathBuf>,
    ) -> bool {
        // Extract file path from symbol location
        let file_path = if let Some(location) = symbol.get("location") {
            if let Some(uri) = location.get("uri").and_then(|u| u.as_str()) {
                if let Some(path_str) = uri.strip_prefix("file://") {
                    Some(PathBuf::from(path_str))
                } else {
                    Some(PathBuf::from(uri))
                }
            } else {
                None
            }
        } else {
            None
        };

        let _symbol_name = symbol
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");

        if let Some(path) = file_path {
            // First check if it's directly in the compilation database (source files)
            if let Some(db) = compilation_database {
                if db.contains(&path) {
                    return true;
                }
            }

            // If not in compilation database, check if it's a project header
            // by seeing if it's under the project source directory
            if let Some(root) = project_root {
                // Include any file under the project root
                if path.starts_with(root) {
                    return true;
                }
            }

            false
        } else {
            // If we can't determine the file, exclude it for safety
            false
        }
    }

    /// Filter symbols based on project boundaries and external inclusion settings
    pub async fn filter_symbols(
        symbols: Vec<serde_json::Value>,
        include_external: bool,
        kinds: &Option<Vec<String>>,
        manager: &ClangdManager,
    ) -> Vec<serde_json::Value> {
        debug!(
            "🔍 Filtering {} symbols, include_external={}",
            symbols.len(),
            include_external
        );

        if include_external {
            // Include all symbols when external is enabled
            debug!("✅ Including all symbols (external enabled)");
            Self::apply_kind_filter(symbols, kinds)
        } else {
            // Filter out external symbols (system headers, libraries, etc.)
            let compilation_database = manager.get_compilation_database().await;
            let project_root = manager.get_project_root().await;
            debug!("📁 Using compilation database and project root for project filtering");

            let filtered: Vec<_> = symbols
                .into_iter()
                .filter(|symbol| {
                    Self::is_project_symbol(symbol, &compilation_database, &project_root)
                })
                .collect();

            debug!(
                "📊 After project filtering: {} symbols remaining",
                filtered.len()
            );
            Self::apply_kind_filter(filtered, kinds)
        }
    }
}

/// Symbol utility functions for common operations
pub struct SymbolUtilities;

impl SymbolUtilities {
    /// Convert LSP symbol kind number to human-readable string
    pub fn symbol_kind_to_string(kind: u64) -> &'static str {
        // LSP SymbolKind enumeration
        match kind {
            1 => "file",
            2 => "module",
            3 => "namespace",
            4 => "package",
            5 => "class",
            6 => "method",
            7 => "property",
            8 => "field",
            9 => "constructor",
            10 => "enum",
            11 => "interface",
            12 => "function",
            13 => "variable",
            14 => "constant",
            15 => "string",
            16 => "number",
            17 => "boolean",
            18 => "array",
            19 => "object",
            20 => "key",
            21 => "null",
            22 => "enum_member",
            23 => "struct",
            24 => "event",
            25 => "operator",
            26 => "type_parameter",
            _ => "unknown",
        }
    }

    /// Convert symbol kinds from numeric to string representation
    pub fn convert_symbol_kinds(symbols: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        symbols
            .into_iter()
            .map(|mut symbol| {
                if let Some(kind_num) = symbol.get("kind").and_then(|k| k.as_u64()) {
                    let kind_name = Self::symbol_kind_to_string(kind_num);
                    symbol["kind"] = serde_json::Value::String(kind_name.to_string());
                }
                symbol
            })
            .collect()
    }

    /// Limit the number of results returned
    pub fn limit_results(
        symbols: Vec<serde_json::Value>,
        max_results: Option<u32>,
    ) -> Vec<serde_json::Value> {
        let max_results = max_results.unwrap_or(100) as usize;
        symbols.into_iter().take(max_results).collect()
    }

    /// Format indexing status for JSON output
    pub fn format_indexing_status(
        indexing_state: &crate::lsp::types::IndexingState,
    ) -> serde_json::Value {
        json!({
            "status": match indexing_state.status {
                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                crate::lsp::types::IndexingStatus::Completed => "completed",
            },
            "is_indexing": indexing_state.is_indexing(),
            "files_processed": indexing_state.files_processed,
            "total_files": indexing_state.total_files,
            "percentage": indexing_state.percentage,
            "message": indexing_state.message,
            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
        })
    }

    /// Check if a symbol matches query and filters for file-specific search
    pub fn matches_query_and_filters(
        symbol: &serde_json::Value,
        query: &str,
        kinds: &Option<Vec<String>>,
    ) -> bool {
        // For file-specific search, we need to do our own query matching
        // since clangd's documentSymbol doesn't take a query parameter
        if let Some(name) = symbol.get("name").and_then(|n| n.as_str()) {
            // Simple fuzzy matching - check if query is contained in symbol name (case insensitive)
            let query_lower = query.to_lowercase();
            let name_lower = name.to_lowercase();

            if !name_lower.contains(&query_lower) {
                return false;
            }
        } else {
            return false;
        }

        // Apply kind filtering
        if let Some(kinds) = kinds {
            if let Some(kind) = symbol.get("kind").and_then(|k| k.as_u64()) {
                let kind_name = Self::symbol_kind_to_string(kind);
                if !kinds
                    .iter()
                    .any(|k| k.to_lowercase() == kind_name.to_lowercase())
                {
                    return false;
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_to_string() {
        assert_eq!(SymbolUtilities::symbol_kind_to_string(5), "class");
        assert_eq!(SymbolUtilities::symbol_kind_to_string(12), "function");
        assert_eq!(SymbolUtilities::symbol_kind_to_string(13), "variable");
        assert_eq!(SymbolUtilities::symbol_kind_to_string(999), "unknown");
    }

    #[test]
    fn test_matches_query_and_filters() {
        let symbol = json!({
            "name": "TestClass",
            "kind": 5
        });

        // Test basic query matching
        assert!(SymbolUtilities::matches_query_and_filters(
            &symbol, "test", &None
        ));
        assert!(SymbolUtilities::matches_query_and_filters(
            &symbol, "Class", &None
        ));
        assert!(!SymbolUtilities::matches_query_and_filters(
            &symbol, "function", &None
        ));

        // Test kind filtering
        let class_kinds = Some(vec!["class".to_string()]);
        let function_kinds = Some(vec!["function".to_string()]);

        assert!(SymbolUtilities::matches_query_and_filters(
            &symbol,
            "test",
            &class_kinds
        ));
        assert!(!SymbolUtilities::matches_query_and_filters(
            &symbol,
            "test",
            &function_kinds
        ));
    }
}
