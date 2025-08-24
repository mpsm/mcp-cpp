//! Index state tracking for compilation database entries
//!
//! This module provides the IndexState component that tracks the indexing
//! status of all files in a compilation database, enabling coverage calculation
//! and progress monitoring.

use crate::project::compilation_database::CompilationDatabase;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

/// File indexing status with simplified states
#[derive(Debug, Clone, PartialEq)]
pub enum FileIndexStatus {
    /// File has not been indexed yet
    None,
    /// File indexing is in progress
    InProgress,
    /// File has been successfully indexed and is current
    Done,
    /// File index exists but is stale (file modified since indexing)
    Stale,
    /// File index is invalid (version mismatch, corrupted, etc.)
    Invalid(String),
}

impl FileIndexStatus {
    /// Check if the status represents a valid, current index
    pub fn is_valid(&self) -> bool {
        matches!(self, FileIndexStatus::Done)
    }

    /// Check if the file needs indexing
    pub fn needs_indexing(&self) -> bool {
        matches!(
            self,
            FileIndexStatus::None | FileIndexStatus::Stale | FileIndexStatus::Invalid(_)
        )
    }
}

/// Metadata about a tracked file
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// Absolute path to the file
    pub path: PathBuf,
    /// Whether this file is from the compilation database
    pub is_compilation_db_entry: bool,
    /// Current indexing status
    pub status: FileIndexStatus,
    /// Optional content hash for staleness detection
    pub content_hash: Option<String>,
    /// Timestamp when status was last updated
    pub last_updated: std::time::SystemTime,
}

impl FileMetadata {
    /// Create metadata for a compilation database file
    pub fn from_compilation_db(path: PathBuf) -> Self {
        Self {
            path,
            is_compilation_db_entry: true,
            status: FileIndexStatus::None,
            content_hash: None,
            last_updated: std::time::SystemTime::now(),
        }
    }

    /// Create metadata for a non-compilation database file
    pub fn from_discovered_file(path: PathBuf) -> Self {
        Self {
            path,
            is_compilation_db_entry: false,
            status: FileIndexStatus::None,
            content_hash: None,
            last_updated: std::time::SystemTime::now(),
        }
    }

    /// Update the indexing status
    pub fn update_status(&mut self, status: FileIndexStatus) {
        self.status = status;
        self.last_updated = std::time::SystemTime::now();
        trace!("Updated status for {:?}: {:?}", self.path, self.status);
    }
}

/// Index state manager for tracking compilation database indexing status
pub struct IndexState {
    /// Map of file paths to their metadata
    files: HashMap<PathBuf, FileMetadata>,
    /// Total number of files from compilation database
    compilation_db_file_count: usize,
    /// Last time the state was refreshed
    last_refresh: std::time::SystemTime,
}

