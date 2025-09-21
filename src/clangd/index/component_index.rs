//! Component index management for tracking file indexing states
//!
//! This module provides ComponentIndex which is a pure data structure for managing
//! the indexing state of files in a compilation database. It maps source files to
//! their index files and tracks the indexing status of each file without complex logic.

use super::hash::compute_file_hash;
use crate::clangd::version::ClangdVersion;
use crate::project::CompilationDatabase;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ComponentIndexError {
    #[error("Index directory does not exist: {path}")]
    IndexDirectoryNotFound { path: String },
    #[error("Unable to access index directory: {path}")]
    IndexDirectoryAccess { path: String },
    #[error("File not found in index: {path}")]
    FileNotFound { path: String },
    #[error("Path canonicalization failed for {path}: {error}")]
    PathCanonicalization { path: String, error: String },
}

/// File indexing states
#[derive(Debug, Clone, PartialEq)]
pub enum FileIndexState {
    /// File is pending indexing (not yet processed)
    Pending,
    /// File is currently being indexed
    InProgress,
    /// File has been successfully indexed
    Indexed,
    /// File indexing failed with error message
    Failed(String),
}

impl Default for FileIndexState {
    fn default() -> Self {
        Self::Pending
    }
}

/// Comprehensive indexing summary with detailed state information
#[derive(Debug, Clone)]
pub struct IndexingSummary {
    /// Total number of files in compilation database
    pub total_files: usize,
    /// Number of files successfully indexed
    pub indexed_count: usize,
    /// Number of files pending indexing
    pub pending_count: usize,
    /// Number of files currently being indexed
    pub in_progress_count: usize,
    /// Number of files that failed indexing
    pub failed_count: usize,
    /// Current coverage ratio (0.0 to 1.0)
    pub coverage: f32,
    /// Whether all files are indexed
    pub is_fully_indexed: bool,
    /// Whether any files are currently being processed
    pub has_active_indexing: bool,
    /// List of files that are pending indexing
    pub pending_files: Vec<PathBuf>,
    /// List of files currently being indexed
    pub in_progress_files: Vec<PathBuf>,
    /// List of files that have been indexed
    pub indexed_files: Vec<PathBuf>,
    /// List of files that failed with their error messages
    pub failed_files: Vec<(PathBuf, String)>,
}

/// Pure data structure for managing component index state
///
/// ComponentIndex is responsible for:
/// - Mapping source files to their index file paths
/// - Tracking indexing status for each compilation database file
/// - Providing simple queries and updates for file states
/// - Calculating coverage statistics
/// - Finding next files to process
///
/// This structure contains no complex logic - it's purely for data management.
pub struct ComponentIndex {
    /// Path to the index directory (.cache/clangd/index/)
    index_dir: PathBuf,
    /// Mapping from source file path to index file path
    file_to_index: HashMap<PathBuf, PathBuf>,
    /// Current indexing state for each file
    file_states: HashMap<PathBuf, FileIndexState>,
    /// Set of files from compilation database that should be indexed
    cdb_files: HashSet<PathBuf>,
    /// Clangd version for hash function selection
    format_version: u32,
}

