//! Filesystem implementation of index storage
//!
//! This module provides a concrete implementation of the IndexStorage trait
//! that reads clangd index files from the local filesystem using dependency injection.

use super::{IndexData, IndexError, IndexMetadata, IndexStorage};
use crate::clangd::index::hash::compute_file_hash;
use crate::clangd::index::idx_parser::{IdxParseError, IdxParser};
use crate::io::file_system::FileSystemTrait;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

/// Filesystem-based index storage implementation with dependency injection
pub struct FilesystemIndexStorage<F: FileSystemTrait> {
    /// Root directory containing index files
    index_directory: PathBuf,
    /// Expected index format version
    expected_version: u32,
    /// Filesystem implementation for dependency injection
    filesystem: F,
}

impl<F: FileSystemTrait + 'static> FilesystemIndexStorage<F> {
    /// Create a new filesystem index storage with dependency injection
    ///
    /// # Arguments
    /// * `index_directory` - Directory containing clangd index files
    /// * `expected_version` - Expected index format version
    /// * `filesystem` - Filesystem implementation for testability
    pub fn new(index_directory: PathBuf, expected_version: u32, filesystem: F) -> Self {
        Self {
            index_directory,
            expected_version,
            filesystem,
        }
    }

    /// Get the path to an index file for a given source file
    fn get_index_file_path(&self, source_path: &Path) -> PathBuf {
        let path_str = source_path.to_string_lossy();
        let hash = compute_file_hash(&path_str, self.expected_version);
        let index_filename = format!("{:016X}.idx", hash);
        self.index_directory.join(index_filename)
    }

    /// Parse an index file and extract metadata
    async fn parse_index_file(&self, index_path: &Path) -> Result<IndexData, IndexError> {
        trace!("Parsing index file: {:?}", index_path);

        // Check if file exists using filesystem trait
        let path = index_path.to_path_buf();
        let filesystem = self.filesystem.clone();

        let exists = tokio::task::spawn_blocking(move || filesystem.exists(&path))
            .await
            .map_err(|e| IndexError::Io(std::io::Error::other(e)))?;

        if !exists {
            return Err(IndexError::FileNotFound {
                path: index_path.to_path_buf(),
            });
        }

        // Read file metadata using filesystem trait
        let path = index_path.to_path_buf();
        let filesystem = self.filesystem.clone();

        let file_metadata = tokio::task::spawn_blocking(move || filesystem.metadata(&path))
            .await
            .map_err(|e| IndexError::Io(std::io::Error::other(e)))?
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::PermissionDenied {
                    IndexError::PermissionDenied {
                        path: index_path.to_path_buf(),
                    }
                } else {
                    IndexError::Io(err)
                }
            })?;

        // Read and parse the index file using the IDX parser
        let file_size = file_metadata.size;
        let created_at = Some(file_metadata.modified);

        // Read file content using filesystem trait
        let path = index_path.to_path_buf();
        let filesystem = self.filesystem.clone();

        let file_data = tokio::task::spawn_blocking(move || filesystem.read(&path))
            .await
            .map_err(|e| IndexError::Io(std::io::Error::other(e)))??;

        // Parse the index file using the IDX parser
        let parsed_data = IdxParser::parse(&file_data).map_err(|e| match e {
            IdxParseError::UnsupportedVersion(v) => {
                IndexError::incompatible_version(v, self.expected_version)
            }
            _ => IndexError::parse_error(e.to_string()),
        })?;

        // Extract source file information from the include graph
        // Look for translation units first, then fall back to any file if no TUs found
        let translation_units = parsed_data.translation_units();
        let source_file = if !translation_units.is_empty() {
            // Use the first translation unit as the primary source file
            PathBuf::from(&translation_units[0].uri)
        } else if !parsed_data.include_graph.is_empty() {
            // Fall back to the first file in the include graph
            PathBuf::from(&parsed_data.include_graph[0].uri)
        } else {
            // No files found in include graph, derive from filename
            self.derive_source_path_from_index(index_path)?
        };

        // Extract content hash from the primary source file
        let content_hash =
            if let Some(node) = parsed_data.find_node_by_uri(&source_file.to_string_lossy()) {
                hex::encode(node.digest)
            } else if !translation_units.is_empty() {
                hex::encode(translation_units[0].digest)
            } else {
                "UNKNOWN_HASH".to_string()
            };

        let index_data = IndexData {
            source_file,
            format_version: parsed_data.format_version,
            content_hash,
            symbols: vec![], // Could be extracted from symb chunk in the future
            metadata: IndexMetadata {
                created_at,
                file_size: Some(file_size),
            },
        };

        debug!(
            "Parsed index file: {} bytes, format version {}, {} include graph nodes, {} TUs",
            file_size,
            index_data.format_version,
            parsed_data.include_graph.len(),
            translation_units.len()
        );

        Ok(index_data)
    }

    /// Derive source file path from index file path
    /// This is a temporary implementation - real implementation would read from index
    fn derive_source_path_from_index(&self, index_path: &Path) -> Result<PathBuf, IndexError> {
        // This is a simplified reverse mapping
        // In reality, we'd read the source file mapping from the index file itself
        let filename = index_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| IndexError::corrupted(index_path, "Invalid index filename format"))?;

        // For testing purposes, assume filename maps to a source file
        // Real implementation would maintain proper source-to-index mapping
        Ok(PathBuf::from(format!("{}.cpp", filename)))
    }
}