impl IndexState {
    /// Create a new empty index state
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            compilation_db_file_count: 0,
            last_refresh: std::time::SystemTime::now(),
        }
    }

    /// Create index state from a compilation database
    pub fn from_compilation_db(comp_db: &CompilationDatabase) -> Result<Self, std::io::Error> {
        let mut state = Self::new();

        debug!(
            "Creating index state from compilation database with {} entries",
            comp_db.entries.len()
        );

        // Add all compilation database entries
        for entry in &comp_db.entries {
            let absolute_path = entry.file.canonicalize()?;
            let metadata = FileMetadata::from_compilation_db(absolute_path.clone());
            state.files.insert(absolute_path, metadata);
        }

        state.compilation_db_file_count = comp_db.entries.len();

        debug!(
            "Index state created with {} compilation database files",
            state.compilation_db_file_count
        );
        Ok(state)
    }

    /// Add a file to be tracked (not from compilation database)
    pub fn add_file(&mut self, path: PathBuf, is_compilation_db_entry: bool) {
        let absolute_path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path, // Use original path if canonicalization fails
        };

        if !self.files.contains_key(&absolute_path) {
            let metadata = if is_compilation_db_entry {
                FileMetadata::from_compilation_db(absolute_path.clone())
            } else {
                FileMetadata::from_discovered_file(absolute_path.clone())
            };

            self.files.insert(absolute_path.clone(), metadata);

            if is_compilation_db_entry {
                self.compilation_db_file_count += 1;
            }

            trace!(
                "Added file to index state: {:?} (cdb: {})",
                absolute_path, is_compilation_db_entry
            );
        }
    }

    /// Mark a file as being indexed
    pub fn mark_indexing(&mut self, path: &Path) {
        if let Some(metadata) = self.files.get_mut(path) {
            metadata.update_status(FileIndexStatus::InProgress);
        }
    }

    /// Mark a file as successfully indexed
    pub fn mark_indexed(&mut self, path: &Path) {
        if let Some(metadata) = self.files.get_mut(path) {
            metadata.update_status(FileIndexStatus::Done);
            debug!("Marked file as indexed: {:?}", path);
        }
    }

    /// Mark a file as stale
    pub fn mark_stale(&mut self, path: &Path) {
        if let Some(metadata) = self.files.get_mut(path) {
            metadata.update_status(FileIndexStatus::Stale);
        }
    }

    /// Mark a file as invalid
    pub fn mark_invalid(&mut self, path: &Path, reason: String) {
        if let Some(metadata) = self.files.get_mut(path) {
            metadata.update_status(FileIndexStatus::Invalid(reason));
        }
    }

    /// Get the status of a specific file
    pub fn get_status(&self, path: &Path) -> FileIndexStatus {
        self.files
            .get(path)
            .map(|metadata| metadata.status.clone())
            .unwrap_or(FileIndexStatus::None)
    }

    /// Check if a file is indexed
    pub fn is_indexed(&self, path: &Path) -> bool {
        matches!(self.get_status(path), FileIndexStatus::Done)
    }

    /// Get total number of tracked files
    pub fn total_files(&self) -> usize {
        self.files.len()
    }

    /// Get number of successfully indexed files
    pub fn indexed_files(&self) -> usize {
        self.files
            .values()
            .filter(|metadata| metadata.status.is_valid())
            .count()
    }

    /// Get number of files from compilation database
    pub fn compilation_db_files(&self) -> usize {
        self.compilation_db_file_count
    }

    /// Get indexing coverage as a percentage (0.0 to 1.0)
    pub fn coverage(&self) -> f32 {
        if self.compilation_db_file_count == 0 {
            return 0.0;
        }

        let indexed_cdb_files = self
            .files
            .values()
            .filter(|metadata| metadata.is_compilation_db_entry && metadata.status.is_valid())
            .count();

        indexed_cdb_files as f32 / self.compilation_db_file_count as f32
    }

    /// Get list of unindexed files from compilation database
    pub fn get_unindexed_files(&self) -> Vec<PathBuf> {
        self.files
            .values()
            .filter(|metadata| metadata.is_compilation_db_entry && metadata.status.needs_indexing())
            .map(|metadata| metadata.path.clone())
            .collect()
    }

    /// Get list of stale files
    pub fn get_stale_files(&self) -> Vec<PathBuf> {
        self.files
            .values()
            .filter(|metadata| matches!(metadata.status, FileIndexStatus::Stale))
            .map(|metadata| metadata.path.clone())
            .collect()
    }

    /// Get detailed statistics
    pub fn get_statistics(&self) -> IndexStatistics {
        let mut stats = IndexStatistics {
            total_files: self.files.len(),
            compilation_db_files: self.compilation_db_file_count,
            ..Default::default()
        };

        for metadata in self.files.values() {
            match &metadata.status {
                FileIndexStatus::None => stats.not_indexed += 1,
                FileIndexStatus::InProgress => stats.in_progress += 1,
                FileIndexStatus::Done => stats.indexed += 1,
                FileIndexStatus::Stale => stats.stale += 1,
                FileIndexStatus::Invalid(_) => stats.invalid += 1,
            }

            if metadata.is_compilation_db_entry && metadata.status == FileIndexStatus::Done {
                stats.compilation_db_indexed += 1;
            }
        }

        stats.coverage = if stats.compilation_db_files > 0 {
            stats.compilation_db_indexed as f32 / stats.compilation_db_files as f32
        } else {
            0.0
        };

        stats
    }

    /// Refresh timestamps
    pub fn refresh(&mut self) {
        self.last_refresh = std::time::SystemTime::now();
    }

    /// Get last refresh time
    pub fn last_refresh(&self) -> std::time::SystemTime {
        self.last_refresh
    }
}

impl Default for IndexState {
    fn default() -> Self {
        Self::new()
    }
}

/// Detailed indexing statistics
#[derive(Debug, Default, Clone)]
pub struct IndexStatistics {
    /// Total number of tracked files
    pub total_files: usize,
    /// Number of files from compilation database
    pub compilation_db_files: usize,
    /// Number of compilation database files that are indexed
    pub compilation_db_indexed: usize,
    /// Number of files not yet indexed
    pub not_indexed: usize,
    /// Number of files currently being indexed
    pub in_progress: usize,
    /// Number of successfully indexed files
    pub indexed: usize,
    /// Number of stale index files
    pub stale: usize,
    /// Number of invalid index files
    pub invalid: usize,
    /// Indexing coverage (0.0 to 1.0)
    pub coverage: f32,
}

impl IndexStatistics {
    /// Check if indexing is complete
    pub fn is_complete(&self) -> bool {
        self.coverage >= 1.0
    }

    /// Get human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "Index: {}/{} files ({}%), {} stale, {} invalid",
            self.compilation_db_indexed,
            self.compilation_db_files,
            (self.coverage * 100.0) as u32,
            self.stale,
            self.invalid
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::compilation_database::CompilationDatabase;
    use json_compilation_db::Entry;

    fn create_test_compilation_db() -> CompilationDatabase {
        CompilationDatabase {
            path: PathBuf::from("/project/compile_commands.json"),
            entries: vec![
                Entry {
                    directory: PathBuf::from("/project"),
                    file: PathBuf::from("file1.cpp"),
                    arguments: vec!["g++".to_string(), "-c".to_string(), "file1.cpp".to_string()],
                    output: None,
                },
                Entry {
                    directory: PathBuf::from("/project"),
                    file: PathBuf::from("file2.cpp"),
                    arguments: vec!["g++".to_string(), "-c".to_string(), "file2.cpp".to_string()],
                    output: None,
                },
            ],
        }
    }

