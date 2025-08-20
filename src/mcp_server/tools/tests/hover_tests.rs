//! Integration tests for hover documentation analysis functionality
//!
//! These tests verify that the hover documentation analysis functionality works
//! correctly with real clangd integration, testing documentation extraction
//! scenarios including markdown processing, type information, and edge cases.

use crate::mcp_server::tools::lsp_helpers::{
    hover::get_hover_info, symbol_resolution::get_matching_symbol,
};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_hover_info_class_symbol() {
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

    let mut locked_session = session.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get Math class symbol
    let symbol = get_matching_symbol("Math", &mut locked_session)
        .await
        .expect("Failed to find Math symbol");
    let symbol_location = &symbol.location;

    // Test getting hover information
    let hover_info = get_hover_info(symbol_location, &mut locked_session)
        .await
        .expect("Failed to get hover info");

    assert!(!hover_info.is_empty());
    info!("Hover info for Math class: {}", hover_info);

    // Hover should contain class information
    assert!(hover_info.contains("Math") || hover_info.contains("class"));
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_hover_info_function_symbol() {
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

    let mut locked_session = session.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get factorial function symbol
    let symbol = get_matching_symbol("factorial", &mut locked_session)
        .await
        .expect("Failed to find factorial symbol");
    let symbol_location = &symbol.location;

    // Test getting hover information
    let hover_info = get_hover_info(symbol_location, &mut locked_session)
        .await
        .expect("Failed to get hover info");

    assert!(!hover_info.is_empty());
    info!("Hover info for factorial function: {}", hover_info);

    // Hover should contain function signature information
    assert!(
        hover_info.contains("factorial")
            || hover_info.contains("int")
            || hover_info.contains("Math")
    );
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_hover_info_method_symbol() {
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

    let mut locked_session = session.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get a method symbol
    let symbol = get_matching_symbol("Math::Complex::add", &mut locked_session)
        .await
        .expect("Failed to find add method symbol");
    let symbol_location = &symbol.location;

    // Test getting hover information
    let hover_info = get_hover_info(symbol_location, &mut locked_session)
        .await
        .expect("Failed to get hover info");

    assert!(!hover_info.is_empty());
    info!("Hover info for add method: {}", hover_info);

    // Hover should contain method information
    assert!(hover_info.contains("add") || hover_info.contains("Complex"));
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_hover_info_interface_symbol() {
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

    let mut locked_session = session.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get interface symbol
    let symbol = get_matching_symbol("IStorageBackend", &mut locked_session)
        .await
        .expect("Failed to find IStorageBackend symbol");
    let symbol_location = &symbol.location;

    // Test getting hover information
    let hover_info = get_hover_info(symbol_location, &mut locked_session)
        .await
        .expect("Failed to get hover info");

    assert!(!hover_info.is_empty());
    info!("Hover info for IStorageBackend interface: {}", hover_info);

    // Hover should contain interface information
    assert!(
        hover_info.contains("IStorageBackend")
            || hover_info.contains("Storage")
            || hover_info.contains("class")
    );
}