impl ComponentIndex {
    /// Create a new ComponentIndex from a compilation database and clangd version
    ///
    /// All files are initialized as Pending - the ComponentIndexMonitor is responsible
    /// for checking disk state and updating file states appropriately.
    pub fn new(
        compilation_db: &CompilationDatabase,
        clangd_version: &ClangdVersion,
    ) -> Result<Self, ComponentIndexError> {
        let compilation_db_path = compilation_db.path();
        let compilation_db_dir = compilation_db_path
            .parent()
            .unwrap_or_else(|| Path::new("."));

        // Index directory is .cache/clangd/index/ relative to compilation database
        let index_dir = compilation_db_dir
            .join(".cache")
            .join("clangd")
            .join("index");

        let format_version = clangd_version.index_format_version();
        let mut file_to_index = HashMap::new();
        let mut file_states = HashMap::new();

        // Get canonical source files using the single source of truth for path canonicalization
        let canonical_files = compilation_db.canonical_source_files().map_err(|e| {
            ComponentIndexError::PathCanonicalization {
                path: "compilation database".to_string(),
                error: e.to_string(),
            }
        })?;

        let cdb_files: HashSet<PathBuf> = canonical_files.iter().cloned().collect();

        // Build mapping for each canonical file
        for canonical_source_file in canonical_files {
            // Compute hash for the canonical source file path
            let file_path_str = canonical_source_file.to_string_lossy();
            let hash = compute_file_hash(&file_path_str, format_version);

            // Extract basename from canonical path
            let basename = canonical_source_file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");

            // Construct index filename: basename.hash.idx
            let index_filename = format!("{basename}.{hash:016X}.idx");
            let index_path = index_dir.join(&index_filename);

            // Add mapping using canonical path as key
            file_to_index.insert(canonical_source_file.clone(), index_path);

            // Initialize all files as pending - ComponentIndexMonitor will update states based on disk
            file_states.insert(canonical_source_file, FileIndexState::Pending);
        }

        Ok(ComponentIndex {
            index_dir,
            file_to_index,
            file_states,
            cdb_files,
            format_version,
        })
    }

    /// Create a new ComponentIndex for testing without filesystem dependencies
    #[cfg(test)]
    pub fn new_for_test(
        compilation_db: &CompilationDatabase,
        clangd_version: &ClangdVersion,
    ) -> Self {
        let format_version = clangd_version.index_format_version();
        let mut file_to_index = HashMap::new();
        let mut file_states = HashMap::new();
        let mut cdb_files = HashSet::new();

        // Build mapping for each file in the compilation database
        for entry in compilation_db.entries() {
            let source_file = &entry.file;
            cdb_files.insert(source_file.to_path_buf());

            // Compute hash for the source file path
            let file_path_str = source_file.to_string_lossy();
            let hash = compute_file_hash(&file_path_str, format_version);

            // Extract basename from path
            let basename = source_file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");

            // Construct index filename: basename.hash.idx (use a fake directory for tests)
            let index_filename = format!("{basename}.{hash:016X}.idx");
            let fake_index_dir = PathBuf::from("/fake/test/index");
            let index_path = fake_index_dir.join(&index_filename);

            // Add mapping - assume all files are pending initially in tests
            file_to_index.insert(source_file.to_path_buf(), index_path);
            file_states.insert(source_file.to_path_buf(), FileIndexState::Pending);
        }

        ComponentIndex {
            index_dir: PathBuf::from("/fake/test/index"),
            file_to_index,
            file_states,
            cdb_files,
            format_version,
        }
    }

    // File State Management Methods

    /// Mark a file as currently being indexed
    pub fn mark_file_in_progress(&mut self, source_file: &Path) -> bool {
        if let Some(state) = self.file_states.get_mut(source_file) {
            *state = FileIndexState::InProgress;
            true
        } else {
            false
        }
    }

    /// Mark a file as successfully indexed
    pub fn mark_file_indexed(&mut self, source_file: &Path) -> bool {
        if let Some(state) = self.file_states.get_mut(source_file) {
            *state = FileIndexState::Indexed;
            true
        } else {
            false
        }
    }

    /// Mark a file as failed to index with error message
    pub fn mark_file_failed(&mut self, source_file: &Path, error: String) -> bool {
        if let Some(state) = self.file_states.get_mut(source_file) {
            *state = FileIndexState::Failed(error);
            true
        } else {
            false
        }
    }

    /// Reset a file's state back to pending
    pub fn mark_file_pending(&mut self, source_file: &Path) -> bool {
        if let Some(state) = self.file_states.get_mut(source_file) {
            *state = FileIndexState::Pending;
            true
        } else {
            false
        }
    }

    // Query Methods

    /// Get the indexing state of a file
    pub fn get_file_state(&self, source_file: &Path) -> Option<&FileIndexState> {
        self.file_states.get(source_file)
    }

    /// Check if a source file has been indexed (index file exists and state is Indexed)
    pub fn is_file_indexed(&self, source_file: &Path) -> bool {
        matches!(
            self.file_states.get(source_file),
            Some(FileIndexState::Indexed)
        )
    }

