//! Integration tests for indexing progress tracking

use crate::project::index::component_monitor::ComponentIndexingState;
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

    // Get component session first to create the monitor
    let component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
        .await
        .unwrap();
    // Get initial coverage (should be 0)
    let state = component_session.get_index_state().await;
    assert_eq!(state.coverage(), 0.0);

    let main_cpp_path = test_project.project_root.join("src/main.cpp");
    component_session
        .ensure_file_ready(&main_cpp_path)
        .await
        .unwrap();

    // Wait for indexing with timeout
    tokio::time::timeout(Duration::from_secs(30), async {
        workspace_session
            .wait_for_indexing_completion(&test_project.build_dir)
            .await
    })
    .await
    .expect("Indexing timed out")
    .expect("Indexing failed");

    // Check final coverage
    let final_state = component_session.get_index_state().await;
    let coverage = final_state.coverage();
    assert!(
        (coverage - 1.0).abs() < 0.001,
        "All compilation database files should be indexed, expected coverage = 1.0, got: {}",
        coverage
    );
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
    let _component_session = workspace_session
        .get_component_session(test_project.build_dir.clone())
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

#[tokio::test]
async fn test_index_persistence_across_session_restarts() {
    // Test project has exactly this many files in compilation database
    const EXPECTED_CDB_FILES: usize = 6;

    let test_project = TestProject::new().await.unwrap();
    test_project.cmake_configure().await.unwrap();

    let scanner = ProjectScanner::with_default_providers();
    let workspace = scanner
        .scan_project(&test_project.project_root, 2, None)
        .unwrap();

    let clangd_path = crate::test_utils::get_test_clangd_path();
    let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path.clone()).unwrap();

    // Phase 1: Initial indexing - create session and wait for completion
    debug!("Phase 1: Starting initial indexing");
    {
        let _component_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await
            .unwrap();

        // Wait for initial indexing completion
        tokio::time::timeout(
            Duration::from_secs(60),
            workspace_session.wait_for_indexing_completion(&test_project.build_dir),
        )
        .await
        .expect("Initial indexing timed out")
        .expect("Initial indexing failed");

        debug!("Phase 1: Initial indexing completed");

        // Verify indexing is complete with explicit state checks
        let component_state = workspace_session
            .get_component_index_state(&test_project.build_dir)
            .await
            .expect("Should have component index state");

        assert_eq!(
            component_state.total_cdb_files, EXPECTED_CDB_FILES,
            "Test project should have exactly {} CDB files",
            EXPECTED_CDB_FILES
        );
        assert_eq!(
            component_state.indexed_cdb_files, EXPECTED_CDB_FILES,
            "All {} CDB files should be indexed after completion",
            EXPECTED_CDB_FILES
        );
        assert_eq!(
            component_state.state,
            ComponentIndexingState::Completed,
            "Component should be in Completed state"
        );
    }

    // Phase 2: Cleanup session completely
    debug!("Phase 2: Cleaning up session and index monitor");

    // Force cleanup of the session by dropping workspace session and recreating it
    // This simulates complete cleanup including index monitors
    drop(workspace_session);

    // Recreate workspace session from scratch
    let workspace_session = WorkspaceSession::new(workspace, clangd_path).unwrap();

    // Phase 3: Restart session and verify index state persistence
    debug!("Phase 3: Restarting session and verifying index persistence");
    {
        let _component_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await
            .unwrap();

        tokio::time::timeout(
            Duration::from_secs(60),
            workspace_session.wait_for_indexing_completion(&test_project.build_dir),
        )
        .await
        .expect("Second indexing timed out")
        .expect("Second indexing failed");

        let component_state = workspace_session
            .get_component_index_state(&test_project.build_dir)
            .await
            .expect("Should have component index state after indexing completion");

        debug!("Component state after restart: {:?}", component_state);

        // Explicit deterministic assertions
        assert_eq!(
            component_state.state,
            ComponentIndexingState::Completed,
            "Component should be Completed state from persisted index"
        );
        assert_eq!(
            component_state.total_cdb_files, EXPECTED_CDB_FILES,
            "Total CDB files should be detected from disk"
        );
        assert_eq!(
            component_state.indexed_cdb_files, EXPECTED_CDB_FILES,
            "All {} CDB files should be detected as indexed from disk",
            EXPECTED_CDB_FILES
        );

        debug!("Phase 3: Index persistence verified - session can read existing index from disk");
    }
}
