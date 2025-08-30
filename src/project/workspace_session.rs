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
use tracing::{debug, info, instrument, trace, warn};

use crate::clangd::config::DEFAULT_WORKSPACE_SYMBOL_LIMIT;
use crate::clangd::index::ProgressEvent;
use crate::clangd::version::ClangdVersion;
use crate::clangd::{ClangdConfigBuilder, ClangdSession, ClangdSessionBuilder};
use crate::io::file_system::RealFileSystem;
use crate::project::index::reader::IndexReader;
use crate::project::index::state::IndexState;
use crate::project::index::storage::IndexStorage;
use crate::project::index::storage::filesystem::FilesystemIndexStorage;
use crate::project::{ProjectError, ProjectWorkspace};
use tokio::sync::mpsc;

/// Channel buffer size for progress event processing
const PROGRESS_CHANNEL_BUFFER_SIZE: usize = 10_000;

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

        // Initialize IndexState for all components to enable immediate coverage reporting
        let mut index_states = HashMap::new();
        for component in &workspace.components {
            let index_state = IndexState::from_compilation_db(&component.compilation_database)
                .map_err(|e| {
                    ProjectError::SessionCreation(format!(
                        "Failed to create index state for {}: {}",
                        component.build_dir_path.display(),
                        e
                    ))
                })?;

            index_states.insert(component.build_dir_path.clone(), index_state);
        }

        Ok(Self {
            workspace,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            clangd_path,
            index_states: Arc::new(Mutex::new(index_states)),
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
            .add_arg("--log=verbose")
            .build()
            .map_err(|e| ProjectError::SessionCreation(format!("Failed to build config: {}", e)))?;

        // Initialize progress event channel for index state tracking
        let (progress_tx, mut progress_rx) = mpsc::channel(PROGRESS_CHANNEL_BUFFER_SIZE);

        // Launch background processor for progress events
        let index_states = Arc::clone(&self.index_states);
        let build_dir_clone = build_dir.clone();
        tokio::spawn(async move {
            while let Some(event) = progress_rx.recv().await {
                Self::handle_progress_event_async(
                    Arc::clone(&index_states),
                    build_dir_clone.clone(),
                    event,
                )
                .await;
            }
        });

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

        // Initialize index components for this build directory first
        self.initialize_index_components(&build_dir).await?;

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

    /// Initialize index components for a build directory
    async fn initialize_index_components(&self, build_dir: &Path) -> Result<(), ProjectError> {
        // Create IndexReader for build directory (IndexState initialized during construction)

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

        // Register IndexReader for build directory operations
        {
            let mut readers = self.index_readers.lock().await;
            readers.insert(build_dir.to_path_buf(), index_reader);
        }

        debug!(
            "Initialized index reader for build dir: {}",
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

    /// Handle progress events asynchronously with proper error handling
    async fn handle_progress_event_async(
        index_states: Arc<Mutex<HashMap<PathBuf, IndexState>>>,
        build_dir: PathBuf,
        event: ProgressEvent,
    ) {
        let Ok(mut states) = index_states.try_lock() else {
            warn!("Could not acquire lock on index states to handle progress event");
            return;
        };

        let Some(index_state) = states.get_mut(&build_dir) else {
            warn!(
                "No index state found for build directory: {}",
                build_dir.display()
            );
            return;
        };

        match event {
            ProgressEvent::FileIndexingStarted { path, .. } => {
                debug!("File indexing started: {:?}", path);
                index_state.mark_indexing(&path);
            }
            ProgressEvent::FileIndexingCompleted {
                path,
                symbols,
                refs,
            } => {
                debug!(
                    "File indexing completed: {:?} ({} symbols, {} refs)",
                    path, symbols, refs
                );
                index_state.mark_indexed(&path);
            }
            ProgressEvent::StandardLibraryStarted {
                stdlib_version,
                context_file,
            } => {
                debug!(
                    "Standard library indexing started: {} (context: {:?})",
                    stdlib_version, context_file
                );
            }
            ProgressEvent::StandardLibraryCompleted { symbols, filtered } => {
                debug!(
                    "Standard library indexing completed: {} symbols, {} filtered",
                    symbols, filtered
                );
            }
            ProgressEvent::OverallProgress {
                current,
                total,
                percentage,
                message,
            } => {
                debug!(
                    "Overall indexing progress: {}/{} ({}%) - {:?}",
                    current, total, percentage, message
                );
            }
            ProgressEvent::OverallCompleted => {
                info!(
                    "Overall indexing completed for build directory: {}",
                    build_dir.display()
                );
            }
            ProgressEvent::IndexingFailed { error } => {
                warn!(
                    "Indexing failed for build directory {}: {}",
                    build_dir.display(),
                    error
                );
            }
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
        let start_time = tokio::time::Instant::now();

        let session = self.get_session_for_build_dir(build_dir).await?;
        info!(
            "Waiting for indexing completion for build dir: {} (timeout: {:?})",
            build_dir.display(),
            timeout
        );

        // Wait for initial indexing completion
        self.wait_for_initial_indexing(&session, timeout).await?;

        // Ensure complete coverage
        self.ensure_complete_indexing(build_dir, &session, start_time, timeout)
            .await?;

        Ok(())
    }

    /// Get session for build directory
    async fn get_session_for_build_dir(
        &self,
        build_dir: &Path,
    ) -> Result<Arc<Mutex<ClangdSession>>, ProjectError> {
        let sessions = self.sessions.lock().await;
        sessions.get(build_dir).cloned().ok_or_else(|| {
            ProjectError::SessionNotFound(format!(
                "No session found for build directory: {}",
                build_dir.display()
            ))
        })
    }

    /// Wait for initial indexing to complete with custom timeout
    async fn wait_for_initial_indexing(
        &self,
        session: &Arc<Mutex<ClangdSession>>,
        timeout: Duration,
    ) -> Result<(), ProjectError> {
        let session_guard = session.lock().await;
        session_guard
            .index_monitor()
            .wait_for_indexing_completion_with_timeout(timeout)
            .await
            .map_err(|e| {
                ProjectError::IndexingTimeout(format!("Initial indexing wait failed: {}", e))
            })
    }

    /// Get unindexed files for a build directory  
    async fn get_unindexed_files(&self, build_dir: &Path) -> Vec<PathBuf> {
        let states = self.index_states.lock().await;
        states
            .get(build_dir)
            .map(|state| state.get_unindexed_files())
            .unwrap_or_default()
    }

    /// Ensure complete indexing by triggering additional files if needed
    async fn ensure_complete_indexing(
        &self,
        build_dir: &Path,
        session: &Arc<Mutex<ClangdSession>>,
        start_time: tokio::time::Instant,
        timeout_duration: Duration,
    ) -> Result<(), ProjectError> {
        loop {
            if start_time.elapsed() > timeout_duration {
                return Err(ProjectError::IndexingTimeout(
                    "Indexing completion timeout exceeded".to_string(),
                ));
            }

            let unindexed_files = self.get_unindexed_files(build_dir).await;

            if unindexed_files.is_empty() {
                info!(
                    "Indexing completion achieved: all files indexed for {}",
                    build_dir.display()
                );
                return Ok(());
            }

            debug!("Found {} unindexed files remaining", unindexed_files.len());

            if let Some(file) = unindexed_files.first() {
                self.trigger_file_indexing(session, file).await?;
            }
        }
    }

    /// Trigger indexing for a specific file
    async fn trigger_file_indexing(
        &self,
        session: &Arc<Mutex<ClangdSession>>,
        file: &Path,
    ) -> Result<(), ProjectError> {
        debug!("Triggering indexing for unindexed file: {:?}", file);

        let mut session_guard = session.lock().await;
        if let Err(e) = session_guard.ensure_file_ready(file).await {
            warn!("Failed to trigger indexing for file {:?}: {}", file, e);
        } else {
            // Apply reduced timeout for individual file indexing operations
            let file_timeout = Duration::from_secs(60);
            session_guard
                .index_monitor()
                .wait_for_indexing_completion_with_timeout(file_timeout)
                .await
                .map_err(|e| {
                    ProjectError::IndexingTimeout(format!("File indexing wait failed: {}", e))
                })?;
        }

        Ok(())
    }

    /// Refresh index state by synchronizing with actual index files on disk
    ///
    /// This method reads the current state of index files and updates the IndexState
    /// to reflect staleness and availability of actual index data.
    #[allow(dead_code)]
    pub async fn refresh_index_state(&self, build_dir: &Path) -> Result<(), ProjectError> {
        debug!(
            "Refreshing index state for build dir: {}",
            build_dir.display()
        );

        let (index_reader, compilation_db_files) = self.get_refresh_dependencies(build_dir).await?;
        self.sync_index_state_with_disk(build_dir, &index_reader, compilation_db_files)
            .await?;

        Ok(())
    }

    /// Get dependencies needed for index refresh
    async fn get_refresh_dependencies(
        &self,
        build_dir: &Path,
    ) -> Result<(IndexReader, Vec<PathBuf>), ProjectError> {
        // Get the index reader for this build directory
        let readers = self.index_readers.lock().await;
        let reader = readers.get(build_dir).cloned().ok_or_else(|| {
            ProjectError::IndexingTimeout(format!(
                "No index reader found for build directory: {}",
                build_dir.display()
            ))
        })?;

        // Get compilation database files
        let component = self
            .workspace
            .get_component_by_build_dir(&build_dir.to_path_buf())
            .ok_or_else(|| {
                ProjectError::SessionCreation("Component not found for build directory".to_string())
            })?;

        let files = component
            .compilation_database
            .source_files()
            .into_iter()
            .map(|p| p.to_path_buf())
            .collect::<Vec<_>>();

        Ok((reader, files))
    }

    /// Synchronize IndexState with actual index files on disk
    async fn sync_index_state_with_disk(
        &self,
        build_dir: &Path,
        index_reader: &IndexReader,
        compilation_db_files: Vec<PathBuf>,
    ) -> Result<(), ProjectError> {
        let mut states = self.index_states.lock().await;
        let Some(index_state) = states.get_mut(build_dir) else {
            return Err(ProjectError::SessionCreation(
                "No index state found for build directory".to_string(),
            ));
        };

        for file_path in compilation_db_files {
            self.update_file_state_from_index(index_state, index_reader, &file_path)
                .await;
        }

        index_state.refresh();
        debug!(
            "Index state refreshed. Coverage: {:.1}%",
            index_state.coverage() * 100.0
        );

        Ok(())
    }

    /// Update individual file state based on index file
    async fn update_file_state_from_index(
        &self,
        index_state: &mut IndexState,
        index_reader: &IndexReader,
        file_path: &Path,
    ) {
        match index_reader.read_index_for_file(file_path).await {
            Ok(index_entry) => {
                use crate::project::index::reader::FileIndexStatus;
                match index_entry.status {
                    FileIndexStatus::Done => index_state.mark_indexed(file_path),
                    FileIndexStatus::Stale => index_state.mark_stale(file_path),
                    FileIndexStatus::InProgress => index_state.mark_indexing(file_path),
                    FileIndexStatus::None | FileIndexStatus::Invalid(_) => {
                        // File not indexed or invalid - leave as None in state
                    }
                }
            }
            Err(e) => {
                trace!("Could not read index for file {:?}: {}", file_path, e);
                // File likely not indexed yet
            }
        }
    }
}
