//! Workspace session management
//!
//! Provides `WorkspaceSession` for managing ClangdSession instances across different
//! build directories within a project workspace. This module handles pure session
//! lifecycle management without build directory resolution policy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

use crate::clangd::config::DEFAULT_WORKSPACE_SYMBOL_LIMIT;
use crate::clangd::version::ClangdVersion;
use crate::clangd::{ClangdConfigBuilder, ClangdSession, ClangdSessionBuilder};
use crate::io::file_system::RealFileSystem;
use crate::project::index::reader::IndexReader;
use crate::project::index::state::IndexState;
use crate::project::index::storage::IndexStorage;
use crate::project::index::storage::filesystem::FilesystemIndexStorage;
use crate::project::{ProjectError, ProjectWorkspace};

/// Manages ClangdSession instances for a project workspace with index tracking
///
/// `WorkspaceSession` provides pure session lifecycle management, handling the creation,
/// reuse, and cleanup of ClangdSession instances for different build directories.
/// It also tracks indexing progress and provides access to compilation database indexing status.
/// Build directory resolution policy is handled by the caller (typically the server layer).
pub struct WorkspaceSession {
    /// Project workspace for determining project root
    workspace: ProjectWorkspace,
    /// Map of build directories to their ClangdSession instances
    sessions: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<ClangdSession>>>>>,
    /// Path to clangd executable
    clangd_path: String,
    /// Index state tracking for each build directory
    index_states: Arc<Mutex<HashMap<PathBuf, IndexState>>>,
    /// Index readers for each build directory
    index_readers: Arc<Mutex<HashMap<PathBuf, IndexReader>>>,
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
            sessions: Arc::new(Mutex::new(HashMap::new())),
            clangd_path,
            index_states: Arc::new(Mutex::new(HashMap::new())),
            index_readers: Arc::new(Mutex::new(HashMap::new())),
            clangd_version,
        })
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
            .add_arg("--query-driver=**")
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

        // Initialize index components for this build directory
        self.initialize_index_components(&build_dir).await?;

        Ok(session_arc)
    }

    /// Initialize index components for a build directory
    async fn initialize_index_components(&self, build_dir: &Path) -> Result<(), ProjectError> {
        let component = self
            .workspace
            .get_component_by_build_dir(&build_dir.to_path_buf())
            .ok_or_else(|| {
                ProjectError::SessionCreation("Component not found for build directory".to_string())
            })?;

        // Initialize index state from compilation database
        let index_state = IndexState::from_compilation_db(&component.compilation_database)
            .map_err(|e| {
                ProjectError::SessionCreation(format!("Failed to create index state: {}", e))
            })?;

        // Create filesystem index storage for clangd index files
        let index_directory = build_dir.join(".cache/clangd/index");

        // Determine expected index version based on clangd version
        let expected_version = match (self.clangd_version.major, self.clangd_version.minor) {
            (14..=17, _) => 17, // Clangd 14-17 use index format v17
            (18..=19, _) => 19, // Clangd 18-19 use index format v19
            _ => 19,            // Default to latest known format
        };

        let storage: Arc<dyn IndexStorage> = Arc::new(FilesystemIndexStorage::new(
            index_directory,
            expected_version,
            RealFileSystem,
        ));

        let index_reader = IndexReader::new(storage, self.clangd_version.clone());

        // Store components
        {
            let mut states = self.index_states.lock().await;
            states.insert(build_dir.to_path_buf(), index_state);
        }

        {
            let mut readers = self.index_readers.lock().await;
            readers.insert(build_dir.to_path_buf(), index_reader);
        }

        debug!(
            "Initialized index components for build dir: {}",
            build_dir.display()
        );
        Ok(())
    }

    /// Get indexing coverage for a build directory (0.0 to 1.0)
    #[allow(dead_code)]
    pub async fn get_indexing_coverage(&self, build_dir: &Path) -> Option<f32> {
        let states = self.index_states.lock().await;
        states.get(build_dir).map(|state| state.coverage())
    }
}
