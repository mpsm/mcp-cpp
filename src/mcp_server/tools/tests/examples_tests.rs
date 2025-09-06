//! Integration tests for usage examples and reference analysis functionality
//!
//! These tests verify that the usage examples and reference analysis functionality
//! works correctly with real clangd integration, testing reference collection,
//! example limiting, and various usage pattern scenarios.

use crate::io::file_manager::RealFileBufferManager;
use crate::mcp_server::tools::lsp_helpers::{
    examples::get_examples, symbol_resolution::get_matching_symbol,
};
use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_examples_class_usage() {
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

    // Get Math class symbol
    let symbol = get_matching_symbol("Math", &mut locked_session)
        .await
        .expect("Failed to find Math symbol");
    let symbol_location = &symbol.location;

    // Test getting usage examples (unlimited)
    let examples = get_examples(&mut locked_session, symbol_location, None)
        .await
        .expect("Failed to get examples");

    assert!(!examples.is_empty());
    info!("Found {} usage examples for Math class", examples.len());

    for (i, example) in examples.iter().enumerate() {
        // Get the line content using the file buffer
        let mut locked_file_buffer = file_buffer_manager.lock().await;
        let buffer = locked_file_buffer
            .get_buffer(&example.file_path)
            .expect("Failed to get file buffer");
        let line_content = buffer
            .get_line(example.range.start.line)
            .expect("Failed to get line content");

        info!(
            "Example {}: {} at {}:{}",
            i + 1,
            line_content.trim(),
            example.file_path.display(),
            example.range.start.line
        );
        // Usage examples should reference the Math class
        assert!(line_content.contains("Math"));
    }
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_examples_function_usage() {
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
    let symbol_location = &symbol.location;

    // Test getting usage examples (unlimited)
    let examples = get_examples(&mut locked_session, symbol_location, None)
        .await
        .expect("Failed to get examples");

    assert!(!examples.is_empty());
    info!(
        "Found {} usage examples for factorial function",
        examples.len()
    );

    for (i, example) in examples.iter().enumerate() {
        // Get the line content using the file buffer
        let mut locked_file_buffer = file_buffer_manager.lock().await;
        let buffer = locked_file_buffer
            .get_buffer(&example.file_path)
            .expect("Failed to get file buffer");
        let line_content = buffer
            .get_line(example.range.start.line)
            .expect("Failed to get line content");

        info!(
            "Example {}: {} at {}:{}",
            i + 1,
            line_content.trim(),
            example.file_path.display(),
            example.range.start.line
        );
        // Usage examples should reference the factorial function
        assert!(line_content.contains("factorial"));
    }
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_examples_with_max_limit() {
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

    // Get Math class symbol (should have multiple usage examples)
    let symbol = get_matching_symbol("Math", &mut locked_session)
        .await
        .expect("Failed to find Math symbol");
    let symbol_location = &symbol.location;

    // Test getting examples with max limit
    const MAX_EXAMPLES: u32 = 2;
    let examples = get_examples(&mut locked_session, symbol_location, Some(MAX_EXAMPLES))
        .await
        .expect("Failed to get examples");

    assert!(!examples.is_empty());
    assert!(
        examples.len() <= MAX_EXAMPLES as usize,
        "Should have at most {} examples, but got {}",
        MAX_EXAMPLES,
        examples.len()
    );

    info!(
        "Found {} usage examples for Math class (max was {})",
        examples.len(),
        MAX_EXAMPLES
    );

    for (i, example) in examples.iter().enumerate() {
        // Get the line content using the file buffer
        let mut locked_file_buffer = file_buffer_manager.lock().await;
        let buffer = locked_file_buffer
            .get_buffer(&example.file_path)
            .expect("Failed to get file buffer");
        let line_content = buffer
            .get_line(example.range.start.line)
            .expect("Failed to get line content");

        info!(
            "Example {}: {} at {}:{}",
            i + 1,
            line_content.trim(),
            example.file_path.display(),
            example.range.start.line
        );
        assert!(line_content.contains("Math"));
    }
}

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_examples_method_usage() {
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

    // Get a method symbol
    let symbol = get_matching_symbol("Math::Complex::add", &mut locked_session)
        .await
        .expect("Failed to find add method symbol");
    let symbol_location = &symbol.location;

    // Test getting usage examples
    let examples = get_examples(&mut locked_session, symbol_location, Some(3))
        .await
        .expect("Failed to get examples");

    assert!(!examples.is_empty());
    info!("Found {} usage examples for add method", examples.len());

    for (i, example) in examples.iter().enumerate() {
        // Get the line content using the file buffer
        let mut locked_file_buffer = file_buffer_manager.lock().await;
        let buffer = locked_file_buffer
            .get_buffer(&example.file_path)
            .expect("Failed to get file buffer");
        let line_content = buffer
            .get_line(example.range.start.line)
            .expect("Failed to get line content");

        info!(
            "Example {}: {} at {}:{}",
            i + 1,
            line_content.trim(),
            example.file_path.display(),
            example.range.start.line
        );
        // Usage examples should reference the method
        assert!(line_content.contains("add"));
    }
}
