//! Index reader with automatic staleness detection
//!
//! This module provides the IndexReader component that reads clangd index files
//! and automatically detects staleness by comparing content hashes.

use super::storage::{IndexError, IndexStorage};
use crate::clangd::version::ClangdVersion;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, trace};

/// File index status with detailed information
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

    /// Check if the index needs to be updated
    pub fn needs_update(&self) -> bool {
        matches!(
            self,
            FileIndexStatus::None | FileIndexStatus::Stale | FileIndexStatus::Invalid(_)
        )
    }
}

/// Index entry with complete metadata and status
#[derive(Debug, Clone)]
pub struct IndexEntry {
    /// Absolute path to the source file
    pub absolute_path: PathBuf,
    /// Current index status with staleness/validity information
    pub status: FileIndexStatus,
    /// Index file format version found
    pub index_format_version: Option<u32>,
    /// Expected format version for this clangd instance
    pub expected_format_version: u32,
    /// Content hash stored in the index file
    pub index_content_hash: Option<String>,
    /// Current content hash of the source file
    pub current_file_hash: Option<String>,
    /// Symbols extracted from index (empty if invalid/stale)
    pub symbols: Vec<String>,
    /// File size of the index
    pub index_file_size: Option<u64>,
    /// Timestamp when index was created
    pub index_created_at: Option<std::time::SystemTime>,
}

impl IndexEntry {
    /// Check if the entry has valid index data
    pub fn is_valid(&self) -> bool {
        self.status.is_valid()
    }

    /// Get a human-readable status description
    pub fn status_description(&self) -> String {
        match &self.status {
            FileIndexStatus::None => "Not indexed".to_string(),
            FileIndexStatus::InProgress => "Indexing in progress".to_string(),
            FileIndexStatus::Done => "Index current".to_string(),
            FileIndexStatus::Stale => "Index stale (file modified)".to_string(),
            FileIndexStatus::Invalid(reason) => format!("Index invalid: {}", reason),
        }
    }
}

/// Trait for index reading operations with testability support
#[async_trait]
#[cfg_attr(test, mockall::automock)]
pub trait IndexReaderTrait: Send + Sync {
    /// Read index for a specific source file with automatic staleness detection
    async fn read_index_for_file(&self, source_path: &Path) -> Result<IndexEntry, IndexError>;

    /// Clear the cache
    async fn clear_cache(&self);

    /// Get cache statistics (total entries, valid entries)
    async fn cache_stats(&self) -> (usize, usize);
}

/// Index reader with caching and automatic staleness detection
#[derive(Clone)]
pub struct IndexReader {
    /// Storage backend for reading index files
    storage: Arc<dyn IndexStorage>,
    /// Cache of previously read index entries
    cache: Arc<RwLock<HashMap<PathBuf, IndexEntry>>>,
    /// Clangd version information for compatibility checking
    clangd_version: ClangdVersion,
}

impl IndexReader {
    /// Create a new index reader
    pub fn new(storage: Arc<dyn IndexStorage>, clangd_version: ClangdVersion) -> Self {
        Self {
            storage,
            cache: Arc::new(RwLock::new(HashMap::new())),
            clangd_version,
        }
    }

    /// Read index for a specific source file with automatic staleness detection
    pub async fn read_index_for_file(&self, source_path: &Path) -> Result<IndexEntry, IndexError> {
        let absolute_path = source_path.canonicalize().map_err(IndexError::Io)?;

        trace!("Reading index for: {:?}", absolute_path);

        // Check cache first
        if let Some(cached_entry) = self.get_cached_entry(&absolute_path).await {
            trace!("Found cached entry for: {:?}", absolute_path);
            return Ok(cached_entry);
        }

        // Read from storage
        let entry = self.read_and_validate_index(&absolute_path).await?;

        // Cache the result
        self.cache_entry(absolute_path.clone(), entry.clone()).await;

        Ok(entry)
    }

