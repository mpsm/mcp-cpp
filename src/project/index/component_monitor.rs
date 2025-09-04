//! Component index monitor for managing all index-related state for a single build directory
//!
//! This module provides ComponentIndexMonitor which consolidates index state management,
//! progress tracking, and completion coordination for individual project components.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, trace, warn};

use crate::clangd::index::{ComponentIndex, IndexLatch, ProgressEvent};
use crate::clangd::version::ClangdVersion;
use crate::project::index::reader::IndexReaderTrait;
use crate::project::index::trigger::IndexTrigger;
use crate::project::{CompilationDatabase, ProjectError};

/// Component indexing states
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentIndexingState {
    /// Initial state - indexing not started
    Init,
    /// Indexing in progress with percentage (0.0 to 100.0)
    InProgress(f32),
    /// Indexing completed but not all CDB files indexed
    Partial,
    /// All CDB files successfully indexed
    Completed,
}

/// High-level component state wrapper for compatibility
#[derive(Debug, Clone)]
pub struct ComponentIndexState {
    /// Current indexing state
    #[allow(dead_code)]
    pub state: ComponentIndexingState,
    /// Total number of CDB files
    #[allow(dead_code)]
    pub total_cdb_files: usize,
    /// Number of CDB files currently indexed
    #[allow(dead_code)]
    pub indexed_cdb_files: usize,
    /// Last updated timestamp
    #[allow(dead_code)]
    pub last_updated: std::time::SystemTime,
}

impl ComponentIndexState {
    /// Create from ComponentIndex for compatibility
    pub fn from_component_index(
        component_index: &ComponentIndex,
        state: ComponentIndexingState,
    ) -> Self {
        Self {
            state,
            total_cdb_files: component_index.total_files_count(),
            indexed_cdb_files: component_index.indexed_count(),
            last_updated: std::time::SystemTime::now(),
        }
    }

    /// Update the indexing state
    #[allow(dead_code)]
    pub fn update_state(&mut self, new_state: ComponentIndexingState) {
        self.state = new_state;
        self.last_updated = std::time::SystemTime::now();
    }

    /// Get current coverage (0.0 to 1.0)
    #[allow(dead_code)]
    pub fn coverage(&self) -> f32 {
        if self.total_cdb_files == 0 {
            1.0
        } else {
            self.indexed_cdb_files as f32 / self.total_cdb_files as f32
        }
    }

    /// Check if indexing is complete (all CDB files indexed)
    #[allow(dead_code)]
    pub fn is_complete(&self) -> bool {
        self.indexed_cdb_files >= self.total_cdb_files
    }
}

/// Consolidated index state for a single component (behind single mutex)
struct IndexMonitorState {
    /// Core index tracking (file status, coverage calculation)
    component_index: ComponentIndex,

    /// Index file reader for disk synchronization
    #[allow(dead_code)]
    index_reader: Arc<dyn IndexReaderTrait>,

    /// Component-level indexing state (Init, InProgress, Partial, Complete)
    current_indexing_state: ComponentIndexingState,

    /// Synchronization latch for completion waiting
    completion_latch: IndexLatch,

    /// Last updated timestamp
    last_updated: std::time::SystemTime,
}

/// Manages all index-related state and operations for a single build directory
///
/// ComponentIndexMonitor consolidates index state management, progress tracking,
/// and completion coordination for individual project components. This eliminates
/// the need for multiple hashmaps in WorkspaceSession and provides better encapsulation.
pub struct ComponentIndexMonitor {
    /// Build directory this monitor tracks
    build_directory: PathBuf,

    /// All index-related state consolidated under single lock
    state: Arc<Mutex<IndexMonitorState>>,

    /// Optional index trigger for initiating indexing operations
    index_trigger: Option<Arc<dyn IndexTrigger>>,
}

impl ComponentIndexMonitor {
    /// Create monitor for specific build directory
    #[allow(dead_code)]
    pub async fn new(
        build_directory: PathBuf,
        compilation_db: &CompilationDatabase,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
    ) -> Result<Self, ProjectError> {
        // Create component index from compilation database (all files start as Pending)
        let component_index = ComponentIndex::new(compilation_db, clangd_version).map_err(|e| {
            ProjectError::SessionCreation(format!(
                "Failed to create component index for {}: {}",
                build_directory.display(),
                e
            ))
        })?;

        // Create completion latch
        let completion_latch = IndexLatch::new();

        let monitor_state = IndexMonitorState {
            component_index,
            index_reader,
            current_indexing_state: ComponentIndexingState::Init,
            completion_latch,
            last_updated: std::time::SystemTime::now(),
        };

        let monitor = Self {
            build_directory,
            state: Arc::new(Mutex::new(monitor_state)),
            index_trigger: None,
        };

        debug!(
            "Created ComponentIndexMonitor for build dir: {}",
            monitor.build_directory.display()
        );

        // Perform initial disk scan to update state from existing index files
        debug!(
            "Performing initial disk scan for build dir: {}",
            monitor.build_directory.display()
        );

        if let Err(e) = monitor.rescan_and_validate_untracked_files().await {
            warn!(
                "Failed to perform initial disk scan for {}: {}",
                monitor.build_directory.display(),
                e
            );
        }

        {
            let state = monitor.state.lock().await;
            debug!(
                "Initial state after disk scan: {}/{} files indexed for {}",
                state.component_index.indexed_count(),
                state.component_index.total_files_count(),
                monitor.build_directory.display()
            );
        }

        Ok(monitor)
    }