    /// Check if a source file is currently being indexed
    pub fn is_file_in_progress(&self, source_file: &Path) -> bool {
        matches!(
            self.file_states.get(source_file),
            Some(FileIndexState::InProgress)
        )
    }

    /// Check if a source file is pending indexing
    pub fn is_file_pending(&self, source_file: &Path) -> bool {
        matches!(
            self.file_states.get(source_file),
            Some(FileIndexState::Pending)
        )
    }

    /// Check if a source file indexing failed
    pub fn is_file_failed(&self, source_file: &Path) -> bool {
        matches!(
            self.file_states.get(source_file),
            Some(FileIndexState::Failed(_))
        )
    }

    /// Get the next file that needs indexing (in Pending state)
    pub fn get_next_uncovered_file(&self) -> Option<&Path> {
        self.cdb_files
            .iter()
            .find(|file| {
                matches!(
                    self.file_states.get(file.as_path()),
                    Some(FileIndexState::Pending)
                )
            })
            .map(|p| p.as_path())
    }

    /// Get all files in pending state
    pub fn get_pending_files(&self) -> Vec<&Path> {
        self.cdb_files
            .iter()
            .filter(|file| {
                matches!(
                    self.file_states.get(file.as_path()),
                    Some(FileIndexState::Pending)
                )
            })
            .map(|p| p.as_path())
            .collect()
    }

    /// Get all files in in-progress state
    pub fn get_in_progress_files(&self) -> Vec<&Path> {
        self.cdb_files
            .iter()
            .filter(|file| {
                matches!(
                    self.file_states.get(file.as_path()),
                    Some(FileIndexState::InProgress)
                )
            })
            .map(|p| p.as_path())
            .collect()
    }

    /// Get all files in indexed state
    pub fn get_indexed_files(&self) -> Vec<&Path> {
        self.cdb_files
            .iter()
            .filter(|file| {
                matches!(
                    self.file_states.get(file.as_path()),
                    Some(FileIndexState::Indexed)
                )
            })
            .map(|p| p.as_path())
            .collect()
    }

    /// Get all files in failed state
    pub fn get_failed_files(&self) -> Vec<(&Path, &String)> {
        self.cdb_files
            .iter()
            .filter_map(|file| {
                if let Some(FileIndexState::Failed(error)) = self.file_states.get(file.as_path()) {
                    Some((file.as_path(), error))
                } else {
                    None
                }
            })
            .collect()
    }

    // Statistics and Coverage Methods

    /// Get the number of files that have been indexed
    pub fn indexed_count(&self) -> usize {
        self.file_states
            .values()
            .filter(|state| matches!(state, FileIndexState::Indexed))
            .count()
    }

    /// Get the number of files that are pending indexing
    pub fn pending_count(&self) -> usize {
        self.file_states
            .values()
            .filter(|state| matches!(state, FileIndexState::Pending))
            .count()
    }

    /// Get the number of files that are currently being indexed
    pub fn in_progress_count(&self) -> usize {
        self.file_states
            .values()
            .filter(|state| matches!(state, FileIndexState::InProgress))
            .count()
    }

    /// Get the number of files that failed to index
    pub fn failed_count(&self) -> usize {
        self.file_states
            .values()
            .filter(|state| matches!(state, FileIndexState::Failed(_)))
            .count()
    }

    /// Get the total number of compilation database files
    pub fn total_files_count(&self) -> usize {
        self.cdb_files.len()
    }

    /// Get current indexing coverage as a ratio (0.0 to 1.0)
    pub fn coverage(&self) -> f32 {
        let total = self.total_files_count();
        if total == 0 {
            1.0
        } else {
            self.indexed_count() as f32 / total as f32
        }
    }

    /// Check if all files are indexed
    pub fn is_fully_indexed(&self) -> bool {
        self.pending_count() == 0 && self.in_progress_count() == 0
    }

    /// Check if any files are currently being processed
    pub fn has_active_indexing(&self) -> bool {
        self.in_progress_count() > 0
    }

