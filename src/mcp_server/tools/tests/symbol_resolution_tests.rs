//! Integration tests for symbol resolution functionality
//!
//! These tests verify that the symbol resolution functionality works correctly
//! with real clangd integration, testing symbol discovery scenarios including
//! finding symbols, handling multiple matches, and no-match cases.

use crate::mcp_server::tools::lsp_helpers::symbol_resolution::get_matching_symbol;
use crate::project::{ProjectScanner, WorkspaceSession, index::IndexSession};
use crate::test_utils::integration::TestProject;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_symbol_resolution_single_match() {
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
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    // Ensure indexing completion using IndexSession
    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    index_session.ensure_indexed().await.unwrap();

    let mut locked_session = session.lock().await;

    // Test finding a unique symbol
    let result = get_matching_symbol("Math", &mut locked_session).await;

    assert!(result.is_ok());
    let symbol = result.unwrap();
    assert_eq!(symbol.name, "Math");
    assert_eq!(symbol.kind, lsp_types::SymbolKind::CLASS);

    info!("Found symbol: {} (kind: {:?})", symbol.name, symbol.kind);
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_symbol_resolution_function() {
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
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    // Ensure indexing completion using IndexSession
    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    index_session.ensure_indexed().await.unwrap();

    let mut locked_session = session.lock().await;

    // Test finding a function symbol
    let result = get_matching_symbol("factorial", &mut locked_session).await;

    assert!(result.is_ok());
    let symbol = result.unwrap();
    assert_eq!(symbol.name, "factorial");
    assert_eq!(symbol.kind, lsp_types::SymbolKind::METHOD);

    info!("Found function: {} (kind: {:?})", symbol.name, symbol.kind);
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_symbol_resolution_no_match() {
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
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    // Ensure indexing completion using IndexSession
    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    index_session.ensure_indexed().await.unwrap();

    let mut locked_session = session.lock().await;

    // Test searching for a non-existent symbol
    let result = get_matching_symbol("NonExistentSymbol", &mut locked_session).await;

    assert!(result.is_err());
    match result {
        Err(crate::mcp_server::tools::analyze_symbols::AnalyzerError::NoSymbols(symbol)) => {
            assert_eq!(symbol, "NonExistentSymbol");
            info!("Correctly detected no symbols found for '{}'", symbol);
        }
        _ => panic!("Expected NoSymbols error"),
    }
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_symbol_resolution_qualified_name() {
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
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    // Ensure indexing completion using IndexSession
    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    index_session.ensure_indexed().await.unwrap();

    let mut locked_session = session.lock().await;

    // Test finding a qualified symbol name
    let result = get_matching_symbol("Math::Complex::add", &mut locked_session).await;

    assert!(result.is_ok());
    let symbol = result.unwrap();
    // clangd typically returns just the method name for qualified searches
    assert_eq!(symbol.name, "add");
    assert_eq!(symbol.kind, lsp_types::SymbolKind::METHOD);

    info!(
        "Found qualified symbol: {} (kind: {:?})",
        symbol.name, symbol.kind
    );
}