    /// Create monitor for specific build directory with optional index trigger
    pub async fn new_with_trigger(
        build_directory: PathBuf,
        compilation_db: &CompilationDatabase,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
        index_trigger: Option<Arc<dyn IndexTrigger>>,
    ) -> Result<Self, ProjectError> {
        // Create component index from compilation database (all files start as Pending)
        let component_index = ComponentIndex::new(compilation_db, clangd_version).map_err(|e| {
            ProjectError::SessionCreation(format!(
                "Failed to create component index for {}: {}",
                build_directory.display(),
                e
            ))
        })?;

        // Create completion latch
        let completion_latch = IndexLatch::new();

        let monitor_state = IndexMonitorState {
            component_index,
            index_reader,
            current_indexing_state: ComponentIndexingState::Init,
            completion_latch,
            last_updated: std::time::SystemTime::now(),
        };

        let monitor = Self {
            build_directory,
            state: Arc::new(Mutex::new(monitor_state)),
            index_trigger,
        };

        debug!(
            "Created ComponentIndexMonitor for build dir: {} with trigger: {}",
            monitor.build_directory.display(),
            monitor.index_trigger.is_some()
        );

        // Perform initial disk scan to update state from existing index files
        debug!(
            "Performing initial disk scan for build dir: {}",
            monitor.build_directory.display()
        );

        if let Err(e) = monitor.rescan_and_validate_untracked_files().await {
            warn!(
                "Failed to perform initial disk scan for {}: {}",
                monitor.build_directory.display(),
                e
            );
        }

        {
            let state = monitor.state.lock().await;
            debug!(
                "Initial state after disk scan: {}/{} files indexed for {}",
                state.component_index.indexed_count(),
                state.component_index.total_files_count(),
                monitor.build_directory.display()
            );
        }

        Ok(monitor)
    }

    /// Create monitor for testing without filesystem dependencies
    #[cfg(test)]
    pub async fn new_for_test(
        build_directory: PathBuf,
        compilation_db: &CompilationDatabase,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
    ) -> Result<Self, ProjectError> {
        // Create component index from compilation database (test version)
        let component_index = ComponentIndex::new_for_test(compilation_db, clangd_version);

        // Create completion latch
        let completion_latch = IndexLatch::new();

        let monitor_state = IndexMonitorState {
            component_index,
            index_reader,
            current_indexing_state: ComponentIndexingState::Init,
            completion_latch,
            last_updated: std::time::SystemTime::now(),
        };

        debug!(
            "Created ComponentIndexMonitor for build dir: {}",
            build_directory.display()
        );

        Ok(Self {
            build_directory,
            state: Arc::new(Mutex::new(monitor_state)),
            index_trigger: None,
        })
    }

    /// Handle progress event (single lock, focused responsibility)
    pub async fn handle_progress_event(&self, event: ProgressEvent) {
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(_) => {
                warn!(
                    "Could not acquire lock on component monitor state for {}",
                    self.build_directory.display()
                );
                return;
            }
        };

