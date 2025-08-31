//! Component index monitor for managing all index-related state for a single build directory
//!
//! This module provides ComponentIndexMonitor which consolidates index state management,
//! progress tracking, and completion coordination for individual project components.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, trace, warn};

use crate::clangd::index::{IndexLatch, ProgressEvent};
use crate::project::index::reader::IndexReaderTrait;
use crate::project::index::state::IndexState;
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

/// Tracks indexing state for a single component
#[derive(Debug, Clone)]
pub struct ComponentIndexState {
    /// Current indexing state
    pub state: ComponentIndexingState,
    /// Set of files from compilation database that should be indexed
    pub cdb_files: HashSet<PathBuf>,
    /// Total number of CDB files
    pub total_cdb_files: usize,
    /// Number of CDB files currently indexed
    pub indexed_cdb_files: usize,
    /// Last updated timestamp
    pub last_updated: std::time::SystemTime,
}

impl ComponentIndexState {
    /// Create new component index state from compilation database files
    pub fn new(cdb_files: Vec<PathBuf>) -> Self {
        let total_count = cdb_files.len();
        Self {
            state: ComponentIndexingState::Init,
            cdb_files: cdb_files.into_iter().collect(),
            total_cdb_files: total_count,
            indexed_cdb_files: 0,
            last_updated: std::time::SystemTime::now(),
        }
    }

    /// Update the indexing state
    pub fn update_state(&mut self, new_state: ComponentIndexingState) {
        self.state = new_state;
        self.last_updated = std::time::SystemTime::now();
    }

    /// Get current coverage (0.0 to 1.0)
    pub fn coverage(&self) -> f32 {
        if self.total_cdb_files == 0 {
            1.0
        } else {
            self.indexed_cdb_files as f32 / self.total_cdb_files as f32
        }
    }

    /// Check if indexing is complete (all CDB files indexed)
    pub fn is_complete(&self) -> bool {
        self.indexed_cdb_files >= self.total_cdb_files
    }
}

/// Consolidated index state for a single component (behind single mutex)
struct IndexMonitorState {
    /// Core index tracking (file status, coverage calculation)
    index_state: IndexState,

    /// Index file reader for disk synchronization
    index_reader: Arc<dyn IndexReaderTrait>,

    /// Component-level indexing state (Init, InProgress, Partial, Complete)
    component_state: ComponentIndexState,

    /// Synchronization latch for completion waiting
    completion_latch: IndexLatch,
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
}

