//! Workspace session management
//!
//! Provides `WorkspaceSession` for managing ComponentSession instances across different
//! build directories within a project workspace. This module handles pure session
//! lifecycle management without build directory resolution policy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::clangd::version::ClangdVersion;
use crate::project::component_session::ComponentSession;
use crate::project::{ProjectError, ProjectScanner, ProjectWorkspace};

/// Manages ComponentSession instances for a project workspace
///
/// `WorkspaceSession` provides pure session lifecycle management, handling the creation,
/// reuse, and cleanup of ComponentSession instances for different build directories.
/// This orchestrates component sessions while maintaining the same external API.
/// Supports dynamic component discovery for build directories not found in initial scanning.
pub struct WorkspaceSession {
    /// Project workspace for determining project root and components (mutable for dynamic discovery)
    workspace: Arc<Mutex<ProjectWorkspace>>,
    /// Map of build directories to their ComponentSession instances
    component_sessions: Arc<Mutex<HashMap<PathBuf, Arc<ComponentSession>>>>,
    /// Path to clangd executable
    clangd_path: String,
    /// Clangd version information
    clangd_version: ClangdVersion,
    /// Project scanner for dynamic component discovery
    scanner: ProjectScanner,
}

impl WorkspaceSession {
    /// Create a new WorkspaceSession for the given project workspace
    pub fn new(workspace: ProjectWorkspace, clangd_path: String) -> Result<Self, ProjectError> {
        // Detect clangd version for index format compatibility
        let clangd_version = ClangdVersion::detect(Path::new(&clangd_path)).map_err(|e| {
            ProjectError::SessionCreation(format!("Failed to detect clangd version: {}", e))
        })?;

        info!(
            "Detected clangd version: {}.{}.{}",
            clangd_version.major, clangd_version.minor, clangd_version.patch
        );

        // Create scanner with default providers for dynamic discovery
        let scanner = ProjectScanner::with_default_providers();

        Ok(Self {
            workspace: Arc::new(Mutex::new(workspace)),
            component_sessions: Arc::new(Mutex::new(HashMap::new())),
            clangd_path,
            clangd_version,
            scanner,
        })
    }

    /// Get or create a ComponentSession for the specified build directory
    pub async fn get_component_session(
        &self,
        build_dir: PathBuf,
    ) -> Result<Arc<ComponentSession>, ProjectError> {
        let mut sessions = self.component_sessions.lock().await;

        // Check if we already have a component session for this build directory
        if let Some(component_session) = sessions.get(&build_dir) {
            info!(
                "Reusing existing ComponentSession for build dir: {}",
                build_dir.display()
            );
            return Ok(Arc::clone(component_session));
        }

        // Create a new component session for this build directory
        info!(
            "Creating new ComponentSession for build dir: {}",
            build_dir.display()
        );

        // Try to get the component from the workspace first
        let component = {
            let workspace = self.workspace.lock().await;
            workspace.get_component_by_build_dir(&build_dir).cloned()
        };

        let component = match component {
            Some(comp) => comp,
            None => {
                // Component not found in workspace - try dynamic discovery
                info!(
                    "Component not found in workspace, attempting dynamic discovery for: {}",
                    build_dir.display()
                );

                match self.scanner.discover_component(&build_dir)? {
                    Some(discovered_component) => {
                        // Add the discovered component to the workspace
                        let mut workspace = self.workspace.lock().await;
                        workspace.add_component(discovered_component.clone());
                        info!(
                            "Successfully discovered and added component for build dir: {}",
                            build_dir.display()
                        );
                        discovered_component
                    }
                    None => {
                        let workspace = self.workspace.lock().await;
                        let available_dirs = workspace.get_build_dirs();
                        return Err(ProjectError::SessionCreation(format!(
                            "No valid project component found at build directory: '{}'. Scan root: '{}'. Use get_project_details to discover available build directories. Available directories: {:?}. Ensure you're using absolute paths from that output to avoid path concatenation issues.",
                            build_dir.display(),
                            workspace.project_root_path.display(),
                            available_dirs
                        )));
                    }
                }
            }
        };

        // Determine project root from workspace
        let project_root = {
            let workspace = self.workspace.lock().await;
            if workspace.project_root_path.exists() {
                workspace.project_root_path.clone()
            } else {
                std::env::current_dir().map_err(|e| {
                    ProjectError::SessionCreation(format!("Failed to get current directory: {}", e))
                })?
            }
        };

        // Create ComponentSession
        let component_session = ComponentSession::new(
            component,
            &self.clangd_path,
            &self.clangd_version,
            project_root,
        )
        .await?;

        let component_session_arc = Arc::new(component_session);

        // Store the component session for future reuse
        sessions.insert(build_dir, Arc::clone(&component_session_arc));

        // Drop sessions lock before returning
        drop(sessions);

        Ok(component_session_arc)
    }

