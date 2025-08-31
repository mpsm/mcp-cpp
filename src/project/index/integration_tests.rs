//! Integration tests for indexing progress tracking

use crate::project::{ProjectScanner, WorkspaceSession};
use crate::test_utils::integration::TestProject;
use std::time::Duration;
use tracing::debug;

#[cfg(feature = "test-logging")]
#[ctor::ctor]
fn init_test_logging() {
    crate::test_utils::logging::init();
}

#[tokio::test]
async fn test_indexing_progress_tracking_with_real_clangd() {
    let test_project = TestProject::new().await.unwrap();
    test_project.cmake_configure().await.unwrap();

    let scanner = ProjectScanner::with_default_providers();
    let workspace = scanner
        .scan_project(&test_project.project_root, 2, None)
        .unwrap();

    let clangd_path = crate::test_utils::get_test_clangd_path();
    let workspace_session = WorkspaceSession::new(workspace, clangd_path).unwrap();

    // Get initial coverage (should be 0)
    let initial_coverage = workspace_session
        .get_indexing_coverage(&test_project.build_dir)
        .await;
    assert_eq!(initial_coverage, Some(0.0));

    // Start clangd session and open a file to trigger indexing
    let session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .unwrap();

    let main_cpp_path = test_project.project_root.join("src/main.cpp");
    session
        .lock()
        .await
        .ensure_file_ready(&main_cpp_path)
        .await
        .unwrap();

    // Wait for indexing with timeout
    tokio::time::timeout(Duration::from_secs(30), async {
        workspace_session.wait_for_indexing_completion(&test_project.build_dir).await
    })
    .await
    .expect("Indexing timed out")
    .expect("Indexing failed");

    // Check final coverage
    let final_coverage = workspace_session
        .get_indexing_coverage(&test_project.build_dir)
        .await;

    if let Some(coverage) = final_coverage {
        assert!(
            (coverage - 1.0).abs() < 0.001,
            "All compilation database files should be indexed, expected coverage = 1.0, got: {}",
            coverage
        );
    } else {
        panic!("Final indexing coverage should not be None");
    }
}

#[tokio::test]
async fn test_wait_for_indexing_completion_ensures_full_coverage() {
    let test_project = TestProject::new().await.unwrap();
    test_project.cmake_configure().await.unwrap();

    let scanner = ProjectScanner::with_default_providers();
    let workspace = scanner
        .scan_project(&test_project.project_root, 2, None)
        .unwrap();

    let clangd_path = crate::test_utils::get_test_clangd_path();
    let workspace_session = WorkspaceSession::new(workspace, clangd_path).unwrap();

    // Create session first - required for wait_for_indexing_completion
    let _session = workspace_session
        .get_or_create_session(test_project.build_dir.clone())
        .await
        .unwrap();

    let result = tokio::time::timeout(
        Duration::from_secs(60),
        workspace_session.wait_for_indexing_completion(&test_project.build_dir),
    )
    .await;

    match result {
        Ok(Ok(())) => {
            debug!("wait_for_indexing_completion succeeded");
        }
        Ok(Err(e)) => {
            panic!("wait_for_indexing_completion failed: {}", e);
        }
        Err(_) => {
            panic!("wait_for_indexing_completion timed out after 60s");
        }
    }
}
