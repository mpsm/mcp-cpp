//! Integration tests for member analysis functionality
//!
//! These tests verify that the member extraction functionality works correctly
//! with real clangd integration, testing various C++ constructs including
//! classes, interfaces, and edge cases.

use crate::mcp_server::tools::analyze_symbols::{AnalyzeSymbolContextTool, AnalyzerResult};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::{DEFAULT_INDEXING_TIMEOUT, integration::TestProject};
use rmcp::model::{RawContent, RawTextContent};
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Ensure indexing is completed before testing
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(DEFAULT_INDEXING_TIMEOUT)
        .await
        .expect("Indexing should complete successfully for members test");

    info!("Indexing completed, proceeding with members math test");

    // Test Math class - should have callable members
    let tool = AnalyzeSymbolContextTool {
        symbol: "Math".to_string(),
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

    let result = tool.call_tool(component_session, &workspace).await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = match call_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in call_result"),
    };
    let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

    assert_eq!(analyzer_result.symbol.name, "Math");
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::CLASS);

    // Check members
    assert!(analyzer_result.members.is_some());
    let members = analyzer_result.members.unwrap();

    info!("Math class members analysis:");
    info!(
        "  Total: {} methods, {} constructors, {} destructors, {} operators",
        members.methods.len(),
        members.constructors.len(),
        members.destructors.len(),
        members.operators.len()
    );

    info!("  Methods ({}):", members.methods.len());
    for (i, method) in members.methods.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            method.name,
            method.member_type,
            method.signature
        );
    }

    info!("  Constructors ({}):", members.constructors.len());
    for (i, constructor) in members.constructors.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            constructor.name,
            constructor.member_type,
            constructor.signature
        );
    }

    info!("  Destructors ({}):", members.destructors.len());
    for (i, destructor) in members.destructors.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            destructor.name,
            destructor.member_type,
            destructor.signature
        );
    }

    info!("  Operators ({}):", members.operators.len());
    for (i, operator) in members.operators.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            operator.name,
            operator.member_type,
            operator.signature
        );
    }

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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Ensure indexing is completed before testing
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(DEFAULT_INDEXING_TIMEOUT)
        .await
        .expect("Indexing should complete successfully for members test");

    info!("Indexing completed, proceeding with members interface test");

    // Test IStorageBackend interface - should have virtual methods
    let tool = AnalyzeSymbolContextTool {
        symbol: "IStorageBackend".to_string(),
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

    let result = tool.call_tool(component_session, &workspace).await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = match call_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in call_result"),
    };
    let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

    assert_eq!(analyzer_result.symbol.name, "IStorageBackend");
    assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::CLASS);

    // Check members
    assert!(analyzer_result.members.is_some());
    let members = analyzer_result.members.unwrap();

    info!("IStorageBackend interface members analysis:");
    info!(
        "  Total: {} methods, {} constructors, {} destructors, {} operators",
        members.methods.len(),
        members.constructors.len(),
        members.destructors.len(),
        members.operators.len()
    );

    info!("  Methods ({}):", members.methods.len());
    for (i, method) in members.methods.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            method.name,
            method.member_type,
            method.signature
        );
    }

    info!("  Constructors ({}):", members.constructors.len());
    for (i, constructor) in members.constructors.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            constructor.name,
            constructor.member_type,
            constructor.signature
        );
    }

    info!("  Destructors ({}):", members.destructors.len());
    for (i, destructor) in members.destructors.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            destructor.name,
            destructor.member_type,
            destructor.signature
        );
    }

    info!("  Operators ({}):", members.operators.len());
    for (i, operator) in members.operators.iter().enumerate() {
        info!(
            "    {}: {} (type: {}, signature: {})",
            i + 1,
            operator.name,
            operator.member_type,
            operator.signature
        );
    }

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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Ensure indexing is completed before testing
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(DEFAULT_INDEXING_TIMEOUT)
        .await
        .expect("Indexing should complete successfully for members test");

    info!("Indexing completed, proceeding with members non-class test");

    // Test a function - should have no members
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

    // Check that members are not present for functions
    assert!(analyzer_result.members.is_none());

    // Call hierarchy should be present for functions (if supported by clangd version)
    if let Some(ref hierarchy) = analyzer_result.call_hierarchy {
        info!(
            "factorial function - has call hierarchy with {} callers, {} callees",
            hierarchy.callers.len(),
            hierarchy.callees.len()
        );
    } else {
        info!(
            "Call hierarchy not available - this may be due to clangd version limitations (requires clangd-20+)"
        );
    }
}
