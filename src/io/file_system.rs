//! File system abstraction layer
//!
//! Provides trait-based abstractions for file system operations, enabling
//! dependency injection and comprehensive testing through mock implementations.
#![allow(dead_code)]

use std::path::Path;
use std::time::SystemTime;

// ============================================================================
// File Metadata
// ============================================================================

/// Custom file metadata abstraction
///
/// Provides a simplified, testable alternative to std::fs::Metadata
/// with controllable modification times and file sizes.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMetadata {
    /// Last modification time
    pub modified: SystemTime,
    /// File size in bytes
    pub size: u64,
}

impl FileMetadata {
    /// Create new file metadata
    pub fn new(modified: SystemTime, size: u64) -> Self {
        Self { modified, size }
    }

    /// Convert from standard library metadata
    pub fn from_std_metadata(metadata: &std::fs::Metadata) -> Result<Self, std::io::Error> {
        Ok(Self {
            modified: metadata.modified()?,
            size: metadata.len(),
        })
    }
}

// ============================================================================
// File System Trait
// ============================================================================

/// Trait for file system operations
///
/// Enables dependency injection and testing through mock implementations.
/// All operations return custom types for enhanced testability.
#[cfg_attr(test, mockall::automock)]
pub trait FileSystemTrait: Clone + Send + Sync {
    /// Check if a file exists
    fn exists(&self, path: &Path) -> bool;

    /// Read file contents as bytes
    fn read(&self, path: &Path) -> Result<Vec<u8>, std::io::Error>;

    /// Get file metadata (modification time, size, etc.)
    fn metadata(&self, path: &Path) -> Result<FileMetadata, std::io::Error>;
}

// ============================================================================
// Real File System Implementation
// ============================================================================

/// Real file system implementation using std::fs
#[derive(Debug, Clone)]
pub struct RealFileSystem;

impl FileSystemTrait for RealFileSystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, std::io::Error> {
        std::fs::read(path)
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata, std::io::Error> {
        let metadata = std::fs::metadata(path)?;
        FileMetadata::from_std_metadata(&metadata)
    }
}

// ============================================================================
// Test File System Implementation
// ============================================================================

#[cfg(test)]
mod test_filesystem {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// In-memory filesystem state for testing scenarios
    type TestFileData = HashMap<PathBuf, (Vec<u8>, SystemTime)>;

    #[derive(Clone)]
    pub struct TestFileSystem {
        state: Arc<Mutex<TestFileData>>,
    }

    impl TestFileSystem {
        pub fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        pub fn set_file_content<P: Into<PathBuf>>(
            &self,
            path: P,
            content: &str,
            modified: SystemTime,
        ) {
            let mut state = self.state.lock().unwrap();
            state.insert(path.into(), (content.as_bytes().to_vec(), modified));
        }

        pub fn update_file_content<P: AsRef<Path>>(
            &self,
            path: P,
            content: &str,
            modified: SystemTime,
        ) {
            let mut state = self.state.lock().unwrap();
            let path_buf = path.as_ref().to_path_buf();
            state.insert(path_buf, (content.as_bytes().to_vec(), modified));
        }
    }

    impl FileSystemTrait for TestFileSystem {
        fn exists(&self, path: &Path) -> bool {
            let state = self.state.lock().unwrap();
            state.contains_key(path)
        }

        fn read(&self, path: &Path) -> Result<Vec<u8>, std::io::Error> {
            let state = self.state.lock().unwrap();
            state
                .get(path)
                .map(|(content, _)| content.clone())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
        }

        fn metadata(&self, path: &Path) -> Result<FileMetadata, std::io::Error> {
            let state = self.state.lock().unwrap();
            state
                .get(path)
                .map(|(content, modified)| FileMetadata {
                    modified: *modified,
                    size: content.len() as u64,
                })
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
        }
    }
}

#[cfg(test)]
pub use test_filesystem::TestFileSystem;

// MockFileSystemTrait Clone implementation for dependency injection patterns
#[cfg(test)]
impl Clone for MockFileSystemTrait {
    fn clone(&self) -> Self {
        MockFileSystemTrait::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn test_file_metadata_creation() {
        let time = UNIX_EPOCH + Duration::from_secs(1000);
        let metadata = FileMetadata::new(time, 42);

        assert_eq!(metadata.modified, time);
        assert_eq!(metadata.size, 42);
    }

    #[test]
    fn test_test_filesystem_basic_operations() {
        let fs = TestFileSystem::new();
        let path = PathBuf::from("/test/file.txt");
        let content = "Hello, world!";
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        // File does not exist initially
        assert!(!fs.exists(&path));

        // Set file content
        fs.set_file_content(&path, content, time);
        assert!(fs.exists(&path));

        // Read content
        let read_content = fs.read(&path).unwrap();
        assert_eq!(read_content, content.as_bytes());

        // Get metadata
        let metadata = fs.metadata(&path).unwrap();
        assert_eq!(metadata.modified, time);
        assert_eq!(metadata.size, content.len() as u64);
    }

    #[test]
    fn test_test_filesystem_update_content() {
        let fs = TestFileSystem::new();
        let path = PathBuf::from("/test/file.txt");
        let time1 = UNIX_EPOCH + Duration::from_secs(1000);
        let time2 = UNIX_EPOCH + Duration::from_secs(2000);

        // Set initial content
        fs.set_file_content(&path, "Initial", time1);
        let metadata1 = fs.metadata(&path).unwrap();
        assert_eq!(metadata1.modified, time1);
        assert_eq!(metadata1.size, 7);

        // Update content
        fs.update_file_content(&path, "Updated content", time2);
        let metadata2 = fs.metadata(&path).unwrap();
        assert_eq!(metadata2.modified, time2);
        assert_eq!(metadata2.size, 15);

        let content = fs.read(&path).unwrap();
        assert_eq!(content, b"Updated content");
    }

    #[test]
    fn test_test_filesystem_cloning() {
        let fs1 = TestFileSystem::new();
        let path = PathBuf::from("/test/clone.txt");
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        // Set content in first instance
        fs1.set_file_content(&path, "Shared content", time);

        // Clone filesystem
        let fs2 = fs1.clone();

        // Both instances should see the same data
        assert!(fs1.exists(&path));
        assert!(fs2.exists(&path));

        let content1 = fs1.read(&path).unwrap();
        let content2 = fs2.read(&path).unwrap();
        assert_eq!(content1, content2);

        // Updates through one clone are visible to the other
        fs2.update_file_content(&path, "Modified", time);
        let updated_content = fs1.read(&path).unwrap();
        assert_eq!(updated_content, b"Modified");
    }

    #[test]
    fn test_real_filesystem_trait_implementation() {
        // This test just ensures RealFileSystem implements the trait correctly
        let fs = RealFileSystem;
        let _cloned = fs.clone();

        // Test with a non-existent path
        let non_existent = PathBuf::from("/definitely/does/not/exist");
        assert!(!fs.exists(&non_existent));

        let result = fs.read(&non_existent);
        assert!(result.is_err());

        let metadata_result = fs.metadata(&non_existent);
        assert!(metadata_result.is_err());
    }
}