#[async_trait]
impl<F: FileSystemTrait + 'static> IndexStorage for FilesystemIndexStorage<F> {
    async fn read_index(&self, source_path: &Path) -> Result<IndexData, IndexError> {
        // Find the actual index file by pattern matching instead of hash computation
        let source_filename = source_path
            .file_name()
            .ok_or_else(|| IndexError::FileNotFound {
                path: source_path.to_path_buf(),
            })?
            .to_string_lossy();

        // Look for files matching the pattern: SourceFile.HASH.idx
        let pattern_prefix = format!("{}.", source_filename);

        let index_files = self
            .list_index_files(&self.index_directory)
            .await
            .unwrap_or_default();

        for index_file in index_files {
            if let Some(filename) = index_file.file_name() {
                let filename_str = filename.to_string_lossy();
                if filename_str.starts_with(&pattern_prefix) && filename_str.ends_with(".idx") {
                    trace!(
                        "Found index file for source: {:?} -> {:?}",
                        source_path, index_file
                    );
                    return self.parse_index_file(&index_file).await;
                }
            }
        }

        // No index file found for this source file
        Err(IndexError::FileNotFound {
            path: source_path.to_path_buf(),
        })
    }

    async fn list_index_files(&self, index_dir: &Path) -> Result<Vec<PathBuf>, IndexError> {
        debug!("Listing index files in: {:?}", index_dir);

        // Check if directory exists using filesystem trait
        let path = index_dir.to_path_buf();
        let filesystem = self.filesystem.clone();

        let exists = tokio::task::spawn_blocking(move || filesystem.exists(&path))
            .await
            .map_err(|e| IndexError::Io(std::io::Error::other(e)))?;

        if !exists {
            return Err(IndexError::DirectoryNotFound {
                path: index_dir.to_path_buf(),
            });
        }

        // Read directory entries using filesystem trait
        let path = index_dir.to_path_buf();
        let filesystem = self.filesystem.clone();

        let entries = tokio::task::spawn_blocking(move || filesystem.read_dir(&path))
            .await
            .map_err(|e| IndexError::Io(std::io::Error::other(e)))??;

        let mut index_files = Vec::new();
        for entry_path in entries {
            // Filter for index files (typically have .idx extension in clangd)
            if let Some(extension) = entry_path.extension()
                && extension == "idx"
            {
                index_files.push(entry_path);
            }
        }

        debug!("Found {} index files", index_files.len());
        Ok(index_files)
    }

    fn supports_version(&self, version: u32) -> bool {
        // Support current version and one version back for compatibility
        version == self.expected_version || version == self.expected_version.saturating_sub(1)
    }

    fn expected_version(&self) -> u32 {
        self.expected_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::file_system::TestFileSystem;
    use tempfile::TempDir;

    #[test]
    fn test_filesystem_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let filesystem = TestFileSystem::new();
        let storage = FilesystemIndexStorage::new(temp_dir.path().to_path_buf(), 19, filesystem);

        assert_eq!(storage.expected_version(), 19);
        assert!(storage.supports_version(19));
        assert!(storage.supports_version(18)); // One back
        assert!(!storage.supports_version(17)); // Too old
        assert!(!storage.supports_version(20)); // Too new
    }

    #[test]
    fn test_index_file_path_generation() {
        let temp_dir = TempDir::new().unwrap();
        let filesystem = TestFileSystem::new();
        let storage = FilesystemIndexStorage::new(temp_dir.path().to_path_buf(), 19, filesystem);

        let source_path = Path::new("/project/src/main.cpp");
        let index_path = storage.get_index_file_path(source_path);

        // Should generate consistent hash-based filename
        assert!(index_path.starts_with(temp_dir.path()));
        assert!(index_path.to_string_lossy().contains(".idx"));
    }

    #[tokio::test]
    async fn test_read_nonexistent_index() {
        let temp_dir = TempDir::new().unwrap();
        let filesystem = TestFileSystem::new();
        let storage = FilesystemIndexStorage::new(temp_dir.path().to_path_buf(), 19, filesystem);

        let source_path = Path::new("/project/src/main.cpp");
        let result = storage.read_index(source_path).await;

        assert!(matches!(result, Err(IndexError::FileNotFound { .. })));
    }

    #[tokio::test]
    async fn test_list_index_files_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let filesystem = TestFileSystem::new();
        // Add directory to test filesystem (directories are tracked via file paths)
        filesystem.set_file_content(
            temp_dir.path().join(".keep"),
            "",
            std::time::SystemTime::now(),
        );
        let storage = FilesystemIndexStorage::new(temp_dir.path().to_path_buf(), 19, filesystem);

        let files = storage.list_index_files(temp_dir.path()).await.unwrap();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_list_index_files_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");
        let filesystem = TestFileSystem::new();
        let storage = FilesystemIndexStorage::new(temp_dir.path().to_path_buf(), 19, filesystem);

        let result = storage.list_index_files(&nonexistent).await;
        assert!(matches!(result, Err(IndexError::DirectoryNotFound { .. })));
    }
}
