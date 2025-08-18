//! Integration tests for member analysis functionality
//!
//! These tests verify that the member extraction functionality works correctly
//! with real clangd integration, testing various C++ constructs including
//! classes, interfaces, and edge cases.

use crate::io::file_manager::RealFileBufferManager;
use crate::mcp_server::tools::analyze_symbols::{AnalyzeSymbolContextTool, AnalyzerResult};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_members_math() {
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

    // Test Math class - should have callable members
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

    // Check members
    assert!(analyzer_result.members.is_some());
    let members = analyzer_result.members.unwrap();

    info!("Math class members:");
    info!(
        "  Methods: {} (e.g., {:?})",
        members.methods.len(),
        members
            .methods
            .iter()
            .take(3)
            .map(|m| &m.name)
            .collect::<Vec<_>>()
    );
    info!("  Constructors: {}", members.constructors.len());
    info!("  Operators: {}", members.operators.len());
    info!("  Static methods: {}", members.static_methods.len());

    // Math should have methods like factorial, power, etc.
    assert!(!members.methods.is_empty());

    // Look for expected methods
    let method_names: Vec<&String> = members.methods.iter().map(|m| &m.name).collect();
    assert!(method_names.iter().any(|name| name.contains("factorial")));
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_members_interface() {
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

    // Test IStorageBackend interface - should have virtual methods
    let tool = AnalyzeSymbolContextTool {
        symbol: "IStorageBackend".to_string(),
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

    assert_eq!(analyzer_result.symbol.name, "IStorageBackend");
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::CLASS);

    // Check members
    assert!(analyzer_result.members.is_some());
    let members = analyzer_result.members.unwrap();

    info!("IStorageBackend interface members:");
    info!(
        "  Methods: {} (e.g., {:?})",
        members.methods.len(),
        members
            .methods
            .iter()
            .take(3)
            .map(|m| &m.name)
            .collect::<Vec<_>>()
    );
    info!("  Constructors: {}", members.constructors.len());
    info!("  Operators: {}", members.operators.len());
    info!("  Static methods: {}", members.static_methods.len());

    // IStorageBackend should have virtual methods like store, retrieve, remove
    assert!(!members.methods.is_empty());

    // Look for expected interface methods
    let method_names: Vec<&String> = members.methods.iter().map(|m| &m.name).collect();
    assert!(method_names.iter().any(|name| name.contains("store")
        || name.contains("retrieve")
        || name.contains("remove")));
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_members_non_class() {
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

    // Test a function - should have no members
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

    // Check that members are not present for functions
    assert!(analyzer_result.members.is_none());

    // But call hierarchy should be present for functions
    assert!(analyzer_result.call_hierarchy.is_some());

    info!(
        "factorial function - has members: {}, has call hierarchy: {}",
        analyzer_result.members.is_some(),
        analyzer_result.call_hierarchy.is_some()
    );
}
