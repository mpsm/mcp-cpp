//! Workspace session management
//!
//! Provides `WorkspaceSession` for managing ClangdSession instances across different
//! build directories within a project workspace. This module handles pure session
//! lifecycle management without build directory resolution policy.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

use crate::clangd::config::DEFAULT_WORKSPACE_SYMBOL_LIMIT;
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
    /// Path to clangd executable
    clangd_path: String,
}

impl WorkspaceSession {
    /// Create a new WorkspaceSession for the given project workspace
    pub fn new(workspace: ProjectWorkspace, clangd_path: String) -> Self {
        Self {
            workspace,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            clangd_path,
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

        // Build configuration using v2 builder pattern with clangd path
        let config = ClangdConfigBuilder::new()
            .working_directory(project_root)
            .build_directory(build_dir.clone())
            .clangd_path(self.clangd_path.clone())
            .add_arg(format!(
                "--limit-results={}",
                DEFAULT_WORKSPACE_SYMBOL_LIMIT
            ))
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

        // Trigger clangd indexing by opening a representative source file.
        // clangd requires at least one textDocument/didOpen to initiate background indexing.
        // Without this, clangd remains idle and subsequent symbol queries return empty results.
        let component = self
            .workspace
            .get_component_by_build_dir(&build_dir)
            .ok_or_else(|| {
                ProjectError::SessionCreation("Component not found for build directory".to_string())
            })?;

        let source_files = component.compilation_database.source_files();
        if let Some(&first_file) = source_files.first() {
            debug!(
                "Triggering indexing by opening first source file: {:?}",
                first_file
            );
            let mut session_guard = session_arc.lock().await;
            if let Err(e) = session_guard.ensure_file_ready(first_file).await {
                warn!(
                    "Failed to open first source file to trigger indexing: {}",
                    e
                );
            }
        } else {
            warn!("No source files found in compilation database - indexing may not start");
        }

        // Store the session for future reuse
        sessions.insert(build_dir.clone(), Arc::clone(&session_arc));

        Ok(session_arc)
    }
}
