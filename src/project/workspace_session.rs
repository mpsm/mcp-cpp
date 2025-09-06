//! Workspace session management
//!
//! Provides `WorkspaceSession` for managing ComponentSession instances across different
//! build directories within a project workspace. This module handles pure session
//! lifecycle management without build directory resolution policy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::info;

use crate::clangd::index::IndexLatch;
use crate::clangd::version::ClangdVersion;
use crate::project::component_session::ComponentSession;
use crate::project::index::{ComponentIndexMonitor, ComponentIndexState};
use crate::project::{ProjectError, ProjectWorkspace};

/// Manages ComponentSession instances for a project workspace
///
/// `WorkspaceSession` provides pure session lifecycle management, handling the creation,
/// reuse, and cleanup of ComponentSession instances for different build directories.
/// This orchestrates component sessions while maintaining the same external API.
pub struct WorkspaceSession {
    /// Project workspace for determining project root and components
    workspace: ProjectWorkspace,
    /// Map of build directories to their ComponentSession instances
    component_sessions: Arc<Mutex<HashMap<PathBuf, Arc<ComponentSession>>>>,
    /// Path to clangd executable
    clangd_path: String,
    /// Clangd version information
    clangd_version: ClangdVersion,
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

        Ok(Self {
            workspace,
            component_sessions: Arc::new(Mutex::new(HashMap::new())),
            clangd_path,
            clangd_version,
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

        // Get the component for this build directory
        let component = self
            .workspace
            .get_component_by_build_dir(&build_dir)
            .ok_or_else(|| {
                ProjectError::SessionCreation("Component not found for build directory".to_string())
            })?;

        // Determine project root from workspace
        let project_root = if self.workspace.project_root_path.exists() {
            self.workspace.project_root_path.clone()
        } else {
            std::env::current_dir().map_err(|e| {
                ProjectError::SessionCreation(format!("Failed to get current directory: {}", e))
            })?
        };

        // Create ComponentSession
        let component_session = ComponentSession::new(
            component.clone(),
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

    /// Get an existing ComponentIndexMonitor for the specified build directory
    #[allow(dead_code)]
    pub async fn get_component_monitor(
        &self,
        build_dir: &Path,
    ) -> Option<Arc<ComponentIndexMonitor>> {
        let sessions = self.component_sessions.lock().await;
        sessions
            .get(build_dir)
            .map(|session| session.index_monitor())
    }

    /// Wait for indexing completion with coverage assurance using custom timeout
    ///
    /// This method waits for clangd to complete indexing and ensures that all files
    /// in the compilation database have been indexed. If coverage is incomplete after
    /// initial indexing, it will trigger indexing for unindexed files.
    pub async fn wait_for_indexing_completion_with_timeout(
        &self,
        build_dir: &Path,
        timeout: Duration,
    ) -> Result<(), ProjectError> {
        info!(
            "Waiting for indexing completion for build dir: {} (timeout: {:?})",
            build_dir.display(),
            timeout
        );

        // Get the ComponentSession - if none exists, create one
        let component_session = self.get_component_session(build_dir.to_path_buf()).await?;

        // Delegate to ComponentSession
        component_session
            .wait_for_indexing_completion(timeout)
            .await
    }

    /// Get component indexing state for a build directory
    #[allow(dead_code)]
    pub async fn get_component_index_state(&self, build_dir: &Path) -> Option<ComponentIndexState> {
        // Get the ComponentSession - if none exists, return None
        let sessions = self.component_sessions.lock().await;
        if let Some(component_session) = sessions.get(build_dir) {
            Some(component_session.get_index_state().await)
        } else {
            None
        }
    }

    /// Wait for indexing completion with coverage assurance using default timeout
    ///
    /// This method waits for clangd to complete indexing and ensures that all files
    /// in the compilation database have been indexed. If coverage is incomplete after
    /// initial indexing, it will trigger indexing for unindexed files.
    /// Uses a default 5 minute timeout.
    #[allow(dead_code)]
    pub async fn wait_for_indexing_completion(&self, build_dir: &Path) -> Result<(), ProjectError> {
        const DEFAULT_INDEXING_TIMEOUT: Duration = Duration::from_secs(300);
        self.wait_for_indexing_completion_with_timeout(build_dir, DEFAULT_INDEXING_TIMEOUT)
            .await
    }

    /// Get a non-mutable reference to the project workspace
    pub fn get_workspace(&self) -> &ProjectWorkspace {
        &self.workspace
    }

    /// Refresh index state by synchronizing with actual index files on disk
    ///
    /// This method reads the current state of index files and updates the IndexState
    /// to reflect staleness and availability of actual index data.
    #[allow(dead_code)]
    pub async fn refresh_index_state(&self, build_dir: &Path) -> Result<(), ProjectError> {
        // Get the ComponentSession - if none exists, create one
        let component_session = self.get_component_session(build_dir.to_path_buf()).await?;

        // Delegate to ComponentSession
        component_session.refresh_index_state().await
    }

    /// Get latch for a build directory to wait for indexing completion
    #[allow(dead_code)]
    pub async fn get_index_latch(&self, build_dir: &Path) -> Option<IndexLatch> {
        // Get the ComponentSession - if none exists, return None
        let sessions = self.component_sessions.lock().await;
        if let Some(component_session) = sessions.get(build_dir) {
            Some(component_session.get_index_latch().await)
        } else {
            None
        }
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
