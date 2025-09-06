//! Component session management
//!
//! Provides `ComponentSession` for managing ClangdSession and ComponentIndexMonitor
//! instances for a single project component. This module encapsulates the lifecycle
//! and operations for a specific build directory and its associated resources.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};

use crate::clangd::config::DEFAULT_WORKSPACE_SYMBOL_LIMIT;
use crate::clangd::file_manager::ClangdFileManager;
use crate::clangd::index::IndexLatch;
use crate::clangd::session::ClangdSessionTrait;
use crate::clangd::version::ClangdVersion;
use crate::clangd::{ClangdConfigBuilder, ClangdSession, ClangdSessionBuilder};
use crate::io::file_system::RealFileSystem;
use crate::project::index::reader::{IndexReader, IndexReaderTrait};
use crate::project::index::storage::IndexStorage;
use crate::project::index::storage::filesystem::FilesystemIndexStorage;
use crate::project::index::{ClangdIndexTrigger, ComponentIndexMonitor, ComponentIndexState};
use crate::project::{ProjectComponent, ProjectError};

/// Channel buffer size for progress event processing
const PROGRESS_CHANNEL_BUFFER_SIZE: usize = 10_000;

/// Manages ClangdSession and ComponentIndexMonitor for a single project component
///
/// `ComponentSession` encapsulates all resources needed for a specific build directory,
/// including the clangd session, index monitoring, and component-specific operations.
/// This provides a cleaner abstraction for component lifecycle management.
pub struct ComponentSession {
    /// Build directory for this component
    build_dir: PathBuf,
    /// ClangdSession for LSP communication (wrapped for background task access)
    clangd_session: Arc<tokio::sync::Mutex<ClangdSession>>,
    /// File manager for tracking open files and coordinating with LSP client
    file_manager: Arc<tokio::sync::Mutex<ClangdFileManager>>,
    /// ComponentIndexMonitor for index state tracking
    index_monitor: Arc<ComponentIndexMonitor>,
    /// Component metadata
    #[allow(dead_code)]
    component: ProjectComponent,
}

impl ComponentSession {
    /// Create a new ComponentSession with all required initialization
    ///
    /// # Arguments
    /// * `component` - The project component this session represents
    /// * `clangd_path` - Path to the clangd executable
    /// * `clangd_version` - Detected clangd version information
    /// * `project_root` - Project root directory for clangd working directory
    ///
    /// # Returns
    /// * `Ok(ComponentSession)` - Successfully created component session
    /// * `Err(ProjectError)` - If session creation fails
    #[instrument(name = "component_session_new", skip(component, clangd_version))]
    pub async fn new(
        component: ProjectComponent,
        clangd_path: &str,
        clangd_version: &ClangdVersion,
        project_root: PathBuf,
    ) -> Result<Self, ProjectError> {
        info!(
            "Creating ComponentSession for build dir: {}",
            component.build_dir_path.display()
        );

        // Build configuration using builder pattern
        let config = ClangdConfigBuilder::new()
            .working_directory(project_root)
            .build_directory(component.build_dir_path.clone())
            .clangd_path(clangd_path.to_string())
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

        // Wrap in Arc<Mutex> for sharing with background tasks
        let clangd_session = Arc::new(tokio::sync::Mutex::new(session));

        // Create file manager for this component
        let file_manager = Arc::new(tokio::sync::Mutex::new(ClangdFileManager::new()));

        // Create ComponentIndexMonitor for this component
        let index_monitor = Self::create_index_monitor(
            &component,
            clangd_version,
            Arc::clone(&clangd_session),
            Arc::clone(&file_manager),
        )
        .await?;

        // Launch background processor for progress events
        let monitor_clone = Arc::clone(&index_monitor);
        tokio::spawn(async move {
            while let Some(event) = progress_rx.recv().await {
                monitor_clone.handle_progress_event(event).await;
            }
        });

        debug!(
            "ComponentSession created successfully for build dir: {}",
            component.build_dir_path.display()
        );

        Ok(Self {
            build_dir: component.build_dir_path.clone(),
            clangd_session,
            file_manager,
            index_monitor,
            component,
        })
    }

