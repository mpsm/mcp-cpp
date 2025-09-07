//! Integration tests for document symbols extraction functionality
//!
//! These tests work with real clangd sessions to validate hierarchical symbol
//! extraction and comprehensive symbol analysis with source content extraction.

use crate::project::{ComponentSession, ProjectScanner, WorkspaceSession};
use crate::test_utils::{DEFAULT_INDEXING_TIMEOUT, integration::TestProject};
use std::sync::Arc;

/// Helper to create a test project with ComponentSession for document symbols tests  
async fn create_test_component_session() -> (TestProject, Arc<ComponentSession>) {
    // Create a test project first
    let test_project = TestProject::new().await.unwrap();
    test_project.cmake_configure().await.unwrap();

    // Scan the test project to create a proper workspace with components
    let scanner = ProjectScanner::with_default_providers();
    let workspace = scanner
        .scan_project(&test_project.project_root, 3, None)
        .expect("Failed to scan test project");

    // Create a WorkspaceSession with test clangd path
    let clangd_path = crate::test_utils::get_test_clangd_path();
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Ensure indexing completion using ComponentSession
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(DEFAULT_INDEXING_TIMEOUT)
        .await
        .unwrap();

    (test_project, component_session)
}
use crate::io::file_buffer::FilePosition;
use crate::io::file_manager::FileBufferManager;
use crate::io::file_system::RealFileSystem;
use crate::mcp_server::tools::lsp_helpers::document_symbols::{
    SymbolSearchBuilder, count_symbols_by_kind, count_total_symbols, find_symbols_by_kind,
    get_document_symbols, get_symbol_paths,
};
use lsp_types::{DocumentSymbol, SymbolKind};
use std::path::Path;
use tracing::debug;

#[cfg(feature = "test-logging")]
#[ctor::ctor]
fn init_test_logging() {
    crate::test_utils::logging::init();
}

/// Test finding a specific symbol by position in the Math.hpp file
#[tokio::test]
async fn test_find_specific_symbol_in_document() {
    let (test_project, component_session) = create_test_component_session().await;

    // Use the Math.hpp file which has rich nested structure
    let math_header = test_project.project_root.join("include/Math.hpp");
    assert!(
        math_header.exists(),
        "Math.hpp should exist in test project"
    );

    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    // Get document symbols for Math.hpp
    let symbols = get_document_symbols(&component_session, file_uri.clone())
        .await
        .unwrap();

    // Should have at least the TestProject namespace and Math class
    assert!(!symbols.is_empty(), "Should find symbols in Math.hpp");

    // Find the Math class specifically
    let math_class = SymbolSearchBuilder::new()
        .with_name("Math")
        .with_kind(SymbolKind::CLASS)
        .find_first(&symbols);

    assert!(math_class.is_some(), "Should find Math class");
    let math_class = math_class.unwrap();
    assert_eq!(math_class.name, "Math");
    assert_eq!(math_class.kind, SymbolKind::CLASS);

    // Math class should have children (nested classes and methods)
    assert!(
        math_class.children.is_some(),
        "Math class should have children"
    );
    let children = math_class.children.as_ref().unwrap();
    assert!(
        !children.is_empty(),
        "Math class should have nested symbols"
    );

    // Find the Statistics nested class
    let statistics_class = SymbolSearchBuilder::new()
        .with_name("Statistics")
        .with_kind(SymbolKind::CLASS)
        .find_first(&symbols);

    assert!(
        statistics_class.is_some(),
        "Should find Statistics nested class"
    );
}

