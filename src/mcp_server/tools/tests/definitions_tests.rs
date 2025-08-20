//! Integration tests for definition and declaration analysis functionality
//!
//! These tests verify that the definition and declaration analysis functionality
//! works correctly with real clangd integration, testing various scenarios including
//! finding definitions, declarations, and handling edge cases.

use crate::io::file_manager::RealFileBufferManager;
use crate::mcp_server::tools::lsp_helpers::{
    definitions::{get_declarations, get_definitions},
    symbol_resolution::get_matching_symbol,
};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_definitions_class_symbol() {
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
    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));
    let mut locked_file_buffer = file_buffer_manager.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get Math class symbol
    let symbol = get_matching_symbol("Math", &mut locked_session)
        .await
        .expect("Failed to find Math symbol");

    // Test getting definitions
    let definitions = get_definitions(
        &mut locked_session,
        &mut locked_file_buffer,
        &symbol.location,
    )
    .await
    .expect("Failed to get definitions");

    assert!(!definitions.is_empty());
    info!("Found {} definitions for Math class", definitions.len());

    for (i, definition) in definitions.iter().enumerate() {
        info!(
            "Definition {}: {} at {}:{}",
            i + 1,
            definition.contents.trim(),
            definition.line.file_path.display(),
            definition.line.line_number
        );
        assert!(definition.contents.contains("Math"));
    }
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_declarations_class_symbol() {
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
    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));
    let mut locked_file_buffer = file_buffer_manager.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get Math class symbol
    let symbol = get_matching_symbol("Math", &mut locked_session)
        .await
        .expect("Failed to find Math symbol");

    // Test getting declarations
    let declarations = get_declarations(
        &mut locked_session,
        &mut locked_file_buffer,
        &symbol.location,
    )
    .await
    .expect("Failed to get declarations");

    assert!(!declarations.is_empty());
    info!("Found {} declarations for Math class", declarations.len());

    for (i, declaration) in declarations.iter().enumerate() {
        info!(
            "Declaration {}: {} at {}:{}",
            i + 1,
            declaration.contents.trim(),
            declaration.line.file_path.display(),
            declaration.line.line_number
        );
        assert!(declaration.contents.contains("Math"));
    }
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_definitions_function_symbol() {
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
    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));
    let mut locked_file_buffer = file_buffer_manager.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get factorial function symbol
    let symbol = get_matching_symbol("factorial", &mut locked_session)
        .await
        .expect("Failed to find factorial symbol");

    // Test getting definitions
    let definitions = get_definitions(
        &mut locked_session,
        &mut locked_file_buffer,
        &symbol.location,
    )
    .await
    .expect("Failed to get definitions");

    assert!(!definitions.is_empty());
    info!(
        "Found {} definitions for factorial function",
        definitions.len()
    );

    for (i, definition) in definitions.iter().enumerate() {
        info!(
            "Definition {}: {} at {}:{}",
            i + 1,
            definition.contents.trim(),
            definition.line.file_path.display(),
            definition.line.line_number
        );
        // Function definition should contain the function signature
        assert!(
            definition.contents.contains("factorial")
                || definition.contents.contains("Math::factorial")
        );
    }
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_definitions_method_symbol() {
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
    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));
    let mut locked_file_buffer = file_buffer_manager.lock().await;

    // Wait for clangd indexing to complete before searching
    crate::mcp_server::tools::utils::wait_for_indexing(locked_session.index_monitor(), None).await;

    // Get a method symbol (using qualified name search)
    let symbol = get_matching_symbol("Math::Complex::add", &mut locked_session)
        .await
        .expect("Failed to find add method symbol");

    // Test getting definitions
    let definitions = get_definitions(
        &mut locked_session,
        &mut locked_file_buffer,
        &symbol.location,
    )
    .await
    .expect("Failed to get definitions");

    assert!(!definitions.is_empty());
    info!("Found {} definitions for add method", definitions.len());

    for (i, definition) in definitions.iter().enumerate() {
        info!(
            "Definition {}: {} at {}:{}",
            i + 1,
            definition.contents.trim(),
            definition.line.file_path.display(),
            definition.line.line_number
        );
        // Method definition should contain the method name
        assert!(definition.contents.contains("add"));
    }
}
