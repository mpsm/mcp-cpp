//! Workspace session management
//!
//! Provides `WorkspaceSession` for managing ClangdSession instances across different
//! build directories within a project workspace. This module handles pure session
//! lifecycle management without build directory resolution policy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

use crate::clangd::config::DEFAULT_WORKSPACE_SYMBOL_LIMIT;
use crate::clangd::index::IndexLatch;
use crate::clangd::version::ClangdVersion;
use crate::clangd::{ClangdConfigBuilder, ClangdSession, ClangdSessionBuilder};
use crate::io::file_system::RealFileSystem;
use crate::project::index::reader::{IndexReader, IndexReaderTrait};
use crate::project::index::storage::IndexStorage;
use crate::project::index::storage::filesystem::FilesystemIndexStorage;
use crate::project::index::{ClangdIndexTrigger, ComponentIndexMonitor, ComponentIndexState};
use crate::project::{ProjectError, ProjectWorkspace};
use tokio::sync::mpsc;

/// Channel buffer size for progress event processing
const PROGRESS_CHANNEL_BUFFER_SIZE: usize = 10_000;

/// Manages ClangdSession instances for a project workspace with index tracking
///
/// `WorkspaceSession` provides pure session lifecycle management, handling the creation,
/// reuse, and cleanup of ClangdSession instances for different build directories.
/// Index monitoring is delegated to ComponentIndexMonitor instances for better encapsulation.
/// Build directory resolution policy is handled by the caller (typically the server layer).
pub struct WorkspaceSession {
    /// Project workspace for determining project root
    workspace: ProjectWorkspace,
    /// Map of build directories to their ClangdSession instances
    sessions: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<ClangdSession>>>>>,
    /// Path to clangd executable
    clangd_path: String,
    /// Component index monitors for each build directory
    index_monitors: Arc<Mutex<HashMap<PathBuf, Arc<ComponentIndexMonitor>>>>,
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
            index_monitors: Arc::new(Mutex::new(HashMap::new())),
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
            .add_arg("--log=verbose")
            .build()
            .map_err(|e| ProjectError::SessionCreation(format!("Failed to build config: {}", e)))?;

        // Initialize progress event channel for index state tracking
        let (progress_tx, mut progress_rx) = mpsc::channel(PROGRESS_CHANNEL_BUFFER_SIZE);

        // Construct ClangdSession with progress event integration
        let session = ClangdSessionBuilder::new()
            .with_config(config)
            .with_progress_sender(progress_tx)
            .build()
            .await
            .map_err(|e| {
                ProjectError::SessionCreation(format!("Failed to create session: {}", e))
            })?;

        // Wrap in Arc<Mutex> for sharing
        let session_arc = Arc::new(Mutex::new(session));

        // Create ComponentIndexMonitor for this build directory
        let component_monitor = self
            .create_component_monitor(&build_dir, Arc::clone(&session_arc))
            .await?;

        // Launch background processor for progress events
        let monitor_clone = Arc::clone(&component_monitor);
        tokio::spawn(async move {
            while let Some(event) = progress_rx.recv().await {
                monitor_clone.handle_progress_event(event).await;
            }
        });

        // Store the session for future reuse
        sessions.insert(build_dir.clone(), Arc::clone(&session_arc));

        // Drop sessions lock before index operations
        drop(sessions);