/// Test listing all methods for the Math class
#[tokio::test]
async fn test_list_all_methods_for_class() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    let symbols = get_document_symbols(&component_session, file_uri)
        .await
        .unwrap();

    // Find all methods (should include static methods like factorial, gcd, mean, etc.)
    let methods = find_symbols_by_kind(&symbols, SymbolKind::METHOD);

    // Math.hpp has many methods - should find a significant number
    assert!(
        methods.len() >= 10,
        "Should find many methods in Math class, found: {}",
        methods.len()
    );

    // Check for some specific expected methods
    let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();

    // These are static methods that should definitely exist in Math class
    let expected_methods = ["factorial", "gcd", "mean", "standardDeviation", "isPrime"];
    for expected in &expected_methods {
        assert!(
            method_names.contains(expected),
            "Should find method '{}' in Math class. Found methods: {:?}",
            expected,
            method_names
        );
    }

    // Test with builder pattern to find methods in Math class specifically
    let math_methods = SymbolSearchBuilder::new()
        .with_kind(SymbolKind::METHOD)
        .path_contains("Math")
        .find_all(&symbols);

    assert!(
        !math_methods.is_empty(),
        "Should find methods in Math namespace/class"
    );
}

/// Test nested class symbol traversal (Math::Statistics::Distribution)
#[tokio::test]
async fn test_nested_class_symbol_traversal() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    let symbols = get_document_symbols(&component_session, file_uri)
        .await
        .unwrap();

    // Get all symbol paths to understand the structure
    let paths = get_symbol_paths(&symbols);

    // Should have nested structures like TestProject::Math::Statistics
    let nested_paths: Vec<String> = paths
        .iter()
        .map(|(name, path)| {
            if path.is_empty() {
                name.clone()
            } else {
                format!("{}::{}", path, name)
            }
        })
        .collect();

    // Check for deeply nested symbols
    assert!(
        nested_paths
            .iter()
            .any(|path| path.contains("Math") && path.contains("Statistics")),
        "Should find nested Statistics class in Math. Found paths: {:?}",
        nested_paths
    );

    // Test path-based searching
    let stats_symbols = SymbolSearchBuilder::new()
        .path_contains("Statistics")
        .find_all(&symbols);

    assert!(
        !stats_symbols.is_empty(),
        "Should find symbols in Statistics namespace/class"
    );

    // Count total symbols in the hierarchy
    let total_count = count_total_symbols(&symbols);
    assert!(
        total_count >= 50,
        "Should find many symbols in Math.hpp (nested classes, methods, etc.), found: {}",
        total_count
    );
}

/// Test template class symbol detection (Math::Matrix) with comprehensive inspection
#[tokio::test]
async fn test_template_class_symbol_detection() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    let symbols = get_document_symbols(&component_session, file_uri)
        .await
        .unwrap();

    // Create FileBufferManager for proper source content extraction
    let mut file_manager = FileBufferManager::new(RealFileSystem);

    // Find template class Matrix
    let matrix_class = SymbolSearchBuilder::new()
        .with_name("Matrix")
        .with_kind(SymbolKind::CLASS)
        .find_first(&symbols);

    assert!(matrix_class.is_some(), "Should find Matrix template class");
    let matrix = matrix_class.unwrap();

    debug!("🚀 MATRIX CLASS ANALYSIS 🚀");

    // Comprehensive inspection of Matrix class using proper class system
    inspect_symbol_with_content(matrix, &math_header, &mut file_manager, 0)
        .expect("Should successfully inspect Matrix class");

    // Matrix should have template methods and nested Iterator class
    if let Some(children) = &matrix.children {
        debug!("Matrix has {} children", children.len());

        // Should have methods like operator+, operator*, etc.
        let matrix_methods: Vec<_> = children
            .iter()
            .filter(|child| child.kind == SymbolKind::METHOD)
            .collect();

        debug!(
            "Matrix methods found: {:?}",
            matrix_methods.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
        assert!(
            !matrix_methods.is_empty(),
            "Matrix class should have methods, found: {}",
            matrix_methods.len()
        );

        // Should have nested Iterator class
        let iterator_class = children
            .iter()
            .find(|child| child.name == "Iterator" && child.kind == SymbolKind::CLASS);

        assert!(
            iterator_class.is_some(),
            "Matrix should have nested Iterator class. Found children: {:?}",
            children
                .iter()
                .map(|c| (&c.name, c.kind))
                .collect::<Vec<_>>()
        );

        // If Iterator exists, inspect it too
        if let Some(iterator) = iterator_class {
            debug!("🔍 DETAILED ITERATOR CLASS ANALYSIS:");
            inspect_symbol_with_content(iterator, &math_header, &mut file_manager, 1)
                .expect("Should successfully inspect Iterator class");
        }

        // Look for specific expected methods
        let expected_methods = ["operator+", "operator-", "operator*", "fill"];
        for expected in &expected_methods {
            let found = children
                .iter()
                .any(|child| child.kind == SymbolKind::METHOD && child.name.contains(expected));
            if !found {
                debug!(
                    "⚠️  Expected method '{}' not found. Available methods: {:?}",
                    expected,
                    matrix_methods.iter().map(|m| &m.name).collect::<Vec<_>>()
                );
            }
        }
    }

    // Test counting symbols by kind
    let class_count = count_symbols_by_kind(&symbols, SymbolKind::CLASS);
    assert!(
        class_count >= 5,
        "Should find multiple classes (Math, Statistics, Distribution, Matrix, Iterator, etc.), found: {}",
        class_count
    );

    // Additional assertions on Matrix class structure
    assert!(
        matrix.detail.is_some(),
        "Matrix class should have detail information"
    );

    // Range validation - selection should be within range
    assert!(
        matrix.selection_range.start.line >= matrix.range.start.line
            && matrix.selection_range.end.line <= matrix.range.end.line,
        "Selection range should be within the full range"
    );

    // Matrix class should span multiple lines (it's a complex template)
    let matrix_lines = matrix.range.end.line - matrix.range.start.line;
    assert!(
        matrix_lines > 10,
        "Matrix class should span many lines (it's a complex template), found: {} lines",
        matrix_lines
    );
}