    /// Create a ComponentIndexMonitor for the component
    async fn create_index_monitor(
        component: &ProjectComponent,
        clangd_version: &ClangdVersion,
        session: Arc<tokio::sync::Mutex<ClangdSession>>,
        file_manager: Arc<tokio::sync::Mutex<ClangdFileManager>>,
    ) -> Result<Arc<ComponentIndexMonitor>, ProjectError> {
        let build_dir = &component.build_dir_path;

        // Create index reader with filesystem storage
        let index_directory = build_dir.join(".cache/clangd/index");

        // Use the centralized version mapping from ClangdVersion
        let expected_version = clangd_version.index_format_version();

        let storage: Arc<dyn IndexStorage> = Arc::new(FilesystemIndexStorage::new(
            index_directory,
            expected_version,
            RealFileSystem,
        ));

        let index_reader: Arc<dyn IndexReaderTrait> =
            Arc::new(IndexReader::new(storage, clangd_version.clone()));

        // Create IndexTrigger from the provided clangd session and file manager
        let index_trigger = Arc::new(ClangdIndexTrigger::new(session, file_manager));

        // Create new ComponentIndexMonitor with IndexTrigger
        let monitor = ComponentIndexMonitor::new_with_trigger(
            build_dir.to_path_buf(),
            &component.compilation_database,
            index_reader,
            clangd_version,
            Some(index_trigger),
        )
        .await?;

        // Trigger initial indexing using the ComponentIndexMonitor
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

        debug!(
            "Created ComponentIndexMonitor for build dir: {}",
            build_dir.display()
        );

        Ok(monitor_arc)
    }

    /// Ensure a file is ready for LSP operations
    ///
    /// This will open the file if not already open, or send a change notification
    /// if the file has been modified on disk since it was opened.
    pub async fn ensure_file_ready(&self, path: &std::path::Path) -> Result<(), ProjectError> {
        let mut session = self.clangd_session.lock().await;
        let mut file_manager = self.file_manager.lock().await;

        file_manager
            .ensure_file_ready(path, session.client_mut())
            .await
            .map_err(|e| ProjectError::SessionCreation(format!("File management failed: {}", e)))
    }

    /// Get mutable access to the LSP session
    ///
    /// This is the primary interface for LSP operations. Use `ensure_file_ready()`
    /// first if you need to open files, then call `.client_mut()` on the returned guard.
    pub async fn lsp_session(&self) -> tokio::sync::MutexGuard<'_, ClangdSession> {
        self.clangd_session.lock().await
    }

    /// Get the ComponentIndexMonitor for this component
    #[allow(dead_code)]
    pub fn index_monitor(&self) -> Arc<ComponentIndexMonitor> {
        Arc::clone(&self.index_monitor)
    }

    /// Get the build directory for this component
    pub fn build_dir(&self) -> &PathBuf {
        &self.build_dir
    }

    /// Get the project component metadata
    #[allow(dead_code)]
    pub fn component(&self) -> &ProjectComponent {
        &self.component
    }

    /// Wait for indexing completion before proceeding with LSP operations
    ///
    /// This method waits for clangd to complete indexing and ensures that all files
    /// in the compilation database have been indexed. This is what tools need to
    /// call before making LSP requests to ensure accurate results.
    pub async fn ensure_indexed(&self, timeout: Duration) -> Result<(), ProjectError> {
        self.wait_for_indexing_completion(timeout).await
    }

    /// Get component indexing state
    pub async fn get_index_state(&self) -> ComponentIndexState {
        self.index_monitor.get_component_state().await
    }

    /// Wait for indexing completion with timeout
    ///
    /// This method waits for clangd to complete indexing and ensures that all files
    /// in the compilation database have been indexed. If coverage is incomplete after
    /// initial indexing, it will trigger indexing for unindexed files.
    pub async fn wait_for_indexing_completion(
        &self,
        timeout: Duration,
    ) -> Result<(), ProjectError> {
        info!(
            "Waiting for indexing completion for build dir: {} (timeout: {:?})",
            self.build_dir.display(),
            timeout
        );

        // Wait for completion using ComponentIndexMonitor
        self.index_monitor.wait_for_completion(timeout).await?;

        Ok(())
    }

    /// Refresh index state by synchronizing with actual index files on disk
    ///
    /// This method reads the current state of index files and updates the IndexState
    /// to reflect staleness and availability of actual index data.
    #[allow(dead_code)]
    pub async fn refresh_index_state(&self) -> Result<(), ProjectError> {
        self.index_monitor.refresh_from_disk().await
    }

    /// Get latch for this component to wait for indexing completion
    #[allow(dead_code)]
    pub async fn get_index_latch(&self) -> IndexLatch {
        self.index_monitor.get_completion_latch().await
    }
}
