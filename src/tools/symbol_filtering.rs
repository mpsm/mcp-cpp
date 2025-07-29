//! Symbol filtering utilities and shared logic

use serde_json::json;
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::debug;

use crate::legacy_lsp::ClangdManager;

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

        let symbol_name = symbol
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");

        if let Some(path) = file_path {
            // First check if it's directly in the compilation database (source files)
            if let Some(db) = compilation_database {
                if db.contains(&path) {
                    debug!(
                        "✅ Symbol '{}' in {} is PROJECT: found in compilation database",
                        symbol_name,
                        path.display()
                    );
                    return true;
                }
            }

            // If not in compilation database, check if it's a project header
            // by seeing if it's under the project source directory
            if let Some(root) = project_root {
                // Include any file under the project root
                if path.starts_with(root) {
                    debug!(
                        "✅ Symbol '{}' in {} is PROJECT: under project root {}",
                        symbol_name,
                        path.display(),
                        root.display()
                    );
                    return true;
                } else {
                    debug!(
                        "❌ Symbol '{}' in {} is EXTERNAL: not under project root {}",
                        symbol_name,
                        path.display(),
                        root.display()
                    );
                }
            } else {
                debug!(
                    "⚠️  Symbol '{}' in {}: no project root available for filtering",
                    symbol_name,
                    path.display()
                );
            }

            false
        } else {
            // If we can't determine the file, exclude it for safety
            debug!(
                "❌ Symbol '{}': no file path available, excluding for safety",
                symbol_name
            );
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

            debug!("📁 Project filtering context:");
            if let Some(ref root) = project_root {
                debug!("   Project root: {}", root.display());
            } else {
                debug!("   Project root: None (⚠️  could cause issues with out-of-tree builds)");
            }

            if let Some(ref db) = compilation_database {
                debug!("   Compilation database: {} files", db.len());
                if db.len() <= 10 {
                    for file in db {
                        debug!("     - {}", file.display());
                    }
                } else {
                    // Show first few files as samples
                    for (i, file) in db.iter().enumerate() {
                        if i >= 3 {
                            debug!("     - ... and {} more files", db.len() - 3);
                            break;
                        }
                        debug!("     - {}", file.display());
                    }
                }
            } else {
                debug!("   Compilation database: None (⚠️  will only rely on project root)");
            }

            let mut project_symbols = 0;
            let mut external_symbols = 0;

            let filtered: Vec<_> = symbols
                .into_iter()
                .filter(|symbol| {
                    let is_project =
                        Self::is_project_symbol(symbol, &compilation_database, &project_root);
                    if is_project {
                        project_symbols += 1;
                    } else {
                        external_symbols += 1;
                    }
                    is_project
                })
                .collect();

            debug!(
                "📊 After project filtering: {} project symbols kept, {} external symbols dropped",
                project_symbols, external_symbols
            );
            debug!(
                "📊 Final result: {} symbols remaining out of {} original",
                filtered.len(),
                project_symbols + external_symbols
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
        indexing_state: &crate::legacy_lsp::types::IndexingState,
    ) -> serde_json::Value {
        json!({
            "status": match indexing_state.status {
                crate::legacy_lsp::types::IndexingStatus::NotStarted => "not_started",
                crate::legacy_lsp::types::IndexingStatus::InProgress => "in_progress",
                crate::legacy_lsp::types::IndexingStatus::Completed => "completed",
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
            let query_lower = query.to_lowercase();
            let name_lower = name.to_lowercase();

            // First try exact match with symbol name
            if name_lower.contains(&query_lower) {
                // Apply kind filtering before returning success
                return Self::apply_kind_filter_to_symbol(symbol, kinds);
            }

            // If no direct name match, try qualified name matching
            if let Some(container) = symbol.get("containerName").and_then(|c| c.as_str()) {
                let qualified_name = format!("{container}::{name}");
                let qualified_lower = qualified_name.to_lowercase();

                // Check if query matches the full qualified name
                if qualified_lower.contains(&query_lower) {
                    return Self::apply_kind_filter_to_symbol(symbol, kinds);
                }

                // Also check if the query is a partial qualified match
                // For example: "Complex::add" should match "Math::Complex::add"
                if query_lower.contains("::") {
                    // Split both the query and qualified name by "::" and check suffixes
                    let query_parts: Vec<&str> = query_lower.split("::").collect();
                    let qualified_parts: Vec<&str> = qualified_lower.split("::").collect();

                    // Check if query_parts is a suffix of qualified_parts
                    if query_parts.len() <= qualified_parts.len() {
                        let offset = qualified_parts.len() - query_parts.len();
                        if qualified_parts[offset..] == query_parts[..] {
                            return Self::apply_kind_filter_to_symbol(symbol, kinds);
                        }
                    }
                }
            }

            false
        } else {
            false
        }
    }

    /// Helper function to apply kind filtering to a single symbol
    fn apply_kind_filter_to_symbol(
        symbol: &serde_json::Value,
        kinds: &Option<Vec<String>>,
    ) -> bool {
        // Apply kind filtering
        if let Some(kinds) = kinds {
            if let Some(kind) = symbol.get("kind").and_then(|k| k.as_u64()) {
                let kind_name = Self::symbol_kind_to_string(kind);
                kinds
                    .iter()
                    .any(|k| k.to_lowercase() == kind_name.to_lowercase())
            } else {
                false
            }
        } else {
            true
        }
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

    #[test]
    fn test_matches_query_with_multiple_namespaces() {
        // Test symbol with nested namespaces like Math::Complex::add
        let symbol_with_container = json!({
            "name": "add",
            "kind": 12, // function
            "containerName": "Math::Complex"
        });

        // Test that it finds the symbol by simple name
        assert!(SymbolUtilities::matches_query_and_filters(
            &symbol_with_container,
            "add",
            &None
        ));

        // Test qualified name matching - should now work
        assert!(SymbolUtilities::matches_query_and_filters(
            &symbol_with_container,
            "Math::Complex::add",
            &None
        ));

        assert!(SymbolUtilities::matches_query_and_filters(
            &symbol_with_container,
            "Complex::add",
            &None
        ));

        // Test partial qualified matches
        assert!(SymbolUtilities::matches_query_and_filters(
            &symbol_with_container,
            "math::complex::add",
            &None // case insensitive
        ));

        // Test that non-matching qualified names don't match
        assert!(!SymbolUtilities::matches_query_and_filters(
            &symbol_with_container,
            "Other::Complex::add",
            &None
        ));

        assert!(!SymbolUtilities::matches_query_and_filters(
            &symbol_with_container,
            "Math::Other::add",
            &None
        ));

        // Test another nested symbol
        let nested_symbol = json!({
            "name": "max_element",
            "kind": 12, // function
            "containerName": "TestProject::Algorithms"
        });

        assert!(SymbolUtilities::matches_query_and_filters(
            &nested_symbol,
            "max_element",
            &None
        ));

        // These should now work for proper qualified name matching
        assert!(SymbolUtilities::matches_query_and_filters(
            &nested_symbol,
            "TestProject::Algorithms::max_element",
            &None
        ));

        assert!(SymbolUtilities::matches_query_and_filters(
            &nested_symbol,
            "Algorithms::max_element",
            &None
        ));

        // Test edge case with just "::" separator but partial match
        assert!(SymbolUtilities::matches_query_and_filters(
            &nested_symbol,
            "Algorithms",
            &None
        ));

        // Test that it correctly rejects non-matching namespace queries
        assert!(!SymbolUtilities::matches_query_and_filters(
            &nested_symbol,
            "WrongProject::Algorithms::max_element",
            &None
        ));
    }

    #[test]
    fn test_qualified_name_edge_cases() {
        // Test symbol with no container (global scope)
        let global_symbol = json!({
            "name": "global_function",
            "kind": 12 // function
        });

        assert!(SymbolUtilities::matches_query_and_filters(
            &global_symbol,
            "global_function",
            &None
        ));

        assert!(SymbolUtilities::matches_query_and_filters(
            &global_symbol,
            "global",
            &None
        ));

        // Global scope queries shouldn't match if there's no containerName
        assert!(!SymbolUtilities::matches_query_and_filters(
            &global_symbol,
            "::global_function",
            &None
        ));

        // Test symbol with single-level namespace
        let single_ns_symbol = json!({
            "name": "process",
            "kind": 12, // function
            "containerName": "std"
        });

        assert!(SymbolUtilities::matches_query_and_filters(
            &single_ns_symbol,
            "process",
            &None
        ));

        assert!(SymbolUtilities::matches_query_and_filters(
            &single_ns_symbol,
            "std::process",
            &None
        ));

        // Test deeply nested namespace
        let deep_nested = json!({
            "name": "parse",
            "kind": 12,
            "containerName": "boost::property_tree::json_parser"
        });

        assert!(SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "parse",
            &None
        ));

        assert!(SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "json_parser::parse",
            &None
        ));

        assert!(SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "property_tree::json_parser::parse",
            &None
        ));

        assert!(SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "boost::property_tree::json_parser::parse",
            &None
        ));

        // Should not match incorrect partial paths
        assert!(!SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "property_tree::parse",
            &None // skipping json_parser
        ));

        assert!(!SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "boost::parse",
            &None // skipping intermediate levels
        ));

        // Test with kind filtering combined with qualified names
        let function_kinds = Some(vec!["function".to_string()]);
        let class_kinds = Some(vec!["class".to_string()]);

        assert!(SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "json_parser::parse",
            &function_kinds
        ));

        assert!(!SymbolUtilities::matches_query_and_filters(
            &deep_nested,
            "json_parser::parse",
            &class_kinds
        ));
    }

    #[test]
    fn test_limit_results_basic() {
        let symbols = vec![
            json!({"name": "Symbol1", "kind": 5}),
            json!({"name": "Symbol2", "kind": 12}),
            json!({"name": "Symbol3", "kind": 13}),
        ];

        let limited = SymbolUtilities::limit_results(symbols, Some(2));
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0]["name"], "Symbol1");
        assert_eq!(limited[1]["name"], "Symbol2");
    }

    #[test]
    fn test_limit_results_default() {
        let symbols: Vec<serde_json::Value> = (0..150)
            .map(|i| json!({"name": format!("Symbol{}", i), "kind": 5}))
            .collect();

        let limited = SymbolUtilities::limit_results(symbols, None);
        assert_eq!(limited.len(), 100); // Default limit
    }

    #[test]
    fn test_limit_results_under_limit() {
        let symbols = vec![
            json!({"name": "Symbol1", "kind": 5}),
            json!({"name": "Symbol2", "kind": 12}),
        ];

        let limited = SymbolUtilities::limit_results(symbols, Some(10));
        assert_eq!(limited.len(), 2); // Should return all available symbols
    }

    #[test]
    fn test_limit_results_empty() {
        let symbols = vec![];
        let limited = SymbolUtilities::limit_results(symbols, Some(5));
        assert_eq!(limited.len(), 0);
    }

    #[test]
    fn test_limit_results_large_limit() {
        let symbols = vec![
            json!({"name": "Symbol1", "kind": 5}),
            json!({"name": "Symbol2", "kind": 12}),
        ];

        let limited = SymbolUtilities::limit_results(symbols, Some(1000));
        assert_eq!(limited.len(), 2); // Should not exceed available symbols
    }

    #[test]
    fn test_limit_results_preserves_order() {
        let symbols = vec![
            json!({"name": "Alpha", "kind": 5}),
            json!({"name": "Beta", "kind": 12}),
            json!({"name": "Gamma", "kind": 13}),
            json!({"name": "Delta", "kind": 5}),
        ];

        let limited = SymbolUtilities::limit_results(symbols, Some(2));
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0]["name"], "Alpha");
        assert_eq!(limited[1]["name"], "Beta");
    }

    #[test]
    fn test_limit_results_zero_limit() {
        let symbols = vec![
            json!({"name": "Symbol1", "kind": 5}),
            json!({"name": "Symbol2", "kind": 12}),
        ];

        let limited = SymbolUtilities::limit_results(symbols, Some(0));
        assert_eq!(limited.len(), 0); // Zero limit should return empty vector
    }
}