    #[test]
    fn test_file_index_status() {
        assert!(FileIndexStatus::Done.is_valid());
        assert!(!FileIndexStatus::None.is_valid());
        assert!(!FileIndexStatus::Stale.is_valid());

        assert!(FileIndexStatus::None.needs_indexing());
        assert!(FileIndexStatus::Stale.needs_indexing());
        assert!(!FileIndexStatus::Done.needs_indexing());
    }

    #[test]
    fn test_file_metadata_creation() {
        let path = PathBuf::from("/test/file.cpp");

        let cdb_metadata = FileMetadata::from_compilation_db(path.clone());
        assert!(cdb_metadata.is_compilation_db_entry);
        assert!(matches!(cdb_metadata.status, FileIndexStatus::None));

        let discovered_metadata = FileMetadata::from_discovered_file(path.clone());
        assert!(!discovered_metadata.is_compilation_db_entry);
    }

    #[test]
    fn test_empty_index_state() {
        let state = IndexState::new();

        assert_eq!(state.total_files(), 0);
        assert_eq!(state.indexed_files(), 0);
        assert_eq!(state.compilation_db_files(), 0);
        assert_eq!(state.coverage(), 0.0);
    }

    #[test]
    fn test_add_files() {
        let mut state = IndexState::new();

        state.add_file(PathBuf::from("/test/file1.cpp"), true);
        state.add_file(PathBuf::from("/test/file2.cpp"), false);

        assert_eq!(state.total_files(), 2);
        assert_eq!(state.compilation_db_files(), 1);
        assert_eq!(state.coverage(), 0.0); // None indexed yet
    }

    #[test]
    fn test_mark_operations() {
        let mut state = IndexState::new();
        let file_path = PathBuf::from("/test/file.cpp");

        state.add_file(file_path.clone(), true);

        // Test marking as in progress
        state.mark_indexing(&file_path);
        assert!(matches!(
            state.get_status(&file_path),
            FileIndexStatus::InProgress
        ));

        // Test marking as indexed
        state.mark_indexed(&file_path);
        assert!(matches!(
            state.get_status(&file_path),
            FileIndexStatus::Done
        ));
        assert!(state.is_indexed(&file_path));

        // Test marking as stale
        state.mark_stale(&file_path);
        assert!(matches!(
            state.get_status(&file_path),
            FileIndexStatus::Stale
        ));

        // Test marking as invalid
        state.mark_invalid(&file_path, "test error".to_string());
        assert!(matches!(
            state.get_status(&file_path),
            FileIndexStatus::Invalid(_)
        ));
    }

    #[test]
    fn test_coverage_calculation() {
        let mut state = IndexState::new();

        // Add 3 compilation database files
        state.add_file(PathBuf::from("/test/file1.cpp"), true);
        state.add_file(PathBuf::from("/test/file2.cpp"), true);
        state.add_file(PathBuf::from("/test/file3.cpp"), true);

        assert_eq!(state.coverage(), 0.0);

        // Index one file
        state.mark_indexed(&PathBuf::from("/test/file1.cpp"));
        assert!((state.coverage() - 1.0 / 3.0).abs() < f32::EPSILON);

        // Index all files
        state.mark_indexed(&PathBuf::from("/test/file2.cpp"));
        state.mark_indexed(&PathBuf::from("/test/file3.cpp"));
        assert!((state.coverage() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_get_unindexed_files() {
        let mut state = IndexState::new();

        let file1 = PathBuf::from("/test/file1.cpp");
        let file2 = PathBuf::from("/test/file2.cpp");
        let file3 = PathBuf::from("/test/file3.cpp");

        state.add_file(file1.clone(), true);
        state.add_file(file2.clone(), true);
        state.add_file(file3.clone(), false); // Not from compilation DB

        let unindexed = state.get_unindexed_files();
        assert_eq!(unindexed.len(), 2); // Only compilation DB files

        state.mark_indexed(&file1);
        let unindexed = state.get_unindexed_files();
        assert_eq!(unindexed.len(), 1);
        assert!(unindexed.contains(&file2));
    }

    #[test]
    fn test_statistics() {
        let mut state = IndexState::new();

        state.add_file(PathBuf::from("/test/file1.cpp"), true);
        state.add_file(PathBuf::from("/test/file2.cpp"), true);

        state.mark_indexed(&PathBuf::from("/test/file1.cpp"));
        state.mark_stale(&PathBuf::from("/test/file2.cpp"));

        let stats = state.get_statistics();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.compilation_db_files, 2);
        assert_eq!(stats.indexed, 1);
        assert_eq!(stats.stale, 1);
        assert_eq!(stats.compilation_db_indexed, 1);
        assert!((stats.coverage - 0.5).abs() < f32::EPSILON);

        assert!(!stats.is_complete());
        assert!(stats.summary().contains("50%"));
    }
}
