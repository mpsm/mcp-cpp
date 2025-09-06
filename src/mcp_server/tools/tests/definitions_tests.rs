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
use std::sync::{Arc, Mutex};
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Complete indexing using ComponentSession prior to session operations
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(std::time::Duration::from_secs(30))
        .await
        .unwrap();

    // Acquire session lock for LSP operations
    let session_arc = component_session.clangd_session();
    let mut locked_session = session_arc.lock().await;

    // Get Math class symbol
    let symbol = get_matching_symbol("Math", &mut locked_session)
        .await
        .expect("Failed to find Math symbol");

    // Test getting definitions
    let definitions = get_definitions(&symbol.location, &mut locked_session)
        .await
        .expect("Failed to get definitions");

    assert!(!definitions.is_empty());
    info!("Found {} definitions for Math class", definitions.len());

    for (i, definition) in definitions.iter().enumerate() {
        info!(
            "Definition {}: {} at {}:{}",
            i + 1,
            definition.to_compact_range(),
            definition.file_path.display(),
            definition.range.start.line + 1
        );
        // Verify the location points to a Math-related file
        assert!(
            definition.file_path.to_string_lossy().contains("Math")
                || definition.file_path.to_string_lossy().contains("math")
        );
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Complete indexing using ComponentSession prior to session operations
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(std::time::Duration::from_secs(30))
        .await
        .unwrap();

    // Acquire session lock for LSP operations
    let session_arc = component_session.clangd_session();
    let mut locked_session = session_arc.lock().await;

    // Get Math class symbol
    let symbol = get_matching_symbol("Math", &mut locked_session)
        .await
        .expect("Failed to find Math symbol");

    // Test getting declarations
    let declarations = get_declarations(&symbol.location, &mut locked_session)
        .await
        .expect("Failed to get declarations");

    assert!(!declarations.is_empty());
    info!("Found {} declarations for Math class", declarations.len());

    for (i, declaration) in declarations.iter().enumerate() {
        info!(
            "Declaration {}: {} at {}:{}",
            i + 1,
            declaration.to_compact_range(),
            declaration.file_path.display(),
            declaration.range.start.line + 1
        );
        // Verify the location points to a Math-related file
        assert!(
            declaration.file_path.to_string_lossy().contains("Math")
                || declaration.file_path.to_string_lossy().contains("math")
        );
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Complete indexing using ComponentSession prior to session operations
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(std::time::Duration::from_secs(30))
        .await
        .unwrap();

    // Acquire session lock for LSP operations
    let session_arc = component_session.clangd_session();
    let mut locked_session = session_arc.lock().await;
    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

    // Get factorial function symbol
    let symbol = get_matching_symbol("factorial", &mut locked_session)
        .await
        .expect("Failed to find factorial symbol");

    // Test getting definitions
    let definitions = get_definitions(&symbol.location, &mut locked_session)
        .await
        .expect("Failed to get definitions");

    assert!(!definitions.is_empty());
    info!(
        "Found {} definitions for factorial function",
        definitions.len()
    );

    for (i, definition) in definitions.iter().enumerate() {
        // Get the line content using the file buffer
        let mut locked_file_buffer = file_buffer_manager.lock().unwrap();
        let buffer = locked_file_buffer
            .get_buffer(&definition.file_path)
            .expect("Failed to get file buffer");
        let line_content = buffer
            .get_line(definition.range.start.line)
            .expect("Failed to get line content");

        info!(
            "Definition {}: {} at {}:{}",
            i + 1,
            line_content.trim(),
            definition.file_path.display(),
            definition.range.start.line
        );
        // Function definition should contain the function signature
        assert!(line_content.contains("factorial") || line_content.contains("Math::factorial"));
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
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    // Complete indexing using ComponentSession prior to session operations
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    component_session
        .ensure_indexed(std::time::Duration::from_secs(30))
        .await
        .unwrap();

    // Acquire session lock for LSP operations
    let session_arc = component_session.clangd_session();
    let mut locked_session = session_arc.lock().await;
    let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

    // Get a method symbol (using qualified name search)
    let symbol = get_matching_symbol("Math::Complex::add", &mut locked_session)
        .await
        .expect("Failed to find add method symbol");

    // Test getting definitions
    let definitions = get_definitions(&symbol.location, &mut locked_session)
        .await
        .expect("Failed to get definitions");

    assert!(!definitions.is_empty());
    info!("Found {} definitions for add method", definitions.len());

    for (i, definition) in definitions.iter().enumerate() {
        // Get the line content using the file buffer
        let mut locked_file_buffer = file_buffer_manager.lock().unwrap();
        let buffer = locked_file_buffer
            .get_buffer(&definition.file_path)
            .expect("Failed to get file buffer");
        let line_content = buffer
            .get_line(definition.range.start.line)
            .expect("Failed to get line content");

        info!(
            "Definition {}: {} at {}:{}",
            i + 1,
            line_content.trim(),
            definition.file_path.display(),
            definition.range.start.line
        );
        // Method definition should contain the method name
        assert!(line_content.contains("add"));
    }
}