/// Test hierarchical response handling and error cases
#[tokio::test]
async fn test_hierarchical_vs_flat_response_handling() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    // This should always succeed due to our hierarchicalDocumentSymbolSupport: true
    let symbols = get_document_symbols(&component_session, file_uri).await;

    assert!(
        symbols.is_ok(),
        "get_document_symbols should succeed with hierarchical support"
    );
    let symbols = symbols.unwrap();

    // Verify we get hierarchical structure (symbols with children)
    let has_nested_symbols = symbols.iter().any(|symbol| {
        symbol
            .children
            .as_ref()
            .is_some_and(|children| !children.is_empty())
    });

    assert!(
        has_nested_symbols,
        "Should receive hierarchical symbols with nested children"
    );

    // Test with non-existent file - should get appropriate error
    let bad_file_uri_str = format!(
        "file://{}",
        test_project.project_root.join("nonexistent.hpp").display()
    );
    let bad_file_uri: lsp_types::Uri = bad_file_uri_str.parse().unwrap();
    let bad_result = get_document_symbols(&component_session, bad_file_uri).await;

    // Should get an error for non-existent file
    assert!(
        bad_result.is_err(),
        "Should get error for non-existent file"
    );
}

// ============================================================================
// Symbol Inspection Utilities
// ============================================================================