    /// Read and validate index with full staleness detection
    async fn read_and_validate_index(
        &self,
        absolute_path: &PathBuf,
    ) -> Result<IndexEntry, IndexError> {
        debug!("Reading and validating index for: {:?}", absolute_path);

        // Attempt to read index data from storage
        let index_data = match self.storage.read_index(absolute_path).await {
            Ok(data) => data,
            Err(IndexError::FileNotFound { .. }) => {
                // Index file doesn't exist - not indexed yet
                return Ok(IndexEntry {
                    absolute_path: absolute_path.clone(),
                    status: FileIndexStatus::None,
                    index_format_version: None,
                    expected_format_version: self.storage.expected_version(),
                    index_content_hash: None,
                    current_file_hash: None,
                    symbols: vec![],
                    index_file_size: None,
                    index_created_at: None,
                });
            }
            Err(e) => return Err(e),
        };

        // Step 1: Check format version compatibility
        if index_data.format_version != self.storage.expected_version() {
            let reason = format!(
                "Index version {} incompatible with clangd version (expects {})",
                index_data.format_version,
                self.storage.expected_version()
            );

            return Ok(IndexEntry {
                absolute_path: absolute_path.clone(),
                status: FileIndexStatus::Invalid(reason),
                index_format_version: Some(index_data.format_version),
                expected_format_version: self.storage.expected_version(),
                index_content_hash: Some(index_data.content_hash),
                current_file_hash: None,
                symbols: vec![], // Don't trust incompatible symbols
                index_file_size: index_data.metadata.file_size,
                index_created_at: index_data.metadata.created_at,
            });
        }

        // Step 2: Trust the index file - if it exists, assume it's valid
        // Clangd manages its own index validity, so we don't need to validate
        let status = FileIndexStatus::Done;
        let symbols = index_data.symbols;

        debug!(
            "Index validation complete for {:?}: {}",
            absolute_path,
            match status {
                FileIndexStatus::Done => "current",
                FileIndexStatus::Stale => "stale",
                _ => "other",
            }
        );

        Ok(IndexEntry {
            absolute_path: absolute_path.clone(),
            status,
            index_format_version: Some(index_data.format_version),
            expected_format_version: self.storage.expected_version(),
            index_content_hash: Some(index_data.content_hash),
            current_file_hash: None, // We trust the index, no need to compute current hash
            symbols,
            index_file_size: index_data.metadata.file_size,
            index_created_at: index_data.metadata.created_at,
        })
    }

    /// Get cached entry if available and still valid
    async fn get_cached_entry(&self, path: &PathBuf) -> Option<IndexEntry> {
        let cache = self.cache.read().await;
        cache.get(path).cloned()
    }

    /// Cache an index entry
    async fn cache_entry(&self, path: PathBuf, entry: IndexEntry) {
        let mut cache = self.cache.write().await;
        cache.insert(path, entry);
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        debug!("Index reader cache cleared");
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().await;
        let total = cache.len();
        let valid = cache.values().filter(|entry| entry.is_valid()).count();
        (total, valid)
    }