        match event {
            ProgressEvent::FileIndexingStarted { path, .. } => {
                debug!("File indexing started: {:?}", path);
                state.component_index.mark_file_in_progress(&path);
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
                state.component_index.mark_file_indexed(&path);

                debug!(
                    "CDB file indexed: {:?} ({}/{})",
                    path,
                    state.component_index.indexed_count(),
                    state.component_index.total_files_count()
                );
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
            ProgressEvent::OverallIndexingStarted => {
                info!(
                    "Overall indexing started for build directory: {}",
                    self.build_directory.display()
                );

                // Transition component state from Init to InProgress
                state.current_indexing_state = ComponentIndexingState::InProgress(0.0);
                state.last_updated = std::time::SystemTime::now();
                debug!(
                    "Component state transitioned to InProgress for {}",
                    self.build_directory.display()
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

                // Update component state with progress percentage
                state.current_indexing_state =
                    ComponentIndexingState::InProgress(percentage as f32);
                state.last_updated = std::time::SystemTime::now();
                trace!(
                    "Component progress updated to {}% for {}",
                    percentage,
                    self.build_directory.display()
                );
            }
            ProgressEvent::OverallCompleted => {
                info!(
                    "Overall indexing completed for build directory: {}",
                    self.build_directory.display()
                );

                // Release the state lock before calling the async rescan method
                drop(state);

                // Critical: Rescan and validate files that were already indexed but not reported by clangd
                // This uses proper IndexReader validation to ensure index files are valid
                debug!(
                    "Starting validation of untracked index files after overall completion for {}",
                    self.build_directory.display()
                );

                if let Err(e) = self.rescan_and_validate_untracked_files().await {
                    warn!(
                        "Failed to rescan untracked index files for {}: {}",
                        self.build_directory.display(),
                        e
                    );
                }

                // Re-acquire state lock for final state determination
                let mut state = match self.state.try_lock() {
                    Ok(state) => state,
                    Err(_) => {
                        warn!(
                            "Could not acquire lock after rescan for {}",
                            self.build_directory.display()
                        );
                        return;
                    }
                };

                // Determine component final state based on CDB coverage AFTER validation
                let component_final_state = if state.component_index.is_fully_indexed() {
                    debug!(
                        "All CDB files indexed ({}/{}), transitioning to Completed",
                        state.component_index.indexed_count(),
                        state.component_index.total_files_count()
                    );
                    ComponentIndexingState::Completed
                } else {
                    debug!(
                        "Partial CDB coverage ({}/{}), transitioning to Partial",
                        state.component_index.indexed_count(),
                        state.component_index.total_files_count()
                    );
                    ComponentIndexingState::Partial
                };

                // Update component state to final state
                state.current_indexing_state = component_final_state;
                state.last_updated = std::time::SystemTime::now();
                info!(
                    "Component state transitioned to {:?} for {} (coverage: {:.1}%)",
                    state.current_indexing_state,
                    self.build_directory.display(),
                    state.component_index.coverage() * 100.0
                );

                // Log detailed indexing summary for diagnostics
                let summary = state.component_index.get_indexing_summary();
                debug!(
                    "Final indexing summary for {}: {} total, {} indexed, {} pending, {} in-progress, {} failed",
                    self.build_directory.display(),
                    summary.total_files,
                    summary.indexed_count,
                    summary.pending_count,
                    summary.in_progress_count,
                    summary.failed_count
                );

                // Trigger latch now that initial indexing has ended (either Partial or Completed)
                let latch = state.completion_latch.clone();
                tokio::spawn(async move {
                    latch.trigger_success().await;
                });
                debug!(
                    "Triggered completion latch for build directory: {} (initial indexing ended)",
                    self.build_directory.display()
                );
            }
            ProgressEvent::IndexingFailed { error } => {
                warn!(
                    "Indexing failed for build directory {}: {}",
                    self.build_directory.display(),
                    error
                );

                // Trigger failure latch
                let latch = state.completion_latch.clone();
                let error_clone = error.clone();
                tokio::spawn(async move {
                    latch.trigger_failure(error_clone).await;
                });
                debug!(
                    "Triggered failure latch for build directory: {}",
                    self.build_directory.display()
                );
            }
        }
    }

    /// Get current component indexing state
    pub async fn get_component_state(&self) -> ComponentIndexState {
        let state = self.state.lock().await;
        ComponentIndexState::from_component_index(
            &state.component_index,
            state.current_indexing_state.clone(),
        )
    }

    /// Get indexing coverage (0.0 to 1.0)
    pub async fn get_coverage(&self) -> f32 {
        let state = self.state.lock().await;
        state.component_index.coverage()
    }

    /// Get comprehensive indexing summary with detailed state information
    #[allow(dead_code)] // Public API for future use
    pub async fn get_indexing_summary(&self) -> crate::clangd::index::IndexingSummary {
        let state = self.state.lock().await;
        state.component_index.get_indexing_summary()
    }

    /// Wait for indexing completion with timeout
    pub async fn wait_for_completion(&self, timeout: Duration) -> Result<(), ProjectError> {
        let latch = {
            let state = self.state.lock().await;
            state.completion_latch.clone()
        };

        latch.wait(timeout).await.map_err(|e| {
            ProjectError::IndexingTimeout(format!(
                "Indexing completion wait failed for {}: {}",
                self.build_directory.display(),
                e
            ))
        })
    }

    /// Refresh index state by syncing with disk using IndexReader
    pub async fn refresh_from_disk(&self) -> Result<(), ProjectError> {
        debug!(
            "Refreshing index state from disk for build dir: {}",
            self.build_directory.display()
        );

        // Use the existing rescan and validate method which properly uses IndexReader
        self.rescan_and_validate_untracked_files().await?;

        let state = self.state.lock().await;
        debug!(
            "Index state after disk sync: {}/{} files indexed",
            state.component_index.indexed_count(),
            state.component_index.total_files_count()
        );

        // Trigger completion latch if all files are indexed
        if state.component_index.is_fully_indexed() {
            let latch = state.completion_latch.clone();
            tokio::spawn(async move {
                latch.trigger_success().await;
            });
            debug!(
                "Triggered completion latch: all files already indexed for {}",
                self.build_directory.display()
            );
        }

        Ok(())
    }

    /// Rescan and validate untracked index files using proper validation
    ///
    /// This method:
    /// 1. Identifies files currently in Pending state
    /// 2. Uses IndexReader to check if index files exist and are valid
    /// 3. Validates format version and index content
    /// 4. Updates ComponentIndex state only for valid files
    /// 5. Provides detailed logging about discovered/rejected files
    async fn rescan_and_validate_untracked_files(&self) -> Result<(), ProjectError> {
        debug!(
            "Starting rescan and validation of untracked index files for build dir: {}",
            self.build_directory.display()
        );

        let mut state = self.state.lock().await;
        let pending_files: Vec<_> = state
            .component_index
            .get_pending_files()
            .iter()
            .map(|p| p.to_path_buf())
            .collect();

        if pending_files.is_empty() {
            debug!(
                "No pending files to rescan for build dir: {}",
                self.build_directory.display()
            );
            return Ok(());
        }

        debug!(
            "Found {} pending files to validate for build dir: {}",
            pending_files.len(),
            self.build_directory.display()
        );

        let mut files_validated = 0;
        let mut files_invalid = 0;
        let mut validation_errors = Vec::new();

        for source_file in &pending_files {
            match state.index_reader.read_index_for_file(source_file).await {
                Ok(index_entry) => {
                    match &index_entry.status {
                        crate::project::index::reader::FileIndexStatus::Done => {
                            // Valid index file found - mark as indexed
                            state.component_index.mark_file_indexed(source_file);
                            files_validated += 1;
                            trace!(
                                "Validated existing index for file: {:?} (format: v{}, {} symbols)",
                                source_file,
                                index_entry.expected_format_version,
                                index_entry.symbols.len()
                            );
                        }
                        crate::project::index::reader::FileIndexStatus::Invalid(reason) => {
                            // Index file exists but is invalid
                            files_invalid += 1;
                            let error_msg =
                                format!("Invalid index for {:?}: {}", source_file, reason);
                            validation_errors.push(error_msg.clone());
                            debug!("{}", error_msg);
                            // Leave as pending - will be re-indexed
                        }
                        crate::project::index::reader::FileIndexStatus::Stale => {
                            // Index file is stale
                            files_invalid += 1;
                            let error_msg = format!(
                                "Stale index for {:?}: file modified since indexing",
                                source_file
                            );
                            validation_errors.push(error_msg.clone());
                            debug!("{}", error_msg);
                            // Leave as pending - will be re-indexed
                        }
                        crate::project::index::reader::FileIndexStatus::None => {
                            // No index file found - this is expected, leave as pending
                            trace!("No existing index found for file: {:?}", source_file);
                        }
                        crate::project::index::reader::FileIndexStatus::InProgress => {
                            // Another process is indexing this file - leave as pending
                            trace!(
                                "File currently being indexed by another process: {:?}",
                                source_file
                            );
                        }
                    }
                }
                Err(e) => {
                    // Error reading index - leave as pending and log warning
                    let error_msg = format!("Failed to read index for {:?}: {}", source_file, e);
                    validation_errors.push(error_msg.clone());
                    warn!("{}", error_msg);
                }
            }
        }

        // Log summary of validation results
        if files_validated > 0 || files_invalid > 0 {
            info!(
                "Index validation complete for build dir {}: {} files validated, {} invalid/stale, {} errors",
                self.build_directory.display(),
                files_validated,
                files_invalid,
                validation_errors.len()
            );
        }

        if files_validated > 0 {
            debug!(
                "Discovered {} previously indexed files on disk for build dir: {}",
                files_validated,
                self.build_directory.display()
            );
        }

        // Log validation errors at debug level for diagnostics
        for error in &validation_errors {
            debug!("Validation error: {}", error);
        }

        Ok(())
    }

    /// Get unindexed files needing attention
    #[allow(dead_code)]
    pub async fn get_unindexed_files(&self) -> Vec<PathBuf> {
        let state = self.state.lock().await;
        state
            .component_index
            .get_pending_files()
            .iter()
            .map(|p| p.to_path_buf())
            .collect()
    }

    /// Get the build directory this monitor tracks
    #[allow(dead_code)]
    pub fn build_directory(&self) -> &Path {
        &self.build_directory
    }

    /// Get completion latch for external waiting
    pub async fn get_completion_latch(&self) -> IndexLatch {
        let state = self.state.lock().await;
        state.completion_latch.clone()
    }

    /// Trigger indexing for a specific file using the configured IndexTrigger
    ///
    /// This method delegates to the injected IndexTrigger to initiate indexing
    /// for the specified file. If no trigger is configured, this is a no-op.
    ///
    /// # Arguments
    /// * `file_path` - Path to the source file to trigger indexing for
    ///
    /// # Returns
    /// * `Ok(())` if indexing was successfully triggered or no trigger is configured
    /// * `Err(ProjectError)` if triggering failed
    pub async fn trigger_indexing(&self, file_path: &Path) -> Result<(), ProjectError> {
        if let Some(trigger) = &self.index_trigger {
            debug!("Triggering indexing for file: {:?}", file_path);
            trigger.trigger(file_path).await?;
        } else {
            debug!(
                "No index trigger configured, skipping indexing trigger for: {:?}",
                file_path
            );
        }
        Ok(())
    }

    /// Trigger initial indexing using the first source file from the compilation database
    ///
    /// This method selects the first source file from the compilation database and
    /// triggers indexing for it. This is typically called after monitor creation
    /// to initiate the indexing process.
    ///
    /// # Arguments
    /// * `compilation_db` - The compilation database to get source files from
    ///
    /// # Returns
    /// * `Ok(())` if indexing was successfully triggered or no trigger is configured
    /// * `Err(ProjectError)` if triggering failed or no source files are available
    pub async fn trigger_initial_indexing(
        &self,
        compilation_db: &CompilationDatabase,
    ) -> Result<(), ProjectError> {
        if self.index_trigger.is_some() {
            let source_files = compilation_db.source_files();
            if let Some(&first_file) = source_files.first() {
                debug!(
                    "Triggering initial indexing with first source file: {:?}",
                    first_file
                );
                self.trigger_indexing(first_file).await?;
            } else {
                warn!(
                    "No source files found in compilation database - cannot trigger initial indexing"
                );
            }
        } else {
            debug!("No index trigger configured, skipping initial indexing trigger");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clangd::index::ProgressEvent;
    use crate::project::compilation_database::CompilationDatabase;
    use crate::project::index::reader::{IndexReaderTrait, MockIndexReaderTrait};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    /// Create a test compilation database with a single file
    fn create_test_compilation_db() -> CompilationDatabase {
        use json_compilation_db::Entry;

        let entries = vec![Entry {
            directory: PathBuf::from("/test/project"),
            file: PathBuf::from("/test/project/src/main.cpp"),
            arguments: vec!["clang++".to_string(), "src/main.cpp".to_string()],
            output: Some(PathBuf::from("/test/project/build/main.o")),
        }];

        CompilationDatabase::from_entries(entries)
    }

    /// Create a test clangd version
    fn create_test_clangd_version() -> ClangdVersion {
        ClangdVersion {
            major: 18,
            minor: 1,
            patch: 8,
            variant: None,
            date: None,
        }
    }

    #[tokio::test]
    async fn test_component_monitor_creation() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");
        let clangd_version = create_test_clangd_version();

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir.clone(),
            &compilation_db,
            mock_reader,
            &clangd_version,
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        assert_eq!(monitor.build_directory(), Path::new("/test/project/build"));

        // Check initial state
        let state = monitor.get_component_state().await;
        assert_eq!(state.state, ComponentIndexingState::Init);
        assert_eq!(state.total_cdb_files, 1);
        assert_eq!(state.indexed_cdb_files, 0);
    }

    #[tokio::test]
    async fn test_progress_event_handling_indexing_started() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test overall indexing started event
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;

        let state = monitor.get_component_state().await;
        assert_eq!(state.state, ComponentIndexingState::InProgress(0.0));
    }

    #[tokio::test]
    async fn test_progress_event_handling_file_completed() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        let file_path = PathBuf::from("/test/project/src/main.cpp");

        // Start indexing
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;

        // File indexing completed
        monitor
            .handle_progress_event(ProgressEvent::FileIndexingCompleted {
                path: file_path,
                symbols: 10,
                refs: 20,
            })
            .await;

        let state = monitor.get_component_state().await;
        assert_eq!(state.indexed_cdb_files, 1);
    }