        Ok(session_arc)
    }

    /// Get an existing ComponentIndexMonitor for the specified build directory
    pub async fn get_component_monitor(
        &self,
        build_dir: &Path,
    ) -> Option<Arc<ComponentIndexMonitor>> {
        let monitors = self.index_monitors.lock().await;
        monitors.get(build_dir).map(Arc::clone)
    }

    /// Create a ComponentIndexMonitor for the specified build directory
    /// This is called internally during session creation and should not be used directly
    async fn create_component_monitor(
        &self,
        build_dir: &Path,
        session: Arc<Mutex<ClangdSession>>,
    ) -> Result<Arc<ComponentIndexMonitor>, ProjectError> {
        // Get the component for this build directory
        let component = self
            .workspace
            .get_component_by_build_dir(&build_dir.to_path_buf())
            .ok_or_else(|| {
                ProjectError::SessionCreation("Component not found for build directory".to_string())
            })?;

        // Create index reader with filesystem storage
        let index_directory = build_dir.join(".cache/clangd/index");

        // Use the centralized version mapping from ClangdVersion
        let expected_version = self.clangd_version.index_format_version();

        let storage: Arc<dyn IndexStorage> = Arc::new(FilesystemIndexStorage::new(
            index_directory,
            expected_version,
            RealFileSystem,
        ));

        let index_reader: Arc<dyn IndexReaderTrait> =
            Arc::new(IndexReader::new(storage, self.clangd_version.clone()));

        // Create IndexTrigger from the provided clangd session
        let index_trigger = Arc::new(ClangdIndexTrigger::new(session));

        // Create new ComponentIndexMonitor with IndexTrigger
        let monitor = ComponentIndexMonitor::new_with_trigger(
            build_dir.to_path_buf(),
            &component.compilation_database,
            index_reader,
            &self.clangd_version,
            Some(index_trigger),
        )
        .await?;

        // Trigger initial indexing using the ComponentIndexMonitor
        // This replaces the manual ensure_file_ready call that was in get_or_create_session
        if let Err(e) = monitor
            .trigger_initial_indexing(&component.compilation_database)
            .await
        {
            warn!(
                "Failed to trigger initial indexing for {}: {}",
                build_dir.display(),
                e
            );
        }

        let monitor_arc = Arc::new(monitor);

        // Store the monitor for future retrieval
        let mut monitors = self.index_monitors.lock().await;
        monitors.insert(build_dir.to_path_buf(), Arc::clone(&monitor_arc));

        debug!(
            "Created ComponentIndexMonitor for build dir: {}",
            build_dir.display()
        );

        Ok(monitor_arc)
    }

    /// Get indexing coverage for a build directory (0.0 to 1.0)
    #[allow(dead_code)]
    pub async fn get_indexing_coverage(&self, build_dir: &Path) -> Option<f32> {
        // Get the ComponentIndexMonitor - if none exists, session wasn't created
        match self.get_component_monitor(build_dir).await {
            Some(monitor) => Some(monitor.get_coverage().await),
            None => None,
        }
    }

    /// Get component indexing state for a build directory
    #[allow(dead_code)]
    pub async fn get_component_index_state(&self, build_dir: &Path) -> Option<ComponentIndexState> {
        // Get the ComponentIndexMonitor - if none exists, session wasn't created
        match self.get_component_monitor(build_dir).await {
            Some(monitor) => Some(monitor.get_component_state().await),
            None => None,
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

    /// Wait for indexing completion with coverage assurance using custom timeout
    ///
    /// This method waits for clangd to complete indexing and ensures that all files
    /// in the compilation database have been indexed. If coverage is incomplete after
    /// initial indexing, it will trigger indexing for unindexed files.
    #[allow(dead_code)]
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

        // Get the ComponentIndexMonitor - if none exists, session wasn't created
        let monitor = self.get_component_monitor(build_dir).await.ok_or_else(|| {
            ProjectError::SessionCreation(
                "ComponentIndexMonitor not found - session not created".to_string(),
            )
        })?;

        // Wait for completion using ComponentIndexMonitor
        monitor.wait_for_completion(timeout).await?;

        Ok(())
    }

    /// Refresh index state by synchronizing with actual index files on disk
    ///
    /// This method reads the current state of index files and updates the IndexState
    /// to reflect staleness and availability of actual index data.
    #[allow(dead_code)]
    pub async fn refresh_index_state(&self, build_dir: &Path) -> Result<(), ProjectError> {
        // Get the ComponentIndexMonitor - if none exists, session wasn't created
        let monitor = self.get_component_monitor(build_dir).await.ok_or_else(|| {
            ProjectError::SessionCreation(
                "ComponentIndexMonitor not found - session not created".to_string(),
            )
        })?;

        monitor.refresh_from_disk().await
    }

    /// Get latch for a build directory to wait for indexing completion
    #[allow(dead_code)]
    pub async fn get_index_latch(&self, build_dir: &Path) -> Option<IndexLatch> {
        // Get the ComponentIndexMonitor - if none exists, session wasn't created
        match self.get_component_monitor(build_dir).await {
            Some(monitor) => Some(monitor.get_completion_latch().await),
            None => None,
        }
    }
}

impl Drop for WorkspaceSession {
    fn drop(&mut self) {
        // Clear the sessions HashMap to drop all Arc references
        // This allows ClangdSession::drop() to be called for proper cleanup
        if let Ok(mut sessions) = self.sessions.try_lock() {
            sessions.clear();
        }
        // Clear the index monitors HashMap
        if let Ok(mut monitors) = self.index_monitors.try_lock() {
            monitors.clear();
        }
        // ComponentIndexMonitor will be cleaned up automatically
    }
}
