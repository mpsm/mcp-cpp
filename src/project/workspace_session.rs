//! Workspace session management
//!
//! Provides `WorkspaceSession` for managing ClangdSession instances across different
//! build directories within a project workspace. This module handles pure session
//! lifecycle management without build directory resolution policy.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument};

use crate::clangd::{ClangdConfigBuilder, ClangdSession, ClangdSessionBuilder};
use crate::project::{ProjectError, ProjectWorkspace};

/// Manages ClangdSession instances for a project workspace
///
/// `WorkspaceSession` provides pure session lifecycle management, handling the creation,
/// reuse, and cleanup of ClangdSession instances for different build directories.
/// Build directory resolution policy is handled by the caller (typically the server layer).
pub struct WorkspaceSession {
    /// Project workspace for determining project root
    workspace: ProjectWorkspace,
    /// Map of build directories to their ClangdSession instances
    sessions: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<ClangdSession>>>>>,
}

impl WorkspaceSession {
    /// Create a new WorkspaceSession for the given project workspace
    #[allow(dead_code)]
    pub fn new(workspace: ProjectWorkspace) -> Self {
        Self {
            workspace,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get or create a ClangdSession for the specified build directory
    ///
    /// This method provides lazy initialization - sessions are created only when needed
    /// and reused for subsequent requests to the same build directory.
    ///
    /// # Arguments
    /// * `build_dir` - The build directory path (must contain compile_commands.json)
    ///
    /// # Returns
    /// * `Ok(Arc<Mutex<ClangdSession>>)` - Shared session for the build directory
    /// * `Err(ProjectError)` - If session creation fails
    #[instrument(name = "workspace_session_get_or_create", skip(self))]
    #[allow(dead_code)]
    pub async fn get_or_create_session(
        &self,
        build_dir: PathBuf,
    ) -> Result<Arc<Mutex<ClangdSession>>, ProjectError> {
        let mut sessions = self.sessions.lock().await;

        // Check if we already have a session for this build directory
        if let Some(session) = sessions.get(&build_dir) {
            info!(
                "Reusing existing ClangdSession for build dir: {}",
                build_dir.display()
            );
            return Ok(Arc::clone(session));
        }

        // Create a new session for this build directory
        info!(
            "Creating new ClangdSession for build dir: {}",
            build_dir.display()
        );

        // Determine project root from workspace
        let project_root = if self.workspace.project_root_path.exists() {
            self.workspace.project_root_path.clone()
        } else {
            std::env::current_dir().map_err(|e| {
                ProjectError::SessionCreation(format!("Failed to get current directory: {}", e))
            })?
        };

        // Build configuration using v2 builder pattern
        let config = ClangdConfigBuilder::new()
            .working_directory(project_root)
            .build_directory(build_dir.clone())
            .build()
            .map_err(|e| ProjectError::SessionCreation(format!("Failed to build config: {}", e)))?;

        // Create session using builder
        let session = ClangdSessionBuilder::new()
            .with_config(config)
            .build()
            .await
            .map_err(|e| {
                ProjectError::SessionCreation(format!("Failed to create session: {}", e))
            })?;

        // Wrap in Arc<Mutex> for sharing
        let session_arc = Arc::new(Mutex::new(session));

        // Store the session for future reuse
        sessions.insert(build_dir.clone(), Arc::clone(&session_arc));

        Ok(session_arc)
    }

    /// Get the number of active sessions
    #[allow(dead_code)]
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.lock().await;
        sessions.len()
    }

    /// Check if a session exists for the given build directory
    #[allow(dead_code)]
    pub async fn has_session(&self, build_dir: &PathBuf) -> bool {
        let sessions = self.sessions.lock().await;
        sessions.contains_key(build_dir)
    }

    /// Get the project workspace reference
    #[allow(dead_code)]
    pub fn workspace(&self) -> &ProjectWorkspace {
        &self.workspace
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_workspace() -> ProjectWorkspace {
        ProjectWorkspace::new(PathBuf::from("/test/project"), vec![], 2)
    }

    #[tokio::test]
    async fn test_workspace_session_creation() {
        let workspace = create_test_workspace();
        let session_manager = WorkspaceSession::new(workspace);

        assert_eq!(session_manager.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_session_count() {
        let workspace = create_test_workspace();
        let session_manager = WorkspaceSession::new(workspace);

        assert_eq!(session_manager.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_has_session() {
        let workspace = create_test_workspace();
        let session_manager = WorkspaceSession::new(workspace);
        let build_dir = PathBuf::from("/test/build");

        assert!(!session_manager.has_session(&build_dir).await);
    }

    #[test]
    fn test_workspace_access() {
        let workspace = create_test_workspace();
        let session_manager = WorkspaceSession::new(workspace);

        assert_eq!(
            session_manager.workspace().project_root_path,
            PathBuf::from("/test/project")
        );
    }
}