    /// Get a non-mutable reference to the project workspace
    ///
    /// Note: This now returns an Arc<Mutex<ProjectWorkspace>> since the workspace
    /// can be mutated during dynamic component discovery. Callers should lock
    /// the mutex to access workspace data.
    pub fn get_workspace(&self) -> &Arc<Mutex<ProjectWorkspace>> {
        &self.workspace
    }
}

impl Drop for WorkspaceSession {
    fn drop(&mut self) {
        // Clear the component sessions HashMap to drop all Arc references
        // ComponentSession::drop() will be called for proper cleanup of resources
        if let Ok(mut sessions) = self.component_sessions.try_lock() {
            sessions.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::integration::TestProject;
    use std::sync::Arc;

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_dynamic_component_discovery() {
        // Create test project and configure it
        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        // Create empty workspace (no initial scan)
        let empty_workspace = ProjectWorkspace::new(
            test_project.project_root.clone(),
            vec![], // No components
            0,
        );

        // Verify workspace is actually empty
        assert_eq!(empty_workspace.component_count(), 0);
        assert!(
            empty_workspace
                .get_component_by_build_dir(&test_project.build_dir)
                .is_none()
        );

        // Create workspace session with empty workspace
        let clangd_path = crate::test_utils::get_test_clangd_path();
        let workspace_session = WorkspaceSession::new(empty_workspace, clangd_path).unwrap();

        // Request component session for build directory not in workspace
        // This should trigger dynamic discovery
        let component_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await;

        // Should succeed through dynamic discovery
        assert!(
            component_session.is_ok(),
            "Dynamic discovery should have succeeded"
        );
        let session = component_session.unwrap();
        assert_eq!(session.build_dir(), &test_project.build_dir);

        // Verify the component was added to the workspace
        {
            let workspace = workspace_session.get_workspace().lock().await;
            assert_eq!(workspace.component_count(), 1);
            assert!(
                workspace
                    .get_component_by_build_dir(&test_project.build_dir)
                    .is_some()
            );
        }

        // Second request should reuse cached session
        let second_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await
            .unwrap();

        // Verify it's the same session (Arc comparison)
        assert!(Arc::ptr_eq(&session, &second_session));
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_invalid_build_directory_fails() {
        // Create empty workspace
        let temp_dir = tempfile::tempdir().unwrap();
        let empty_workspace = ProjectWorkspace::new(temp_dir.path().to_path_buf(), vec![], 0);

        let clangd_path = crate::test_utils::get_test_clangd_path();
        let workspace_session = WorkspaceSession::new(empty_workspace, clangd_path).unwrap();

        // Request session for non-existent/invalid build directory
        let invalid_dir = temp_dir.path().join("not_a_build_dir");
        std::fs::create_dir_all(&invalid_dir).unwrap(); // Create directory but don't make it a build directory

        let result = workspace_session.get_component_session(invalid_dir).await;

        // Should fail as it's not a valid build directory
        assert!(result.is_err(), "Should fail for invalid build directory");
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_existing_component_not_rediscovered() {
        // Create test project and configure it
        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        // Scan the project to create workspace with existing component
        let scanner = crate::project::ProjectScanner::with_default_providers();
        let workspace = scanner
            .scan_project(&test_project.project_root, 2, None)
            .unwrap();

        // Verify component is already in workspace
        assert_eq!(workspace.component_count(), 1);
        assert!(
            workspace
                .get_component_by_build_dir(&test_project.build_dir)
                .is_some()
        );

        // Create workspace session with pre-populated workspace
        let clangd_path = crate::test_utils::get_test_clangd_path();
        let workspace_session = WorkspaceSession::new(workspace, clangd_path).unwrap();

        // Request component session - should use existing component, not rediscover
        let component_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await;

        // Should succeed using existing component
        assert!(
            component_session.is_ok(),
            "Should succeed with existing component"
        );
        let session = component_session.unwrap();
        assert_eq!(session.build_dir(), &test_project.build_dir);

        // Verify workspace still has exactly one component (not duplicated)
        {
            let workspace = workspace_session.get_workspace().lock().await;
            assert_eq!(workspace.component_count(), 1);
        }
    }
}
