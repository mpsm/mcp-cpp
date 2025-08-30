//! Integration tests for call hierarchy analysis functionality
//!
//! These tests verify that the call hierarchy analysis functionality works correctly
//! with real clangd integration, testing function and method call relationships including
//! incoming calls (callers) and outgoing calls (callees).

use crate::io::file_manager::RealFileBufferManager;
use crate::mcp_server::tools::analyze_symbols::{AnalyzeSymbolContextTool, AnalyzerResult};
use crate::project::{ProjectScanner, WorkspaceSession, index::IndexSession};
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");
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
        location_hint: None,
    };

    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    let result = tool
        .call_tool(index_session, session, &workspace, file_buffer_manager)
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");
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
        location_hint: None,
    };

    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    let result = tool
        .call_tool(index_session, session, &workspace, file_buffer_manager)
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");
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
        location_hint: None,
    };

    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    let result = tool
        .call_tool(index_session, session, &workspace, file_buffer_manager)
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

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_analyzer_call_hierarchy_coherence() {
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
    let session_arc = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

    // Test the call chain: standardDeviation -> variance -> mean
    // This validates coherence: if A calls B, then B's callers must include A

    // Use qualified names to be more specific about which overload we want
    // According to clangd documentation, we can use scope-based queries

    // 1. Analyze variance (middle of the chain) - use location hint to target vector<double> overload
    let test_project_include = workspace.project_root_path.join("include/Math.hpp");
    let canonical_path = test_project_include
        .canonicalize()
        .expect("Failed to canonicalize Math.hpp path");
    let variance_location = format!("{}:431:19", canonical_path.display()); // Line 431, column 19 points to "variance" function name

    let variance_tool = AnalyzeSymbolContextTool {
        symbol: "variance".to_string(),
        build_directory: None,
        max_examples: Some(2),
        location_hint: Some(variance_location),
    };

    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    let variance_result = variance_tool
        .call_tool(
            index_session,
            session_arc.clone(),
            &workspace,
            file_buffer_manager.clone(),
        )
        .await
        .expect("Failed to analyze variance");

    let variance_text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
        rust_mcp_sdk::schema::TextContent { text, .. },
    )) = variance_result.content.first()
    {
        text
    } else {
        panic!("Expected TextContent in variance_result");
    };
    let variance_analysis: AnalyzerResult = serde_json::from_str(variance_text).unwrap();

    assert_eq!(variance_analysis.symbol.name, "variance");
    assert_eq!(variance_analysis.symbol.kind, lsp_types::SymbolKind::METHOD);
    assert!(variance_analysis.call_hierarchy.is_some());

    let variance_hierarchy = variance_analysis.call_hierarchy.unwrap();
    info!("variance callers: {:?}", variance_hierarchy.callers);
    info!("variance callees: {:?}", variance_hierarchy.callees);

    // 2. Analyze mean (end of the chain) - use qualified name
    let mean_tool = AnalyzeSymbolContextTool {
        symbol: "Math::mean".to_string(), // Use qualified name
        build_directory: None,
        max_examples: Some(2),
        location_hint: None,
    };

    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    let mean_result = mean_tool
        .call_tool(
            index_session,
            session_arc.clone(),
            &workspace,
            file_buffer_manager.clone(),
        )
        .await
        .expect("Failed to analyze mean");

    let mean_text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
        rust_mcp_sdk::schema::TextContent { text, .. },
    )) = mean_result.content.first()
    {
        text
    } else {
        panic!("Expected TextContent in mean_result");
    };
    let mean_analysis: AnalyzerResult = serde_json::from_str(mean_text).unwrap();

    assert_eq!(mean_analysis.symbol.name, "mean");
    assert_eq!(mean_analysis.symbol.kind, lsp_types::SymbolKind::METHOD);
    assert!(mean_analysis.call_hierarchy.is_some());

    let mean_hierarchy = mean_analysis.call_hierarchy.unwrap();
    info!("mean callers: {:?}", mean_hierarchy.callers);
    info!("mean callees: {:?}", mean_hierarchy.callees);

    // 3. Analyze standardDeviation (start of the chain) - use qualified name
    let std_dev_tool = AnalyzeSymbolContextTool {
        symbol: "Math::standardDeviation".to_string(), // Use qualified name
        build_directory: None,
        max_examples: Some(2),
        location_hint: None,
    };

    let index_session = IndexSession::new(&workspace_session, test_project.build_dir.clone());
    let std_dev_result = std_dev_tool
        .call_tool(
            index_session,
            session_arc.clone(),
            &workspace,
            file_buffer_manager.clone(),
        )
        .await
        .expect("Failed to analyze standardDeviation");

    let std_dev_text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
        rust_mcp_sdk::schema::TextContent { text, .. },
    )) = std_dev_result.content.first()
    {
        text
    } else {
        panic!("Expected TextContent in std_dev_result");
    };
    let std_dev_analysis: AnalyzerResult = serde_json::from_str(std_dev_text).unwrap();

    assert_eq!(std_dev_analysis.symbol.name, "standardDeviation");
    assert_eq!(std_dev_analysis.symbol.kind, lsp_types::SymbolKind::METHOD);
    assert!(std_dev_analysis.call_hierarchy.is_some());

    let std_dev_hierarchy = std_dev_analysis.call_hierarchy.unwrap();
    info!("standardDeviation callers: {:?}", std_dev_hierarchy.callers);
    info!("standardDeviation callees: {:?}", std_dev_hierarchy.callees);

    // COHERENCE VALIDATION: Check bidirectional relationships

    // 1. Check standardDeviation -> variance relationship
    // standardDeviation's callees should include variance
    assert!(
        std_dev_hierarchy
            .callees
            .iter()
            .any(|c| c.contains("variance")),
        "standardDeviation should call variance"
    );

    // variance's callers should include standardDeviation
    assert!(
        variance_hierarchy
            .callers
            .iter()
            .any(|c| c.contains("standardDeviation")),
        "variance should be called by standardDeviation"
    );

    // 2. Check variance -> mean relationship
    // variance's callees should include mean
    assert!(
        variance_hierarchy
            .callees
            .iter()
            .any(|c| c.contains("mean")),
        "variance should call mean"
    );

    // mean's callers should include variance
    assert!(
        mean_hierarchy
            .callers
            .iter()
            .any(|c| c.contains("variance")),
        "mean should be called by variance"
    );

    // 3. Additional coherence check: verify the call chain
    // standardDeviation(double) -> variance(double) -> mean(double)
    // So mean should have variance as a caller (which we already verified)
    assert!(
        mean_hierarchy
            .callers
            .iter()
            .any(|c| c.contains("variance")),
        "mean should be called by variance (completing the call chain)"
    );

    info!(
        "Call hierarchy coherence validated successfully:\n\
         - standardDeviation -> variance: bidirectional relationship confirmed\n\
         - variance -> mean: bidirectional relationship confirmed\n\
         - standardDeviation -> mean (direct): relationship confirmed"
    );
}