    #[tokio::test]
    async fn test_overall_progress_updates() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Start indexing
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;

        // Progress update
        monitor
            .handle_progress_event(ProgressEvent::OverallProgress {
                current: 5,
                total: 10,
                percentage: 50,
                message: Some("Indexing symbols".to_string()),
            })
            .await;

        let state = monitor.get_component_state().await;
        assert_eq!(state.state, ComponentIndexingState::InProgress(50.0));
    }

    #[tokio::test]
    async fn test_indexing_completion_with_full_coverage() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        let file_path = PathBuf::from("/test/project/src/main.cpp");

        // Complete indexing flow
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;
        monitor
            .handle_progress_event(ProgressEvent::FileIndexingCompleted {
                path: file_path,
                symbols: 10,
                refs: 20,
            })
            .await;
        monitor
            .handle_progress_event(ProgressEvent::OverallCompleted)
            .await;

        let state = monitor.get_component_state().await;
        assert_eq!(state.state, ComponentIndexingState::Completed);
        assert_eq!(state.indexed_cdb_files, 1);
        assert!(state.is_complete());
        assert!((state.coverage() - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_indexing_completion_with_partial_coverage() {
        let mut mock_reader = MockIndexReaderTrait::new();

        // Set up mock expectations for the rescan operation
        // The rescan will check the remaining pending file (utils.cpp)
        mock_reader
            .expect_read_index_for_file()
            .with(mockall::predicate::eq(PathBuf::from(
                "/test/project/src/utils.cpp",
            )))
            .returning(|_| {
                Box::pin(async {
                    Ok(crate::project::index::reader::IndexEntry {
                        absolute_path: PathBuf::from("/test/project/src/utils.cpp"),
                        status: crate::project::index::reader::FileIndexStatus::None, // No existing index
                        index_format_version: None,
                        expected_format_version: 19,
                        index_content_hash: None,
                        current_file_hash: None,
                        symbols: vec![],
                        index_file_size: None,
                        index_created_at: None,
                    })
                })
            })
            .times(1);

        let mock_reader = Arc::new(mock_reader) as Arc<dyn IndexReaderTrait>;

        // Create compilation database with multiple files
        use json_compilation_db::Entry;

        let entries = vec![
            Entry {
                directory: PathBuf::from("/test/project"),
                file: PathBuf::from("/test/project/src/main.cpp"),
                arguments: vec!["clang++".to_string(), "src/main.cpp".to_string()],
                output: Some(PathBuf::from("/test/project/build/main.o")),
            },
            Entry {
                directory: PathBuf::from("/test/project"),
                file: PathBuf::from("/test/project/src/utils.cpp"),
                arguments: vec!["clang++".to_string(), "src/utils.cpp".to_string()],
                output: Some(PathBuf::from("/test/project/build/utils.o")),
            },
        ];
        let compilation_db = CompilationDatabase::from_entries(entries);

        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        let file_path = PathBuf::from("/test/project/src/main.cpp");

        // Complete indexing flow - only index one of two files
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;
        monitor
            .handle_progress_event(ProgressEvent::FileIndexingCompleted {
                path: file_path,
                symbols: 10,
                refs: 20,
            })
            .await;
        monitor
            .handle_progress_event(ProgressEvent::OverallCompleted)
            .await;

        let state = monitor.get_component_state().await;
        assert_eq!(state.state, ComponentIndexingState::Partial);
        assert_eq!(state.indexed_cdb_files, 1);
        assert_eq!(state.total_cdb_files, 2);
        assert!(!state.is_complete());
        assert!((state.coverage() - 0.5).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_indexing_failure_handling() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test indexing failure
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;
        monitor
            .handle_progress_event(ProgressEvent::IndexingFailed {
                error: "Test error".to_string(),
            })
            .await;

        // Component state should remain InProgress since failure doesn't change it directly
        let state = monitor.get_component_state().await;
        assert_eq!(state.state, ComponentIndexingState::InProgress(0.0));
    }

    #[tokio::test]
    async fn test_refresh_from_disk() {
        let mut mock_reader = MockIndexReaderTrait::new();

        // Configure mock to return FileIndexStatus::None for all files (no indexes found)
        mock_reader.expect_read_index_for_file().returning(|_| {
            Box::pin(async {
                Ok(crate::project::index::reader::IndexEntry {
                    absolute_path: PathBuf::from("/test/project/src/main.cpp"),
                    status: crate::project::index::reader::FileIndexStatus::None,
                    index_format_version: None,
                    expected_format_version: 19,
                    index_content_hash: None,
                    current_file_hash: None,
                    symbols: vec![],
                    index_file_size: None,
                    index_created_at: None,
                })
            })
        });

        let mock_reader = Arc::new(mock_reader) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test that refresh works by calling the rescan method
        // which now properly uses IndexReader instead of direct filesystem access
        monitor
            .refresh_from_disk()
            .await
            .expect("Failed to refresh from disk");

        // Verify the state remains unchanged since no valid index files exist
        let coverage = monitor.get_coverage().await;
        assert_eq!(coverage, 0.0); // No files should be marked as indexed
    }

    #[tokio::test]
    async fn test_completion_latch_wait() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Start a task that will trigger completion immediately
        let monitor_clone = Arc::new(monitor);
        let trigger_monitor = Arc::clone(&monitor_clone);
        tokio::spawn(async move {
            trigger_monitor
                .handle_progress_event(ProgressEvent::OverallIndexingStarted)
                .await;
            trigger_monitor
                .handle_progress_event(ProgressEvent::FileIndexingCompleted {
                    path: PathBuf::from("/test/project/src/main.cpp"),
                    symbols: 10,
                    refs: 20,
                })
                .await;
            trigger_monitor
                .handle_progress_event(ProgressEvent::OverallCompleted)
                .await;
        });

        // Wait for completion
        let result = monitor_clone
            .wait_for_completion(Duration::from_secs(1))
            .await;
        assert!(result.is_ok(), "Wait for completion should succeed");
    }

    #[tokio::test]
    async fn test_standard_library_indexing_events() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();

        let monitor = ComponentIndexMonitor::new_for_test(
            PathBuf::from("/test/build"),
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .unwrap();

        monitor
            .handle_progress_event(ProgressEvent::StandardLibraryStarted {
                stdlib_version: "libstdc++-13".to_string(),
                context_file: PathBuf::from("/usr/include/iostream"),
            })
            .await;

        monitor
            .handle_progress_event(ProgressEvent::StandardLibraryCompleted {
                symbols: 5000,
                filtered: 1000,
            })
            .await;

        // Events processed successfully - no panics occurred
    }

    #[tokio::test]
    async fn test_overall_completed_with_rescanning() {
        let mut mock_reader = MockIndexReaderTrait::new();

        // Set up mock expectations for the rescan operation
        // The rescan will check pending files for existing index files
        mock_reader
            .expect_read_index_for_file()
            .with(mockall::predicate::eq(PathBuf::from(
                "/test/project/src/utils.cpp",
            )))
            .returning(|_| {
                Box::pin(async {
                    Ok(crate::project::index::reader::IndexEntry {
                        absolute_path: PathBuf::from("/test/project/src/utils.cpp"),
                        status: crate::project::index::reader::FileIndexStatus::None, // No existing index
                        index_format_version: None,
                        expected_format_version: 19,
                        index_content_hash: None,
                        current_file_hash: None,
                        symbols: vec![],
                        index_file_size: None,
                        index_created_at: None,
                    })
                })
            })
            .times(1);

        let mock_reader = Arc::new(mock_reader) as Arc<dyn IndexReaderTrait>;

        // Create compilation database with multiple files to test rescanning
        use json_compilation_db::Entry;
        let entries = vec![
            Entry {
                directory: PathBuf::from("/test/project"),
                file: PathBuf::from("/test/project/src/main.cpp"),
                arguments: vec!["clang++".to_string(), "src/main.cpp".to_string()],
                output: Some(PathBuf::from("/test/project/build/main.o")),
            },
            Entry {
                directory: PathBuf::from("/test/project"),
                file: PathBuf::from("/test/project/src/utils.cpp"),
                arguments: vec!["clang++".to_string(), "src/utils.cpp".to_string()],
                output: Some(PathBuf::from("/test/project/build/utils.o")),
            },
        ];
        let compilation_db = CompilationDatabase::from_entries(entries);

        let monitor = ComponentIndexMonitor::new_for_test(
            PathBuf::from("/test/project/build"),
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        let main_file = PathBuf::from("/test/project/src/main.cpp");

        // Start indexing and complete one file
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;
        monitor
            .handle_progress_event(ProgressEvent::FileIndexingCompleted {
                path: main_file,
                symbols: 10,
                refs: 20,
            })
            .await;

        // Before overall completion, we have partial coverage
        let state = monitor.get_component_state().await;
        assert_eq!(state.indexed_cdb_files, 1);
        assert_eq!(state.total_cdb_files, 2);

        // Complete overall indexing - this triggers rescan with validation
        monitor
            .handle_progress_event(ProgressEvent::OverallCompleted)
            .await;

        // After completion, the rescan should have been called (mock expectation verified)
        // Since the mock returned None status, no additional files should be marked as indexed
        let final_state = monitor.get_component_state().await;
        assert_eq!(final_state.state, ComponentIndexingState::Partial); // Still partial since only 1/2 files indexed
        assert_eq!(final_state.indexed_cdb_files, 1);
    }

    #[tokio::test]
    async fn test_get_indexing_summary() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        let file_path = PathBuf::from("/test/project/src/main.cpp");

        // Progress through different states
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;
        monitor
            .handle_progress_event(ProgressEvent::FileIndexingStarted {
                path: file_path.clone(),
                digest: "ABC123".to_string(),
            })
            .await;

        let summary = monitor.get_indexing_summary().await;

        // Verify summary structure
        assert_eq!(summary.total_files, 1);
        assert_eq!(summary.in_progress_count, 1);
        assert_eq!(summary.indexed_count, 0);
        assert!(summary.has_active_indexing);
        assert!(!summary.is_fully_indexed);
        assert_eq!(summary.in_progress_files.len(), 1);
        assert_eq!(summary.in_progress_files[0], file_path);
    }

    #[tokio::test]
    async fn test_enhanced_logging_on_completion() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        let file_path = PathBuf::from("/test/project/src/main.cpp");

        // Complete full indexing cycle
        monitor
            .handle_progress_event(ProgressEvent::OverallIndexingStarted)
            .await;
        monitor
            .handle_progress_event(ProgressEvent::FileIndexingCompleted {
                path: file_path,
                symbols: 10,
                refs: 20,
            })
            .await;
        monitor
            .handle_progress_event(ProgressEvent::OverallCompleted)
            .await;

        // Verify final state shows completed indexing with enhanced tracking
        let final_state = monitor.get_component_state().await;
        assert_eq!(final_state.state, ComponentIndexingState::Completed);
        assert!(final_state.is_complete());
        assert!((final_state.coverage() - 1.0).abs() < 0.001);

        let summary = monitor.get_indexing_summary().await;
        assert!(summary.is_fully_indexed);
        assert!(!summary.has_active_indexing);
        assert_eq!(summary.indexed_count, 1);
        assert_eq!(summary.pending_count, 0);
    }

    #[tokio::test]
    async fn test_trigger_indexing_with_mock() {
        use crate::project::index::trigger::MockIndexTrigger;

        let mut mock_reader = MockIndexReaderTrait::new();

        // Expect the initial disk scan call during ComponentIndexMonitor creation
        mock_reader
            .expect_read_index_for_file()
            .with(mockall::predicate::function(|path: &Path| {
                path == Path::new("/test/project/src/main.cpp")
            }))
            .returning(|_| {
                Box::pin(async {
                    Ok(crate::project::index::reader::IndexEntry {
                        absolute_path: PathBuf::from("/test/project/src/main.cpp"),
                        status: crate::project::index::reader::FileIndexStatus::None,
                        index_format_version: None,
                        expected_format_version: 19,
                        index_content_hash: None,
                        current_file_hash: None,
                        symbols: vec![],
                        index_file_size: None,
                        index_created_at: None,
                    })
                })
            });

        let mock_reader = Arc::new(mock_reader) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        // Create mock trigger
        let mut mock_trigger = MockIndexTrigger::new();
        let test_file = PathBuf::from("/test/project/src/main.cpp");
        let expected_file = test_file.clone();

        mock_trigger
            .expect_trigger()
            .with(mockall::predicate::function(move |path: &Path| {
                path == expected_file
            }))
            .times(1)
            .returning(|_| Ok(()));

        let trigger = Arc::new(mock_trigger) as Arc<dyn IndexTrigger>;

        // Create monitor with trigger
        let monitor = ComponentIndexMonitor::new_with_trigger(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
            Some(trigger),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test trigger_indexing method
        let result = monitor.trigger_indexing(&test_file).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_trigger_indexing_without_trigger() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        // Create monitor without trigger (using regular new method)
        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test trigger_indexing method - should succeed but do nothing
        let test_file = PathBuf::from("/test/project/src/main.cpp");
        let result = monitor.trigger_indexing(&test_file).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_trigger_initial_indexing() {
        use crate::project::index::trigger::MockIndexTrigger;

        let mut mock_reader = MockIndexReaderTrait::new();

        // Expect the initial disk scan call during ComponentIndexMonitor creation
        mock_reader
            .expect_read_index_for_file()
            .with(mockall::predicate::function(|path: &Path| {
                path == Path::new("/test/project/src/main.cpp")
            }))
            .returning(|_| {
                Box::pin(async {
                    Ok(crate::project::index::reader::IndexEntry {
                        absolute_path: PathBuf::from("/test/project/src/main.cpp"),
                        status: crate::project::index::reader::FileIndexStatus::None,
                        index_format_version: None,
                        expected_format_version: 19,
                        index_content_hash: None,
                        current_file_hash: None,
                        symbols: vec![],
                        index_file_size: None,
                        index_created_at: None,
                    })
                })
            });

        let mock_reader = Arc::new(mock_reader) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        // Create mock trigger that expects the first file from compilation database
        let mut mock_trigger = MockIndexTrigger::new();
        let expected_file = PathBuf::from("/test/project/src/main.cpp");
        let expected_file_clone = expected_file.clone();

        mock_trigger
            .expect_trigger()
            .with(mockall::predicate::function(move |path: &Path| {
                path == expected_file_clone
            }))
            .times(1)
            .returning(|_| Ok(()));

        let trigger = Arc::new(mock_trigger) as Arc<dyn IndexTrigger>;

        // Create monitor with trigger
        let monitor = ComponentIndexMonitor::new_with_trigger(
            build_dir,
            &compilation_db,
            mock_reader,
            &create_test_clangd_version(),
            Some(trigger),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test trigger_initial_indexing method
        let result = monitor.trigger_initial_indexing(&compilation_db).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_trigger_initial_indexing_empty_db() {
        use crate::project::index::trigger::MockIndexTrigger;

        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let build_dir = PathBuf::from("/test/project/build");

        // Create empty compilation database
        let empty_compilation_db = CompilationDatabase::from_entries(vec![]);

        // Create mock trigger that should not be called
        let mock_trigger = MockIndexTrigger::new();
        let trigger = Arc::new(mock_trigger) as Arc<dyn IndexTrigger>;

        // Create monitor with trigger
        let monitor = ComponentIndexMonitor::new_with_trigger(
            build_dir,
            &empty_compilation_db,
            mock_reader,
            &create_test_clangd_version(),
            Some(trigger),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test trigger_initial_indexing method with empty database - should succeed but not call trigger
        let result = monitor
            .trigger_initial_indexing(&empty_compilation_db)
            .await;
        assert!(result.is_ok());
    }
}
