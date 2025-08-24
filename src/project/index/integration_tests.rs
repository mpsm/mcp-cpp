//! Integration tests for indexing progress tracking
//!
//! These tests verify that our indexing progress monitoring works correctly
//! with real clangd integration, including IndexReader, IndexState, and WorkspaceSession integration.

use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use std::time::Duration;
use tracing::info;

#[cfg(feature = "clangd-integration-tests")]
#[tokio::test]
async fn test_indexing_progress_tracking_with_real_clangd() {
    // Create a test project first
    let test_project = TestProject::new().await.unwrap();
    test_project.cmake_configure().await.unwrap();

    info!(
        "Created test project at: {}",
        test_project.project_root.display()
    );

    // Scan the test project to create a proper workspace with components
    let scanner = ProjectScanner::with_default_providers();
    let workspace = scanner
        .scan_project(&test_project.project_root, 3, None)
        .expect("Failed to scan test project");

    info!(
        "Scanned workspace with {} components",
        workspace.components.len()
    );

    // Create a WorkspaceSession with test clangd path - this initializes index management
    let clangd_path = crate::test_utils::get_test_clangd_path();
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
        .expect("Failed to create workspace session");

    info!("Created WorkspaceSession");

    // Get or create session for the build directory - this should initialize IndexState and IndexReader
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .expect("Failed to create session");

    info!("Created ClangdSession for build directory");

    // Check initial indexing coverage - should be 0.0 initially
    let initial_coverage = workspace_session
        .get_indexing_coverage(&test_project.build_dir)
        .await;

    info!("Initial indexing coverage: {:?}", initial_coverage);

    // Open main.cpp to trigger indexing
    let main_cpp_path = test_project.project_root.join("src/main.cpp");
    {
        let mut session_guard = session.lock().await;
        session_guard
            .ensure_file_ready(&main_cpp_path)
            .await
            .expect("Failed to open main.cpp");
    }

    info!("Opened main.cpp to trigger indexing");

    // Wait for indexing to complete with timeout
    let completion_result = {
        let session_guard = session.lock().await;
        tokio::time::timeout(
            Duration::from_secs(30),
            session_guard.index_monitor().wait_for_indexing_completion(),
        )
        .await
    };

    // Verify indexing completed successfully
    match completion_result {
        Ok(Ok(())) => {
            info!("Indexing completed successfully");

            // Check final indexing coverage - should be 1.0 (100%) after indexing
            let final_coverage = workspace_session
                .get_indexing_coverage(&test_project.build_dir)
                .await;

            info!("Final indexing coverage: {:?}", final_coverage);

            assert!(
                final_coverage.is_some(),
                "IndexState should be initialized and provide coverage information"
            );

            let coverage = final_coverage.unwrap();
            assert!(
                (coverage - 1.0).abs() < 0.001,
                "All compilation database files should be indexed, expected coverage = 1.0, got: {}",
                coverage
            );
            info!(
                "✅ Indexing progress tracking verified: coverage = {}",
                coverage
            );
        }
        Ok(Err(e)) => panic!("Indexing failed: {e}"),
        Err(_) => panic!("Indexing timed out after 30 seconds"),
    }

    // Session will be automatically cleaned up when it goes out of scope

    info!("✅ Integration test completed successfully");
}
