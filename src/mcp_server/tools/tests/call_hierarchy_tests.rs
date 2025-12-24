//! Integration tests for call hierarchy analysis functionality
//!
//! These tests verify that the call hierarchy analysis functionality works correctly
//! with real clangd integration, testing function and method call relationships including
//! incoming calls (callers) and outgoing calls (callees).

use crate::mcp_server::tools::analyze_symbols::{AnalyzeSymbolContextTool, AnalyzerResult};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use rmcp::model::{RawContent, RawTextContent};
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Test factorial function - should have callers from main.cpp
    let tool = AnalyzeSymbolContextTool {
        symbol: "factorial".to_string(),
        build_directory: None,
        max_examples: Some(2),
        location_hint: None,
        wait_timeout: None,
        include_type_hierarchy: None,
        include_call_hierarchy: None,
        include_usage_patterns: None,
        include_members: None,
        include_code: None,
    };

    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    let result = tool.call_tool(component_session, &workspace).await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = match call_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in call_result"),
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
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_call_hierarchy_recursive() {
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

    // Test factorial function - should show recursive call to itself
    let tool = AnalyzeSymbolContextTool {
        symbol: "factorial".to_string(),
        build_directory: None,
        max_examples: Some(2),
        location_hint: None,
        wait_timeout: None,
        include_type_hierarchy: None,
        include_call_hierarchy: None,
        include_usage_patterns: None,
        include_members: None,
        include_code: None,
    };

    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    let result = tool.call_tool(component_session, &workspace).await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = match call_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in call_result"),
    };
    let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

    // Check that factorial has recursive call in its callees
    let hierarchy = analyzer_result
        .call_hierarchy
        .expect("Should have call hierarchy");
    info!("factorial callees: {:?}", hierarchy.callees);

    // factorial should call itself (recursive function)
    let recursive_call = hierarchy
        .callees
        .iter()
        .any(|callee| callee.contains("factorial"));
    assert!(
        recursive_call,
        "factorial should have a recursive call to itself"
    );
}