    // File and index path methods

    /// Get the index file path for a given source file
    pub fn get_index_file(&self, source_file: &Path) -> Option<&Path> {
        self.file_to_index.get(source_file).map(|p| p.as_path())
    }

    /// Get all source files that are part of the compilation database
    pub fn source_files(&self) -> Vec<&Path> {
        self.cdb_files.iter().map(|p| p.as_path()).collect()
    }

    /// Get the index directory path
    pub fn index_directory(&self) -> &Path {
        &self.index_dir
    }

    /// Get the format version used for this index
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Get all index file paths
    pub fn index_files(&self) -> Vec<&Path> {
        self.file_to_index.values().map(|p| p.as_path()).collect()
    }

    /// Get comprehensive indexing summary with detailed state information
    pub fn get_indexing_summary(&self) -> IndexingSummary {
        let pending_files: Vec<_> = self
            .get_pending_files()
            .iter()
            .map(|p| p.to_path_buf())
            .collect();
        let in_progress_files: Vec<_> = self
            .get_in_progress_files()
            .iter()
            .map(|p| p.to_path_buf())
            .collect();
        let indexed_files: Vec<_> = self
            .get_indexed_files()
            .iter()
            .map(|p| p.to_path_buf())
            .collect();
        let failed_files: Vec<_> = self
            .get_failed_files()
            .iter()
            .map(|(path, error)| (path.to_path_buf(), (*error).clone()))
            .collect();

        IndexingSummary {
            total_files: self.total_files_count(),
            indexed_count: self.indexed_count(),
            pending_count: self.pending_count(),
            in_progress_count: self.in_progress_count(),
            failed_count: self.failed_count(),
            coverage: self.coverage(),
            is_fully_indexed: self.is_fully_indexed(),
            has_active_indexing: self.has_active_indexing(),
            pending_files,
            in_progress_files,
            indexed_files,
            failed_files,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clangd::version::ClangdVersion;
    use crate::project::CompilationDatabase;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_compilation_database(dir: &Path) -> std::io::Result<CompilationDatabase> {
        let compile_commands_path = dir.join("compile_commands.json");
        let mut file = fs::File::create(&compile_commands_path)?;

        let content = r#"[
            {
                "directory": "/test/project",
                "command": "clang++ -c main.cpp -o main.o",
                "file": "/test/project/main.cpp"
            },
            {
                "directory": "/test/project",
                "command": "clang++ -c utils.cpp -o utils.o", 
                "file": "/test/project/utils.cpp"
            }
        ]"#;

        file.write_all(content.as_bytes())?;
        Ok(CompilationDatabase::new(compile_commands_path).unwrap())
    }

    fn create_test_version() -> ClangdVersion {
        ClangdVersion {
            major: 18,
            minor: 1,
            patch: 8,
            variant: None,
            date: None,
        }
    }

    #[test]
    fn test_component_index_creation() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let build_dir = temp_dir.path();

        let compilation_db = create_test_compilation_database(build_dir)?;

        let version = create_test_version();
        let component_index = ComponentIndex::new(&compilation_db, &version).unwrap();

        // All files should start as pending (pure in-memory, no disk checks)
        assert_eq!(component_index.total_files_count(), 2);
        assert_eq!(component_index.pending_count(), 2);
        assert_eq!(component_index.indexed_count(), 0);
        assert_eq!(component_index.coverage(), 0.0);
        assert!(!component_index.is_fully_indexed());

        Ok(())
    }

    #[test]
    fn test_file_state_management() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let build_dir = temp_dir.path();

        let compilation_db = create_test_compilation_database(build_dir)?;

        let version = create_test_version();
        let mut component_index = ComponentIndex::new(&compilation_db, &version).unwrap();

        let main_cpp = Path::new("/test/project/main.cpp");

        // Test marking file as in progress
        assert!(component_index.mark_file_in_progress(main_cpp));
        assert!(component_index.is_file_in_progress(main_cpp));
        assert_eq!(component_index.in_progress_count(), 1);