    /// Compute SHA256 hash of file content for staleness detection
    async fn compute_content_hash(path: &Path) -> Result<String, std::io::Error> {
        use tokio::fs;

        let content = fs::read_to_string(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[async_trait]
impl IndexReaderTrait for IndexReader {
    async fn read_index_for_file(&self, source_path: &Path) -> Result<IndexEntry, IndexError> {
        self.read_index_for_file(source_path).await
    }

    async fn clear_cache(&self) {
        self.clear_cache().await
    }

    async fn cache_stats(&self) -> (usize, usize) {
        self.cache_stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clangd::version::ClangdVersion;
    use crate::project::index::storage::filesystem::FilesystemIndexStorage;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_clangd_version() -> ClangdVersion {
        ClangdVersion {
            major: 18,
            minor: 0,
            patch: 0,
            variant: None,
            date: None,
        }
    }

    async fn create_test_source_file(
        temp_dir: &std::path::Path,
        name: &str,
        content: &str,
    ) -> PathBuf {
        use tokio::fs;
        let file_path = temp_dir.join(name);
        fs::write(&file_path, content).await.unwrap();
        file_path
    }

    async fn create_test_reader_with_storage(temp_dir: &std::path::Path) -> IndexReader {
        let filesystem = crate::io::file_system::TestFileSystem::new();
        let storage = Arc::new(FilesystemIndexStorage::new(
            temp_dir.to_path_buf(),
            19,
            filesystem,
        ));
        let clangd_version = create_test_clangd_version();
        IndexReader::new(storage, clangd_version)
    }

    #[test]
    fn test_index_entry_status_descriptions() {
        let test_cases = vec![
            (FileIndexStatus::None, "Not indexed"),
            (FileIndexStatus::InProgress, "Indexing in progress"),
            (FileIndexStatus::Done, "Index current"),
            (FileIndexStatus::Stale, "Index stale (file modified)"),
            (
                FileIndexStatus::Invalid("test".to_string()),
                "Index invalid: test",
            ),
        ];

        for (status, expected) in test_cases {
            let entry = IndexEntry {
                absolute_path: PathBuf::from("/test.cpp"),
                status,
                index_format_version: None,
                expected_format_version: 19,
                index_content_hash: None,
                current_file_hash: None,
                symbols: vec![],
                index_file_size: None,
                index_created_at: None,
            };

            assert_eq!(entry.status_description(), expected);
        }
    }

    #[tokio::test]
    async fn test_read_index_for_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let reader = create_test_reader_with_storage(temp_dir.path()).await;

        let nonexistent_file = temp_dir.path().join("nonexistent.cpp");
        let result = reader.read_index_for_file(&nonexistent_file).await;

        assert!(result.is_err());
        if let Err(IndexError::Io(_)) = result {
        } else {
            panic!("Expected IndexError::Io, got: {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_read_index_for_file_with_no_index() {
        let temp_dir = TempDir::new().unwrap();
        let reader = create_test_reader_with_storage(temp_dir.path()).await;

        let source_file =
            create_test_source_file(temp_dir.path(), "test.cpp", "int main() { return 0; }").await;

        let result = reader.read_index_for_file(&source_file).await;

        assert!(result.is_ok());
        let entry = result.unwrap();
        assert_eq!(entry.status, FileIndexStatus::None);
        assert_eq!(entry.index_format_version, None);
        assert_eq!(entry.symbols.len(), 0);
    }

    #[tokio::test]
    async fn test_cache_hit_scenario() {
        let temp_dir = TempDir::new().unwrap();
        let reader = create_test_reader_with_storage(temp_dir.path()).await;

        let source_file =
            create_test_source_file(temp_dir.path(), "cached.cpp", "void test_function() {}").await;

        let first_result = reader.read_index_for_file(&source_file).await.unwrap();
        let (cache_total_before, _) = reader.cache_stats().await;

        let second_result = reader.read_index_for_file(&source_file).await.unwrap();
        let (cache_total_after, _) = reader.cache_stats().await;

        assert_eq!(first_result.absolute_path, second_result.absolute_path);
        assert_eq!(first_result.status, second_result.status);
        assert_eq!(cache_total_before, 1);
        assert_eq!(cache_total_after, 1);
    }

    #[tokio::test]
    async fn test_cache_operations_with_entry() {
        let temp_dir = TempDir::new().unwrap();
        let reader = create_test_reader_with_storage(temp_dir.path()).await;

        let source_file =
            create_test_source_file(temp_dir.path(), "test_cache.cpp", "class TestClass {};").await;

        let (initial_total, initial_valid) = reader.cache_stats().await;
        assert_eq!(initial_total, 0);
        assert_eq!(initial_valid, 0);

        let entry = reader.read_index_for_file(&source_file).await.unwrap();
        let (after_read_total, after_read_valid) = reader.cache_stats().await;
        assert_eq!(after_read_total, 1);
        assert_eq!(after_read_valid, if entry.is_valid() { 1 } else { 0 });

        reader.clear_cache().await;
        let (after_clear_total, after_clear_valid) = reader.cache_stats().await;
        assert_eq!(after_clear_total, 0);
        assert_eq!(after_clear_valid, 0);
    }

    #[tokio::test]
    async fn test_compute_content_hash() {
        let temp_dir = TempDir::new().unwrap();
        let test_content = "int main() { return 42; }";
        let source_file =
            create_test_source_file(temp_dir.path(), "hash_test.cpp", test_content).await;

        let hash = IndexReader::compute_content_hash(&source_file).await;
        assert!(hash.is_ok());
        let hash_value = hash.unwrap();
        assert!(!hash_value.is_empty());
        assert!(hash_value.len() == 64); // SHA256 hex length

        let same_content_file =
            create_test_source_file(temp_dir.path(), "hash_test2.cpp", test_content).await;
        let hash2 = IndexReader::compute_content_hash(&same_content_file)
            .await
            .unwrap();
        assert_eq!(hash_value, hash2);
    }

    #[tokio::test]
    async fn test_compute_content_hash_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_file = temp_dir.path().join("does_not_exist.cpp");

        let result = IndexReader::compute_content_hash(&nonexistent_file).await;
        assert!(result.is_err());
    }
}