/// Comprehensive symbol inspection with file content extraction using proper class system
fn inspect_symbol_with_content(
    symbol: &DocumentSymbol,
    file_path: &Path,
    file_manager: &mut FileBufferManager<RealFileSystem>,
    indent_level: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let indent = "  ".repeat(indent_level);

    debug!("{}🔍 SYMBOL: {} ({:?})", indent, symbol.name, symbol.kind);

    // Show ranges
    debug!(
        "{}  📍 Range: {}:{}-{}:{} ({} lines)",
        indent,
        symbol.range.start.line + 1,
        symbol.range.start.character,
        symbol.range.end.line + 1,
        symbol.range.end.character,
        symbol.range.end.line - symbol.range.start.line + 1
    );

    debug!(
        "{}  📍 Selection: {}:{}-{}:{}",
        indent,
        symbol.selection_range.start.line + 1,
        symbol.selection_range.start.character,
        symbol.selection_range.end.line + 1,
        symbol.selection_range.end.character
    );

    // Show detail if present
    if let Some(ref detail) = symbol.detail {
        debug!("{}  📝 Detail: {}", indent, detail);
    }

    // Extract source text using proper io:: crate FileBufferManager and FileBuffer
    match file_manager.get_buffer(file_path) {
        Ok(file_buffer) => {
            // Convert LSP positions to FilePosition
            let start_pos = FilePosition::new(
                symbol.selection_range.start.line,
                symbol.selection_range.start.character,
            );
            let end_pos = FilePosition::new(
                symbol.selection_range.end.line,
                symbol.selection_range.end.character,
            );

            // Extract text using FileBuffer's text_between method
            match file_buffer.text_between(start_pos, end_pos) {
                Ok(selection_content) => {
                    debug!(
                        "{}  📄 Selection Source: {:?}",
                        indent,
                        selection_content.trim()
                    );
                }
                Err(e) => {
                    debug!("{}  ❌ Failed to extract selection content: {}", indent, e);
                }
            }
        }
        Err(e) => {
            debug!("{}  ❌ Failed to get file buffer: {}", indent, e);
        }
    }

    // Show children info
    if let Some(children) = &symbol.children {
        debug!(
            "{}  👥 Children: {} [{}]",
            indent,
            children.len(),
            children
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Recursively inspect first few children
        for (idx, child) in children.iter().enumerate() {
            if idx < 5 {
                // Limit to first 5 children for readability
                inspect_symbol_with_content(child, file_path, file_manager, indent_level + 1)?;
            } else if idx == 5 {
                debug!("{}    ... and {} more children", indent, children.len() - 5);
                break;
            }
        }
    }

    Ok(())
}

// ============================================================================
// Fuzzy Matching Integration Tests
// ============================================================================

/// Test fuzzy matching with real clangd symbols
#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_fuzzy_matching_integration_basic() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    let symbols = get_document_symbols(&component_session, file_uri)
        .await
        .unwrap();

    // First, let's see what symbols we actually have
    let all_symbols: Vec<_> = symbols.iter().map(|s| &s.name).collect();
    debug!("All top-level symbols found: {:?}", all_symbols);

    // Test exact match first to establish baseline
    let found = SymbolSearchBuilder::new()
        .with_name("Math")
        .find_all(&symbols);
    debug!(
        "Exact 'Math' search found {} symbols: {:?}",
        found.len(),
        found.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // Test fuzzy match for "factorial" function with a simpler typo
    let found = SymbolSearchBuilder::new()
        .with_name("factorial") // First try exact match
        .find_all(&symbols);
    debug!("Exact 'factorial' search found {} symbols", found.len());

    if found.is_empty() {
        // If we don't find factorial exactly, try a fuzzy variant
        let found = SymbolSearchBuilder::new()
            .with_name("fact") // Partial match
            .find_all(&symbols);
        debug!(
            "Fuzzy 'fact' search found {} symbols: {:?}",
            found.len(),
            found.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(!found.is_empty(), "Should find symbols matching 'fact'");
    } else {
        // Try fuzzy variant
        let found = SymbolSearchBuilder::new()
            .with_name("factrl") // Missing letters should still match
            .find_all(&symbols);
        debug!(
            "Fuzzy 'factrl' search found {} symbols: {:?}",
            found.len(),
            found.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            !found.is_empty(),
            "Fuzzy search 'factrl' should match 'factorial'"
        );
        assert!(
            found.iter().any(|s| s.name.contains("factorial")),
            "Should find factorial function with fuzzy match"
        );
    }

    // Test fuzzy match for Math class with debug output
    let found = SymbolSearchBuilder::new()
        .with_name("Mat") // Simpler fuzzy match
        .find_all(&symbols);
    debug!(
        "Fuzzy 'Mat' search found {} symbols: {:?}",
        found.len(),
        found.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // This should definitely find Math and Matrix symbols
    assert!(!found.is_empty(), "Fuzzy search 'Mat' should find matches");
    assert!(
        found.iter().any(|s| s.name == "Math"),
        "Fuzzy search 'Mat' should match 'Math' class"
    );
    assert!(
        found.iter().any(|s| s.name.contains("Matrix")),
        "Fuzzy search 'Mat' should also match 'Matrix' symbols"
    );
}

/// Test fuzzy matching with scoring and ordering
#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_fuzzy_matching_integration_scoring() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    let symbols = get_document_symbols(&component_session, file_uri)
        .await
        .unwrap();

    // Search for "stat" which should match "Statistics" class
    let found = SymbolSearchBuilder::new()
        .with_name("stat")
        .find_all(&symbols);

    assert!(!found.is_empty(), "Should find matches for 'stat'");

    // Best match should be "Statistics" (if present)
    if let Some(first_match) = found.first() {
        debug!(
            "Best match for 'stat': {} (kind: {:?})",
            first_match.name, first_match.kind
        );
        // The fuzzy matching should prioritize better matches
        assert!(
            first_match.name.to_lowercase().contains("stat") || first_match.name == "Statistics",
            "Best fuzzy match should contain 'stat' or be 'Statistics', got: {}",
            first_match.name
        );
    }
}

/// Test fuzzy matching with kind filtering
#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_fuzzy_matching_integration_with_kind_filter() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    let symbols = get_document_symbols(&component_session, file_uri)
        .await
        .unwrap();

    // Search for methods with fuzzy matching
    let found = SymbolSearchBuilder::new()
        .with_name("mean") // Should match "mean" method
        .with_kind(SymbolKind::METHOD)
        .find_all(&symbols);

    assert!(!found.is_empty(), "Should find method matches for 'mean'");

    // All results should be methods
    for symbol in &found {
        assert_eq!(
            symbol.kind,
            SymbolKind::METHOD,
            "All fuzzy matches should be methods, got: {} (kind: {:?})",
            symbol.name,
            symbol.kind
        );
    }

    // Should include the "mean" method if it exists
    assert!(
        found.iter().any(|s| s.name.contains("mean")),
        "Should find a method containing 'mean'"
    );
}

/// Test fuzzy matching performance with large symbol sets
#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_fuzzy_matching_integration_performance() {
    let (test_project, component_session) = create_test_component_session().await;

    let math_header = test_project.project_root.join("include/Math.hpp");
    let file_uri_str = format!("file://{}", math_header.display());
    let file_uri: lsp_types::Uri = file_uri_str.parse().unwrap();

    let symbols = get_document_symbols(&component_session, file_uri)
        .await
        .unwrap();

    // Ensure we have a reasonable number of symbols to test performance
    let total_symbols = count_total_symbols(&symbols);
    assert!(
        total_symbols >= 30,
        "Should have enough symbols for performance testing, got: {}",
        total_symbols
    );

    let start = std::time::Instant::now();

    // Perform fuzzy search that should match multiple results
    let found = SymbolSearchBuilder::new()
        .with_name("ma") // Should match many symbols (Math, Matrix, etc.)
        .find_all(&symbols);

    let duration = start.elapsed();

    // Should complete quickly even with many symbols
    assert!(
        duration.as_millis() < 100,
        "Fuzzy matching should be fast, took: {:?}",
        duration
    );

    debug!(
        "Fuzzy search for 'ma' found {} matches in {:?}",
        found.len(),
        duration
    );

    // Verify results are properly sorted (if we have multiple matches)
    if found.len() > 1 {
        // Check that exact matches come first when possible
        let exact_matches: Vec<_> = found
            .iter()
            .filter(|s| s.name.to_lowercase().starts_with("ma"))
            .collect();

        if !exact_matches.is_empty() {
            debug!(
                "Found {} exact/prefix matches out of {} total",
                exact_matches.len(),
                found.len()
            );
        }
    }
}