impl ComponentIndexMonitor {
    /// Create monitor for specific build directory
    pub async fn new(
        build_directory: PathBuf,
        compilation_db: &CompilationDatabase,
        index_reader: Arc<dyn IndexReaderTrait>,
    ) -> Result<Self, ProjectError> {
        // Create index state from compilation database
        #[cfg(test)]
        let index_state = IndexState::from_compilation_db_test(compilation_db);

        #[cfg(not(test))]
        let index_state = IndexState::from_compilation_db(compilation_db).map_err(|e| {
            ProjectError::SessionCreation(format!(
                "Failed to create index state for {}: {}",
                build_directory.display(),
                e
            ))
        })?;

        // Create component index state with CDB files
        let cdb_files = compilation_db
            .source_files()
            .iter()
            .map(|p| p.to_path_buf())
            .collect();
        let component_state = ComponentIndexState::new(cdb_files);

        // Create completion latch
        let completion_latch = IndexLatch::new();

        let monitor_state = IndexMonitorState {
            index_state,
            index_reader,
            component_state,
            completion_latch,
        };

        debug!(
            "Created ComponentIndexMonitor for build dir: {}",
            build_directory.display()
        );

        Ok(Self {
            build_directory,
            state: Arc::new(Mutex::new(monitor_state)),
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
                state.index_state.mark_indexing(&path);
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
                state.index_state.mark_indexed(&path);

                // Update component state: increment indexed CDB file count if this is a CDB file
                if state.component_state.cdb_files.contains(&path) {
                    state.component_state.indexed_cdb_files += 1;
                    debug!(
                        "CDB file indexed: {:?} ({}/{})",
                        path,
                        state.component_state.indexed_cdb_files,
                        state.component_state.total_cdb_files
                    );
                }
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
                state
                    .component_state
                    .update_state(ComponentIndexingState::InProgress(0.0));
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
                state
                    .component_state
                    .update_state(ComponentIndexingState::InProgress(percentage as f32));
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

                // Determine component final state based on CDB coverage
                let component_final_state = if state.component_state.is_complete() {
                    debug!(
                        "All CDB files indexed ({}/{}), transitioning to Completed",
                        state.component_state.indexed_cdb_files,
                        state.component_state.total_cdb_files
                    );
                    ComponentIndexingState::Completed
                } else {
                    debug!(
                        "Partial CDB coverage ({}/{}), transitioning to Partial",
                        state.component_state.indexed_cdb_files,
                        state.component_state.total_cdb_files
                    );
                    ComponentIndexingState::Partial
                };

                // Update component state to final state
                state.component_state.update_state(component_final_state);
                info!(
                    "Component state transitioned to {:?} for {} (coverage: {:.1}%)",
                    state.component_state.state,
                    self.build_directory.display(),
                    state.component_state.coverage() * 100.0
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
        state.component_state.clone()
    }

    /// Get indexing coverage (0.0 to 1.0)
    pub async fn get_coverage(&self) -> f32 {
        let state = self.state.lock().await;
        state.index_state.coverage()
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

    /// Refresh index state by syncing with disk
    pub async fn refresh_from_disk(&self) -> Result<(), ProjectError> {
        debug!(
            "Refreshing index state from disk for build dir: {}",
            self.build_directory.display()
        );

        let mut state = self.state.lock().await;

        // Get compilation database files for this component
        let compilation_db_files = state.index_state.get_unindexed_files();

        // Sync each file's state with disk
        for file_path in &compilation_db_files {
            match state.index_reader.read_index_for_file(file_path).await {
                Ok(index_entry) => {
                    use crate::project::index::reader::FileIndexStatus;
                    match index_entry.status {
                        FileIndexStatus::Done => state.index_state.mark_indexed(file_path),
                        FileIndexStatus::Stale => state.index_state.mark_stale(file_path),
                        FileIndexStatus::InProgress => state.index_state.mark_indexing(file_path),
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

        state.index_state.refresh();

        let stats = state.index_state.get_statistics();
        debug!(
            "Index state after disk sync: {}/{} files indexed",
            stats.compilation_db_indexed, stats.compilation_db_files
        );

        // Trigger completion latch if all files are indexed
        if stats.is_complete() {
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

    /// Get unindexed files needing attention
    #[allow(dead_code)]
    pub async fn get_unindexed_files(&self) -> Vec<PathBuf> {
        let state = self.state.lock().await;
        state.index_state.get_unindexed_files()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clangd::index::ProgressEvent;
    use crate::project::compilation_database::CompilationDatabase;
    use crate::project::index::reader::{
        FileIndexStatus, IndexEntry, IndexReaderTrait, MockIndexReaderTrait,
    };
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

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

    #[tokio::test]
    async fn test_component_monitor_creation() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new(build_dir.clone(), &compilation_db, mock_reader)
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

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, mock_reader)
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

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, mock_reader)
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

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, mock_reader)
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

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, mock_reader)
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
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;

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

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, mock_reader)
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

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, mock_reader)
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

        // Set up expectations using mockall - much more powerful than manual mock
        mock_reader
            .expect_read_index_for_file()
            .times(1) // Expect exactly one call
            .withf(|path| path == Path::new("/test/project/src/main.cpp")) // Verify correct path
            .returning(|_| {
                Box::pin(async move {
                    Ok(IndexEntry {
                        absolute_path: PathBuf::from("/test/project/src/main.cpp"),
                        status: FileIndexStatus::Done,
                        index_format_version: Some(19),
                        expected_format_version: 19,
                        index_content_hash: Some("hash123".to_string()),
                        current_file_hash: Some("hash123".to_string()),
                        symbols: vec!["main".to_string()],
                        index_file_size: Some(1024),
                        index_created_at: Some(SystemTime::now()),
                    })
                })
            });

        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, Arc::new(mock_reader))
            .await
            .expect("Failed to create ComponentIndexMonitor");

        // Refresh from disk - this will automatically verify expectations
        monitor
            .refresh_from_disk()
            .await
            .expect("Failed to refresh from disk");

        // Mockall automatically verifies that expectations were met when mock is dropped
    }

    #[tokio::test]
    async fn test_completion_latch_wait() {
        let mock_reader = Arc::new(MockIndexReaderTrait::new()) as Arc<dyn IndexReaderTrait>;
        let compilation_db = create_test_compilation_db();
        let build_dir = PathBuf::from("/test/project/build");

        let monitor = ComponentIndexMonitor::new(build_dir, &compilation_db, mock_reader)
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

        let monitor =
            ComponentIndexMonitor::new(PathBuf::from("/test/build"), &compilation_db, mock_reader)
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
}
