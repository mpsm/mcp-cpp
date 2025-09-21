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
use crate::project::compilation_database::PathMappings;

/// Result of validating a single index entry
enum IndexValidationResult {
    /// Index is valid and file should be marked as indexed
    Valid,
    /// Index is invalid or stale, contains error message
    Invalid(String),
    /// No index file found (expected state)
    NotFound,
    /// File is currently being indexed by another process
    InProgress,
}

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

    /// When indexing started, None if not started or completed
    indexing_start_time: Option<std::time::SystemTime>,

    /// Last updated timestamp
    last_updated: std::time::SystemTime,

    /// Bidirectional path mappings for efficient path lookup
    /// (original_path -> canonical_path, canonical_path -> original_path)
    path_mappings: PathMappings,
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
        Self::create_monitor(
            build_directory,
            compilation_db,
            index_reader,
            clangd_version,
            None,
            false, // perform_scan = false for non-test version
        )
        .await
    }

    /// Create monitor for specific build directory with optional index trigger
    pub async fn new_with_trigger(
        build_directory: PathBuf,
        compilation_db: Arc<CompilationDatabase>,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
        index_trigger: Option<Arc<dyn IndexTrigger>>,
    ) -> Result<Self, ProjectError> {
        Self::create_monitor(
            build_directory,
            &compilation_db,
            index_reader,
            clangd_version,
            index_trigger,
            true, // perform_scan = true for production version
        )
        .await
    }

    /// Create monitor for testing without filesystem dependencies
    #[cfg(test)]
    pub async fn new_for_test(
        build_directory: PathBuf,
        compilation_db: Arc<CompilationDatabase>,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
    ) -> Result<Self, ProjectError> {
        Self::create_monitor_for_test(
            build_directory,
            &compilation_db,
            index_reader,
            clangd_version,
        )
        .await
    }

    /// Common monitor creation logic
    async fn create_monitor(
        build_directory: PathBuf,
        compilation_db: &CompilationDatabase,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
        index_trigger: Option<Arc<dyn IndexTrigger>>,
        perform_scan: bool,
    ) -> Result<Self, ProjectError> {
        let monitor_state = Self::create_monitor_state(
            compilation_db,
            index_reader,
            clangd_version,
            &build_directory,
        )?;

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

        if perform_scan {
            monitor.perform_initial_scan().await;
        }

        Ok(monitor)
    }

    /// Create monitor state for testing
    #[cfg(test)]
    async fn create_monitor_for_test(
        build_directory: PathBuf,
        compilation_db: &CompilationDatabase,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
    ) -> Result<Self, ProjectError> {
        // Create component index from compilation database (test version)
        let component_index = ComponentIndex::new_for_test(compilation_db, clangd_version);

        // Get path mappings for testing (use empty mappings for test)
        let path_mappings = (std::collections::HashMap::new(), std::collections::HashMap::new());

        // Create completion latch
        let completion_latch = IndexLatch::new();

        let monitor_state = IndexMonitorState {
            component_index,
            index_reader,
            current_indexing_state: ComponentIndexingState::Init,
            completion_latch,
            indexing_start_time: None,
            last_updated: std::time::SystemTime::now(),
            path_mappings,
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

    /// Create common monitor state
    fn create_monitor_state(
        compilation_db: &CompilationDatabase,
        index_reader: Arc<dyn IndexReaderTrait>,
        clangd_version: &ClangdVersion,
        build_directory: &Path,
    ) -> Result<IndexMonitorState, ProjectError> {
        // Create component index from compilation database (all files start as Pending)
        let component_index = ComponentIndex::new(compilation_db, clangd_version).map_err(|e| {
            ProjectError::SessionCreation(format!(
                "Failed to create component index for {}: {}",
                build_directory.display(),
                e
            ))
        })?;

        // Get path mappings for efficient lookup without repeated canonicalization
        let path_mappings = compilation_db.path_mappings().map_err(|e| {
            ProjectError::SessionCreation(format!(
                "Failed to create path mappings for {}: {}",
                build_directory.display(),
                e
            ))
        })?;

        // Create completion latch
        let completion_latch = IndexLatch::new();

        Ok(IndexMonitorState {
            component_index,
            index_reader,
            current_indexing_state: ComponentIndexingState::Init,
            completion_latch,
            indexing_start_time: None,
            last_updated: std::time::SystemTime::now(),
            path_mappings,
        })
    }

    /// Perform initial disk scan to update state from existing index files
    async fn perform_initial_scan(&self) {
        debug!(
            "Performing initial disk scan for build dir: {}",
            self.build_directory.display()
        );

        if let Err(e) = self.rescan_and_validate_untracked_files().await {
            warn!(
                "Failed to perform initial disk scan for {}: {}",
                self.build_directory.display(),
                e
            );
        }

        let state = self.state.lock().await;
        debug!(
            "Initial state after disk scan: {}/{} files indexed for {}",
            state.component_index.indexed_count(),
            state.component_index.total_files_count(),
            self.build_directory.display()
        );
    }

    /// Handle progress event (single lock, focused responsibility)
    pub async fn handle_progress_event(&self, event: ProgressEvent) {
        match event {
            ProgressEvent::FileIndexingStarted { path, digest } => {
                self.handle_file_indexing_started(path, digest).await;
            }
            ProgressEvent::FileIndexingCompleted {
                path,
                symbols,
                refs,
            } => {
                self.handle_file_indexing_completed(path, symbols, refs)
                    .await;
            }
            ProgressEvent::FileAstIndexed { path } => {
                self.handle_file_ast_indexed(path).await;
            }
            ProgressEvent::FileAstFailed { path } => {
                self.handle_file_ast_failed(path).await;
            }
            ProgressEvent::StandardLibraryStarted {
                stdlib_version,
                context_file,
            } => {
                self.handle_standard_library_started(stdlib_version, context_file)
                    .await;
            }
            ProgressEvent::StandardLibraryCompleted { symbols, filtered } => {
                self.handle_standard_library_completed(symbols, filtered)
                    .await;
            }
            ProgressEvent::OverallIndexingStarted => {
                self.handle_overall_indexing_started().await;
            }
            ProgressEvent::OverallProgress {
                current,
                total,
                percentage,
                message,
            } => {
                self.handle_overall_progress(current, total, percentage, message)
                    .await;
            }
            ProgressEvent::OverallCompleted => {
                self.handle_overall_completed().await;
            }
            ProgressEvent::IndexingFailed { error } => {
                self.handle_indexing_failed(error).await;
            }
        }
    }

    /// Handle file indexing started event
    async fn handle_file_indexing_started(&self, path: PathBuf, _digest: String) {
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

        debug!("File indexing started: {:?}", path);

        // Canonicalize path for consistent HashMap lookup
        let canonical_path = self.canonicalize_path_for_lookup(&path, &state.path_mappings);
        state.component_index.mark_file_in_progress(&canonical_path);
    }

    /// Convert a path from progress events to canonical form using precomputed mappings
    /// This replaces filesystem canonicalization with efficient HashMap lookup
    fn canonicalize_path_for_lookup(&self, path: &Path, path_mappings: &PathMappings) -> PathBuf {
        let (original_to_canonical, _canonical_to_original) = path_mappings;

        // First try direct lookup for exact match
        if let Some(canonical) = original_to_canonical.get(path) {
            return canonical.clone();
        }

        // If path is relative, resolve it against the build directory (where clangd runs)
        // and try lookup again
        if path.is_relative() {
            let resolved_path = self.build_directory.join(path);
            if let Some(canonical) = original_to_canonical.get(&resolved_path) {
                return canonical.clone();
            }
        }

        // Fallback: return the path as-is if no mapping found
        // This handles edge cases where clangd emits paths not in the compilation database
        path.to_path_buf()
    }

    /// Handle file indexing completed event
    async fn handle_file_indexing_completed(&self, path: PathBuf, symbols: u32, refs: u32) {
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

        debug!(
            "File indexing completed: {:?} ({} symbols, {} refs)",
            path, symbols, refs
        );

        // Canonicalize path for consistent HashMap lookup
        let canonical_path = self.canonicalize_path_for_lookup(&path, &state.path_mappings);
        state.component_index.mark_file_indexed(&canonical_path);

        debug!(
            "CDB file indexed: {:?} ({}/{})",
            path,
            state.component_index.indexed_count(),
            state.component_index.total_files_count()
        );
    }

    /// Handle file AST indexed event
    async fn handle_file_ast_indexed(&self, path: PathBuf) {
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

        debug!("File AST indexed: {:?}", path);

        // Mark this file as indexed since AST is now available
        // Canonicalize path for consistent HashMap lookup
        let canonical_path = self.canonicalize_path_for_lookup(&path, &state.path_mappings);
        state.component_index.mark_file_indexed(&canonical_path);

        debug!(
            "AST indexed - marking file as indexed: {:?} ({}/{})",
            path,
            state.component_index.indexed_count(),
            state.component_index.total_files_count()
        );

        // Trigger next file ONLY if we're in Init or Partial state
        // During InProgress, we wait for clangd to finish and determine Partial/Completed
        if matches!(
            state.current_indexing_state,
            ComponentIndexingState::Init | ComponentIndexingState::Partial
        ) {
            if let Some(next_file) = state.component_index.get_next_uncovered_file() {
                debug!(
                    "State is {:?}, triggering indexing for next file: {:?}",
                    state.current_indexing_state, next_file
                );

                // Clone the path since we need to release the lock
                let next_file_path = next_file.to_path_buf();

                // Release the state lock before async operation
                drop(state);

                // Trigger indexing for the next file
                if let Err(e) = self.trigger_indexing(&next_file_path).await {
                    warn!(
                        "Failed to trigger indexing for next file {:?}: {}",
                        next_file_path, e
                    );
                }

                // Don't re-acquire the lock since we're done
            } else {
                debug!("No more pending files to trigger");
            }
        } else {
            debug!(
                "Not triggering next file - state is {:?} (waiting for completion)",
                state.current_indexing_state
            );
        }
    }

    /// Handle file AST failed event
    async fn handle_file_ast_failed(&self, path: PathBuf) {
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

        debug!("File AST failed: {:?}", path);

        // Mark this file as failed since AST build failed
        // Canonicalize path for consistent HashMap lookup
        let canonical_path = self.canonicalize_path_for_lookup(&path, &state.path_mappings);
        state
            .component_index
            .mark_file_failed(&canonical_path, "AST build failed".to_string());

        debug!(
            "AST failed - marking file as failed: {:?} ({}/{})",
            path,
            state.component_index.indexed_count(),
            state.component_index.total_files_count()
        );

        // Trigger next file ONLY if we're in Init or Partial state
        // During InProgress, we wait for clangd to finish and determine Partial/Completed
        if matches!(
            state.current_indexing_state,
            ComponentIndexingState::Init | ComponentIndexingState::Partial
        ) {
            if let Some(next_file) = state.component_index.get_next_uncovered_file() {
                debug!(
                    "State is {:?}, triggering indexing for next file: {:?}",
                    state.current_indexing_state, next_file
                );

                // Clone the path since we need to release the lock
                let next_file_path = next_file.to_path_buf();

                // Release the state lock before async operation
                drop(state);

                // Trigger indexing for the next file
                if let Err(e) = self.trigger_indexing(&next_file_path).await {
                    warn!(
                        "Failed to trigger indexing for next file {:?}: {}",
                        next_file_path, e
                    );
                }

                // Don't re-acquire the lock since we're done
            } else {
                debug!("No more pending files to trigger");
            }
        } else {
            debug!(
                "Not triggering next file - state is {:?} (waiting for completion)",
                state.current_indexing_state
            );
        }
    }

    /// Handle standard library indexing started event
    async fn handle_standard_library_started(&self, stdlib_version: String, context_file: PathBuf) {
        debug!(
            "Standard library indexing started: {} (context: {:?})",
            stdlib_version, context_file
        );
    }

    /// Handle standard library indexing completed event
    async fn handle_standard_library_completed(&self, symbols: u32, filtered: u32) {
        debug!(
            "Standard library indexing completed: {} symbols, {} filtered",
            symbols, filtered
        );
    }

    /// Handle overall indexing started event
    async fn handle_overall_indexing_started(&self) {
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

        info!(
            "Overall indexing started for build directory: {}",
            self.build_directory.display()
        );

        // Transition component state from Init to InProgress and set start time
        state.current_indexing_state = ComponentIndexingState::InProgress(0.0);
        state.indexing_start_time = Some(std::time::SystemTime::now());
        state.last_updated = std::time::SystemTime::now();
        debug!(
            "Component state transitioned to InProgress for {}",
            self.build_directory.display()
        );
    }

    /// Handle overall progress event
    async fn handle_overall_progress(
        &self,
        current: u32,
        total: u32,
        percentage: u8,
        message: Option<String>,
    ) {
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

        debug!(
            "Overall indexing progress: {}/{} ({}%) - {:?}",
            current, total, percentage, message
        );

        // Update component state with progress percentage
        state.current_indexing_state = ComponentIndexingState::InProgress(percentage as f32);
        state.last_updated = std::time::SystemTime::now();
        trace!(
            "Component progress updated to {}% for {}",
            percentage,
            self.build_directory.display()
        );
    }

    /// Handle overall completion event
    async fn handle_overall_completed(&self) {
        info!(
            "Overall indexing completed for build directory: {}",
            self.build_directory.display()
        );

        // Perform validation of untracked index files
        self.perform_post_completion_validation().await;

        // Determine final state and handle next file triggering
        let should_trigger_next = self.determine_final_indexing_state().await;

        // Trigger next file if needed or finalize completion
        if let Some(next_file_path) = should_trigger_next {
            self.trigger_next_file_and_finalize(next_file_path).await;
        } else {
            self.finalize_completion().await;
        }
    }

    /// Perform validation of untracked index files after overall completion
    async fn perform_post_completion_validation(&self) {
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
    }

    /// Determine final indexing state and return next file to trigger if any
    async fn determine_final_indexing_state(&self) -> Option<PathBuf> {
        // Re-acquire state lock for final state determination
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(_) => {
                warn!(
                    "Could not acquire lock after rescan for {}",
                    self.build_directory.display()
                );
                return None;
            }
        };

        // Determine component final state based on CDB coverage AFTER validation
        let should_trigger_next = if state.component_index.is_fully_indexed() {
            debug!(
                "All CDB files indexed ({}/{}), transitioning to Completed",
                state.component_index.indexed_count(),
                state.component_index.total_files_count()
            );

            // Update state to Completed and clear start time
            state.current_indexing_state = ComponentIndexingState::Completed;
            state.indexing_start_time = None;
            state.last_updated = std::time::SystemTime::now();
            None // No next file to trigger
        } else {
            debug!(
                "Partial CDB coverage ({}/{}), transitioning to Partial",
                state.component_index.indexed_count(),
                state.component_index.total_files_count()
            );

            // Update state to Partial and clear start time
            state.current_indexing_state = ComponentIndexingState::Partial;
            state.indexing_start_time = None;
            state.last_updated = std::time::SystemTime::now();

            // Check if we should trigger next file after transitioning to Partial
            state
                .component_index
                .get_next_uncovered_file()
                .map(|p| p.to_path_buf())
        };

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

        should_trigger_next
    }

    /// Trigger next file and finalize completion
    async fn trigger_next_file_and_finalize(&self, next_file_path: PathBuf) {
        debug!(
            "Triggering next unindexed file after Partial transition: {:?}",
            next_file_path
        );

        // Trigger the next file
        if let Err(e) = self.trigger_indexing(&next_file_path).await {
            warn!(
                "Failed to trigger next file after Partial transition: {}",
                e
            );
        }

        // Finalize with latch triggering
        self.finalize_completion().await;
    }

    /// Finalize completion by triggering the completion latch
    async fn finalize_completion(&self) {
        // Re-acquire state lock for latch triggering
        let state = match self.state.try_lock() {
            Ok(state) => state,
            Err(_) => {
                warn!("Could not acquire state lock for latch triggering");
                return;
            }
        };

        // Trigger latch now that initial indexing has ended (either Partial or Completed)
        let latch = state.completion_latch.clone();
        drop(state); // Release before spawning
        tokio::spawn(async move {
            latch.trigger_success().await;
        });
        debug!(
            "Triggered completion latch for build directory: {} (initial indexing ended)",
            self.build_directory.display()
        );
    }

    /// Handle indexing failed event
    async fn handle_indexing_failed(&self, error: String) {
        let state = match self.state.try_lock() {
            Ok(state) => state,
            Err(_) => {
                warn!(
                    "Could not acquire lock on component monitor state for {}",
                    self.build_directory.display()
                );
                return;
            }
        };

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

    /// Get current component indexing state
    #[cfg(test)]
    pub async fn get_component_state(&self) -> ComponentIndexState {
        let state = self.state.lock().await;
        ComponentIndexState::from_component_index(
            &state.component_index,
            state.current_indexing_state.clone(),
        )
    }

    /// Get comprehensive indexing summary with detailed state information
    #[allow(dead_code)] // Public API for future use
    pub async fn get_indexing_summary(&self) -> crate::clangd::index::IndexingSummary {
        let state = self.state.lock().await;
        state.component_index.get_indexing_summary()
    }

    /// Get progress tracking data including start time for ETA calculation
    pub async fn get_progress_data(&self) -> (ComponentIndexState, Option<std::time::SystemTime>) {
        let state = self.state.lock().await;
        let component_state = ComponentIndexState::from_component_index(
            &state.component_index,
            state.current_indexing_state.clone(),
        );
        (component_state, state.indexing_start_time)
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

    /// Validate a single index entry and return appropriate action
    fn validate_index_entry(
        &self,
        source_file: &Path,
        index_entry: &crate::project::index::reader::IndexEntry,
    ) -> IndexValidationResult {
        match &index_entry.status {
            crate::project::index::reader::FileIndexStatus::Done => {
                // Valid index file found - mark as indexed
                trace!(
                    "Validated existing index for file: {:?} (format: v{}, {} symbols)",
                    source_file,
                    index_entry.expected_format_version,
                    index_entry.symbols.len()
                );
                IndexValidationResult::Valid
            }
            crate::project::index::reader::FileIndexStatus::Invalid(reason) => {
                // Index file exists but is invalid
                let error_msg = format!("Invalid index for {:?}: {}", source_file, reason);
                IndexValidationResult::Invalid(error_msg)
            }
            crate::project::index::reader::FileIndexStatus::Stale => {
                // Index file is stale
                let error_msg = format!(
                    "Stale index for {:?}: file modified since indexing",
                    source_file
                );
                IndexValidationResult::Invalid(error_msg)
            }
            crate::project::index::reader::FileIndexStatus::None => {
                // No index file found - this is expected, leave as pending
                IndexValidationResult::NotFound
            }
            crate::project::index::reader::FileIndexStatus::InProgress => {
                // Another process is indexing this file - leave as pending
                IndexValidationResult::InProgress
            }
        }
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
                    let validation_result = self.validate_index_entry(source_file, &index_entry);
                    match validation_result {
                        IndexValidationResult::Valid => {
                            state.component_index.mark_file_indexed(source_file);
                            files_validated += 1;
                        }
                        IndexValidationResult::Invalid(error_msg) => {
                            files_invalid += 1;
                            validation_errors.push(error_msg.clone());
                            debug!("{}", error_msg);
                        }
                        IndexValidationResult::NotFound => {
                            trace!("No existing index found for file: {:?}", source_file);
                        }
                        IndexValidationResult::InProgress => {
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
        compilation_db: Arc<CompilationDatabase>,
    ) -> Result<(), ProjectError> {
        if self.index_trigger.is_some() {
            // Use canonical files from the single source of truth
            let canonical_files = compilation_db.canonical_source_files().map_err(|e| {
                ProjectError::SessionCreation(format!(
                    "Failed to get canonical source files for trigger: {}",
                    e
                ))
            })?;

            if let Some(first_file) = canonical_files.first() {
                debug!(
                    "Triggering initial indexing with first canonical source file: {:?}",
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
    async fn test_completion_latch_wait() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            Arc::new(compilation_db.clone()),
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
    async fn test_handle_file_ast_failed_event() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new_for_test(
            build_dir,
            Arc::new(compilation_db.clone()),
            mock_reader,
            &create_test_clangd_version(),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Handle file AST failed event
        let test_file_path = PathBuf::from("/test/project/src/main.cpp");
        monitor
            .handle_progress_event(ProgressEvent::FileAstFailed {
                path: test_file_path.clone(),
            })
            .await;

        // Verify file is marked as failed
        let state = monitor.state.lock().await;
        assert!(state.component_index.is_file_failed(&test_file_path));
        assert_eq!(state.component_index.failed_count(), 1);
        assert_eq!(state.component_index.indexed_count(), 0);
    }

    #[tokio::test]
    async fn test_standard_library_indexing_events() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();

        let monitor = ComponentIndexMonitor::new_for_test(
            PathBuf::from("/test/build"),
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
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
            Arc::new(compilation_db.clone()),
            mock_reader,
            &create_test_clangd_version(),
            Some(trigger),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test trigger_initial_indexing method
        let result = monitor
            .trigger_initial_indexing(Arc::new(compilation_db))
            .await;
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
            Arc::new(empty_compilation_db.clone()),
            mock_reader,
            &create_test_clangd_version(),
            Some(trigger),
        )
        .await
        .expect("Failed to create ComponentIndexMonitor");

        // Test trigger_initial_indexing method with empty database - should succeed but not call trigger
        let result = monitor
            .trigger_initial_indexing(Arc::new(empty_compilation_db))
            .await;
        assert!(result.is_ok());
    }
}
