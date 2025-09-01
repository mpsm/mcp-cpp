//! Index storage abstraction and implementations
//!
//! This module provides a storage abstraction for reading clangd index files,
//! enabling different backends (filesystem, network, etc.) while maintaining
//! a consistent interface.

pub mod filesystem;

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during index storage operations
#[derive(Debug, Error)]
pub enum IndexError {
    #[error("Index directory not found: {path}")]
    DirectoryNotFound { path: PathBuf },

    #[error("Index file not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("Permission denied accessing: {path}")]
    PermissionDenied { path: PathBuf },

    #[error("Index file corrupted: {path} - {reason}")]
    CorruptedIndex { path: PathBuf, reason: String },

    #[error("Index format version {found} incompatible with expected {expected}")]
    IncompatibleVersion { found: u32, expected: u32 },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Hash computation failed: {0}")]
    HashComputation(String),

    #[error("Index parsing failed: {0}")]
    ParseError(String),
}

impl IndexError {
    /// Create a corrupted index error
    pub fn corrupted<P: AsRef<Path>>(path: P, reason: impl Into<String>) -> Self {
        Self::CorruptedIndex {
            path: path.as_ref().to_path_buf(),
            reason: reason.into(),
        }
    }

    /// Create an incompatible version error
    pub fn incompatible_version(found: u32, expected: u32) -> Self {
        Self::IncompatibleVersion { found, expected }
    }

    /// Create a parse error
    pub fn parse_error(reason: impl Into<String>) -> Self {
        Self::ParseError(reason.into())
    }
}

/// Raw index data read from storage
#[derive(Debug, Clone)]
pub struct IndexData {
    /// Source file that this index represents
    pub source_file: PathBuf,
    /// Format version of the index file
    pub format_version: u32,
    /// Content hash stored in the index (for staleness detection)
    pub content_hash: String,
    /// Symbols found in the index (simplified representation)
    pub symbols: Vec<String>,
    /// Additional metadata
    pub metadata: IndexMetadata,
}

/// Additional metadata about the index
#[derive(Debug, Clone, Default)]
pub struct IndexMetadata {
    /// Timestamp when index was created
    pub created_at: Option<std::time::SystemTime>,
    /// Size of the index file in bytes
    pub file_size: Option<u64>,
}

/// Trait for index storage backends
#[async_trait]
pub trait IndexStorage: Send + Sync {
    /// Read index data for a specific source file
    ///
    /// This method should:
    /// 1. Locate the index file for the given source path
    /// 2. Parse the index file format
    /// 3. Extract source file mapping and content hash
    /// 4. Return structured IndexData
    async fn read_index(&self, source_path: &Path) -> Result<IndexData, IndexError>;

    /// List all index files in a directory
    async fn list_index_files(&self, index_dir: &Path) -> Result<Vec<PathBuf>, IndexError>;

    /// Check if storage supports a specific index format version
    fn supports_version(&self, version: u32) -> bool;

    /// Get the expected index format version for this storage backend
    fn expected_version(&self) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_error_creation() {
        let error = IndexError::corrupted("/path/to/index", "Invalid magic bytes");
        match error {
            IndexError::CorruptedIndex { path, reason } => {
                assert_eq!(path, PathBuf::from("/path/to/index"));
                assert_eq!(reason, "Invalid magic bytes");
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_incompatible_version_error() {
        let error = IndexError::incompatible_version(17, 19);
        match error {
            IndexError::IncompatibleVersion { found, expected } => {
                assert_eq!(found, 17);
                assert_eq!(expected, 19);
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_index_data_creation() {
        let data = IndexData {
            source_file: PathBuf::from("/source/file.cpp"),
            format_version: 19,
            content_hash: "ABC123".to_string(),
            symbols: vec!["symbol1".to_string(), "symbol2".to_string()],
            metadata: IndexMetadata::default(),
        };

        assert_eq!(data.source_file, PathBuf::from("/source/file.cpp"));
        assert_eq!(data.format_version, 19);
        assert_eq!(data.symbols.len(), 2);
    }
}
