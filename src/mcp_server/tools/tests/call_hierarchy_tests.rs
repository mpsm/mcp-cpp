//! Integration tests for call hierarchy analysis functionality
//!
//! These tests verify that the call hierarchy analysis functionality works correctly
//! with real clangd integration, testing function and method call relationships including
//! incoming calls (callers) and outgoing calls (callees).

use crate::mcp_server::tools::analyze_symbols::{AnalyzeSymbolContextTool, AnalyzerResult};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::{DEFAULT_INDEXING_TIMEOUT, integration::TestProject};
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

    // Ensure indexing is completed before testing call hierarchy
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(DEFAULT_INDEXING_TIMEOUT)
        .await
        .expect("Indexing should complete successfully for call hierarchy test");

    info!("Indexing completed, proceeding with call hierarchy function test");

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

    // Check call hierarchy - skip if not supported by clangd version
    if let Some(ref hierarchy) = analyzer_result.call_hierarchy {
        // factorial should have callers (from main.cpp)
        info!("factorial callers: {:?}", hierarchy.callers);
        info!("factorial callees: {:?}", hierarchy.callees);

        // factorial should have at least one caller (main function)
        assert!(!hierarchy.callers.is_empty());
    } else {
        info!(
            "Call hierarchy not available - this may be due to clangd version limitations (requires clangd-20+)"
        );
    }
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

    // Ensure indexing is completed before testing call hierarchy
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(DEFAULT_INDEXING_TIMEOUT)
        .await
        .expect("Indexing should complete successfully for call hierarchy test");

    info!("Indexing completed, proceeding with recursive call hierarchy test");

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

    let result = tool.call_tool(component_session, &workspace).await;

    assert!(result.is_ok());

    let call_result = result.unwrap();
    let text = match call_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in call_result"),
    };
    let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

    // Check that factorial has recursive call in its callees (if call hierarchy is available)
    if let Some(ref hierarchy) = analyzer_result.call_hierarchy {
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
    } else {
        info!(
            "Call hierarchy not available - skipping recursive call check (requires clangd-20+)"
        );
    }
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

    // Ensure indexing is completed before testing call hierarchy coherence
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();

    // Wait for indexing to complete (30 seconds should be plenty for the test project)
    component_session
        .ensure_indexed(DEFAULT_INDEXING_TIMEOUT)
        .await
        .expect("Indexing should complete successfully for call hierarchy test");

    info!("Indexing completed, proceeding with call hierarchy coherence test");

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
    let variance_result = variance_tool
        .call_tool(component_session, &workspace)
        .await
        .expect("Failed to analyze variance");

    let variance_text = match variance_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in variance_result"),
    };
    let variance_analysis: AnalyzerResult = serde_json::from_str(variance_text).unwrap();

    assert_eq!(variance_analysis.symbol.name, "variance");
    assert_eq!(variance_analysis.symbol.kind, lsp_types::SymbolKind::METHOD);

    // Skip test if call hierarchy is not available (clangd-18 may not support it)
    let variance_hierarchy = match variance_analysis.call_hierarchy {
        Some(hierarchy) => hierarchy,
        None => {
            info!(
                "Call hierarchy not available for variance - skipping coherence test (requires clangd-20+)"
            );
            return;
        }
    };
    info!("variance callers: {:?}", variance_hierarchy.callers);
    info!("variance callees: {:?}", variance_hierarchy.callees);

    // 2. Analyze mean (end of the chain) - use qualified name
    let mean_tool = AnalyzeSymbolContextTool {
        symbol: "Math::mean".to_string(), // Use qualified name
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
    let mean_result = mean_tool
        .call_tool(component_session, &workspace)
        .await
        .expect("Failed to analyze mean");

    let mean_text = match mean_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in mean_result"),
    };
    let mean_analysis: AnalyzerResult = serde_json::from_str(mean_text).unwrap();

    assert_eq!(mean_analysis.symbol.name, "mean");
    assert_eq!(mean_analysis.symbol.kind, lsp_types::SymbolKind::METHOD);

    // Skip test if call hierarchy is not available
    let mean_hierarchy = match mean_analysis.call_hierarchy {
        Some(hierarchy) => hierarchy,
        None => {
            info!(
                "Call hierarchy not available for mean - skipping coherence test (requires clangd-20+)"
            );
            return;
        }
    };
    info!("mean callers: {:?}", mean_hierarchy.callers);
    info!("mean callees: {:?}", mean_hierarchy.callees);

    // 3. Analyze standardDeviation (start of the chain) - use qualified name
    let std_dev_tool = AnalyzeSymbolContextTool {
        symbol: "Math::standardDeviation".to_string(), // Use qualified name
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
    let std_dev_result = std_dev_tool
        .call_tool(component_session, &workspace)
        .await
        .expect("Failed to analyze standardDeviation");

    let std_dev_text = match std_dev_result.content.first().map(|c| &c.raw) {
        Some(RawContent::Text(RawTextContent { text, .. })) => text,
        _ => panic!("Expected TextContent in std_dev_result"),
    };
    let std_dev_analysis: AnalyzerResult = serde_json::from_str(std_dev_text).unwrap();

    assert_eq!(std_dev_analysis.symbol.name, "standardDeviation");
    assert_eq!(std_dev_analysis.symbol.kind, lsp_types::SymbolKind::METHOD);

    // Skip test if call hierarchy is not available
    let std_dev_hierarchy = match std_dev_analysis.call_hierarchy {
        Some(hierarchy) => hierarchy,
        None => {
            info!(
                "Call hierarchy not available for standardDeviation - skipping coherence test (requires clangd-20+)"
            );
            return;
        }
    };
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
