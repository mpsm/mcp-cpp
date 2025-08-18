//! Integration tests for call hierarchy analysis functionality
//!
//! These tests verify that the call hierarchy analysis functionality works correctly
//! with real clangd integration, testing function and method call relationships including
//! incoming calls (callers) and outgoing calls (callees).

use crate::io::file_manager::RealFileBufferManager;
use crate::mcp_server::tools::analyze_symbols::{AnalyzeSymbolContextTool, AnalyzerResult};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_call_hierarchy_function() {
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

    // Test factorial function - should have callers from main.cpp
    let tool = AnalyzeSymbolContextTool {
        symbol: "factorial".to_string(),
        build_directory: None,
        max_examples: Some(2),
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

    // Check call hierarchy
    assert!(analyzer_result.call_hierarchy.is_some());
    let hierarchy = analyzer_result.call_hierarchy.unwrap();

    // factorial should have callers (from main.cpp)
    info!("factorial callers: {:?}", hierarchy.callers);
    info!("factorial callees: {:?}", hierarchy.callees);

    // factorial should have at least one caller (main function)
    assert!(!hierarchy.callers.is_empty());

    info!(
        "factorial call hierarchy - callers: {} callees: {}",
        hierarchy.callers.len(),
        hierarchy.callees.len()
    );
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_call_hierarchy_method() {
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

    // Test Math::Complex::add method - should have callers from main.cpp
    let tool = AnalyzeSymbolContextTool {
        symbol: "Math::Complex::add".to_string(), // Fully qualified name
        build_directory: None,
        max_examples: Some(2),
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

    assert_eq!(analyzer_result.symbol.name, "add"); // clangd returns just the method name
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::METHOD);

    // Check call hierarchy
    assert!(analyzer_result.call_hierarchy.is_some());
    let hierarchy = analyzer_result.call_hierarchy.unwrap();

    info!("add method callers: {:?}", hierarchy.callers);
    info!("add method callees: {:?}", hierarchy.callees);

    // add should have at least one caller (main function)
    assert!(!hierarchy.callers.is_empty());

    info!(
        "add method call hierarchy - callers: {} callees: {}",
        hierarchy.callers.len(),
        hierarchy.callees.len()
    );
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_call_hierarchy_non_function() {
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

    // Test a class - should have no call hierarchy
    let tool = AnalyzeSymbolContextTool {
        symbol: "Math".to_string(),
        build_directory: None,
        max_examples: Some(2),
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

    assert_eq!(analyzer_result.symbol.name, "Math");
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::CLASS);

    // Check that call hierarchy is not present for classes
    assert!(analyzer_result.call_hierarchy.is_none());

    // But type hierarchy should be present for classes
    assert!(analyzer_result.type_hierarchy.is_some());

    info!(
        "Math class - has type hierarchy: {}, has call hierarchy: {}",
        analyzer_result.type_hierarchy.is_some(),
        analyzer_result.call_hierarchy.is_some()
    );
}