        // Test marking file as indexed
        assert!(component_index.mark_file_indexed(main_cpp));
        assert!(component_index.is_file_indexed(main_cpp));
        assert_eq!(component_index.indexed_count(), 1);
        assert_eq!(component_index.coverage(), 0.5);

        // Test marking file as failed
        assert!(component_index.mark_file_failed(main_cpp, "Test error".to_string()));
        assert!(component_index.is_file_failed(main_cpp));
        assert_eq!(component_index.failed_count(), 1);

        // Test marking file as pending again
        assert!(component_index.mark_file_pending(main_cpp));
        assert!(component_index.is_file_pending(main_cpp));
        assert_eq!(component_index.pending_count(), 2);

        Ok(())
    }

    #[test]
    fn test_next_uncovered_file() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let build_dir = temp_dir.path();

        let compilation_db = create_test_compilation_database(build_dir)?;

        let version = create_test_version();
        let mut component_index = ComponentIndex::new(&compilation_db, &version).unwrap();

        // Should return one of the pending files (all start as pending now)
        let next_file = component_index.get_next_uncovered_file();
        assert!(next_file.is_some());

        // Mark first file as indexed
        let main_cpp = Path::new("/test/project/main.cpp");
        component_index.mark_file_indexed(main_cpp);

        // Should still return the remaining pending file
        let next_file = component_index.get_next_uncovered_file();
        assert!(next_file.is_some());
        assert_ne!(next_file.unwrap(), main_cpp);

        // Mark second file as indexed
        let utils_cpp = Path::new("/test/project/utils.cpp");
        component_index.mark_file_indexed(utils_cpp);

        // Should return None when all files are indexed
        assert!(component_index.get_next_uncovered_file().is_none());
        assert!(component_index.is_fully_indexed());

        Ok(())
    }

    #[test]
    fn test_file_collections() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let build_dir = temp_dir.path();

        let compilation_db = create_test_compilation_database(build_dir)?;

        let version = create_test_version();
        let mut component_index = ComponentIndex::new(&compilation_db, &version).unwrap();

        let main_cpp = Path::new("/test/project/main.cpp");
        let utils_cpp = Path::new("/test/project/utils.cpp");

        // Set different states
        component_index.mark_file_in_progress(main_cpp);
        component_index.mark_file_failed(utils_cpp, "Test failure".to_string());

        // Test collections
        let in_progress = component_index.get_in_progress_files();
        assert_eq!(in_progress.len(), 1);
        assert_eq!(in_progress[0], main_cpp);

        let failed = component_index.get_failed_files();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, utils_cpp);
        assert_eq!(failed[0].1, "Test failure");

        let pending = component_index.get_pending_files();
        assert_eq!(pending.len(), 0);

        Ok(())
    }

    #[test]
    fn test_get_indexing_summary() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let build_dir = temp_dir.path();

        let compilation_db = create_test_compilation_database(build_dir)?;

        let version = create_test_version();
        let mut component_index = ComponentIndex::new(&compilation_db, &version).unwrap();

        let main_cpp = Path::new("/test/project/main.cpp");
        let utils_cpp = Path::new("/test/project/utils.cpp");

        // Set up different states
        component_index.mark_file_in_progress(main_cpp);
        component_index.mark_file_failed(utils_cpp, "Test error".to_string());

        let summary = component_index.get_indexing_summary();

        // Verify summary contents
        assert_eq!(summary.total_files, 2);
        assert_eq!(summary.indexed_count, 0);
        assert_eq!(summary.pending_count, 0);
        assert_eq!(summary.in_progress_count, 1);
        assert_eq!(summary.failed_count, 1);
        assert_eq!(summary.coverage, 0.0);
        assert!(!summary.is_fully_indexed);
        assert!(summary.has_active_indexing);

        // Verify file lists
        assert_eq!(summary.in_progress_files.len(), 1);
        assert_eq!(summary.in_progress_files[0], main_cpp);
        assert_eq!(summary.failed_files.len(), 1);
        assert_eq!(summary.failed_files[0].0, utils_cpp);
        assert_eq!(summary.failed_files[0].1, "Test error");

        Ok(())
    }
}
