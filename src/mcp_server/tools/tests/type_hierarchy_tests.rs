//! Integration tests for type hierarchy analysis functionality
//!
//! These tests verify that the type hierarchy analysis functionality works correctly
//! with real clangd integration, testing inheritance relationships including
//! interfaces, derived classes, and edge cases.

use crate::io::file_manager::RealFileBufferManager;
use crate::mcp_server::tools::analyze_symbols::{AnalyzeSymbolContextTool, AnalyzerResult};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_type_hierarchy_interface() {
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path);
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

    // Test IStorageBackend interface - should have derived classes
    let tool = AnalyzeSymbolContextTool {
        symbol: "IStorageBackend".to_string(),
        build_directory: None,
        max_examples: Some(2),
        location_hint: None,
    };

    let result = tool
        .call_tool(session, &workspace, file_buffer_manager)
        .await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
        rust_mcp_sdk::schema::TextContent { text, .. },
    )) = call_result.content.first()
    {
        text
    } else {
        panic!("Expected TextContent in call_result");
    };
    let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

    assert_eq!(analyzer_result.symbol.name, "IStorageBackend");
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::CLASS);

    // Check type hierarchy
    assert!(analyzer_result.type_hierarchy.is_some());
    let hierarchy = analyzer_result.type_hierarchy.unwrap();

    // IStorageBackend should have no supertypes (it's the base interface)
    assert_eq!(hierarchy.supertypes.len(), 0);

    // IStorageBackend should have MemoryStorage as subtype
    // Note: Only MemoryStorage is compiled by default (USE_MEMORY_STORAGE=ON in CMakeLists.txt)
    // FileStorage is conditionally excluded, so clangd won't see it
    assert_eq!(hierarchy.subtypes.len(), 1);
    assert_eq!(hierarchy.subtypes[0], "MemoryStorage");

    info!("IStorageBackend subtypes: {:?}", hierarchy.subtypes);
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_type_hierarchy_derived_class() {
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path);
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

    // Test MemoryStorage - should have IStorageBackend as supertype
    let tool = AnalyzeSymbolContextTool {
        symbol: "MemoryStorage".to_string(),
        build_directory: None,
        max_examples: Some(2),
        location_hint: None,
    };

    let result = tool
        .call_tool(session, &workspace, file_buffer_manager)
        .await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
        rust_mcp_sdk::schema::TextContent { text, .. },
    )) = call_result.content.first()
    {
        text
    } else {
        panic!("Expected TextContent in call_result");
    };
    let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

    assert_eq!(analyzer_result.symbol.name, "MemoryStorage");
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::CLASS);

    // Check type hierarchy
    assert!(analyzer_result.type_hierarchy.is_some());
    let hierarchy = analyzer_result.type_hierarchy.unwrap();

    // MemoryStorage should have IStorageBackend as supertype
    assert_eq!(hierarchy.supertypes.len(), 1);
    assert_eq!(hierarchy.supertypes[0], "IStorageBackend");

    // MemoryStorage should have no subtypes (it's a leaf class)
    assert_eq!(hierarchy.subtypes.len(), 0);

    info!("MemoryStorage supertypes: {:?}", hierarchy.supertypes);
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_type_hierarchy_non_class() {
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path);
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

    // Test a function - should have no type hierarchy
    let tool = AnalyzeSymbolContextTool {
        symbol: "factorial".to_string(),
        build_directory: None,
        max_examples: Some(2),
        location_hint: None,
    };

    let result = tool
        .call_tool(session, &workspace, file_buffer_manager)
        .await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
        rust_mcp_sdk::schema::TextContent { text, .. },
    )) = call_result.content.first()
    {
        text
    } else {
        panic!("Expected TextContent in call_result");
    };
    let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

    assert_eq!(analyzer_result.symbol.name, "factorial");
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::METHOD);

    // Check that type hierarchy is not present for functions
    assert!(analyzer_result.type_hierarchy.is_none());

    info!("factorial symbol kind: {:?}", analyzer_result.symbol.kind);
}
