//! File buffer manager for caching and lifecycle management
//!
//! Provides centralized management of file buffers with caching,
//! using the manager-owned filesystem pattern for testability.
#![allow(dead_code)]

use crate::io::file_buffer::{FileBuffer, FileBufferError};
use crate::io::file_system::FileSystemTrait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ============================================================================
// File Buffer Manager
// ============================================================================

/// Manages a collection of file buffers with caching and lifecycle management
///
/// Uses the manager-owned filesystem pattern where the manager owns the
/// filesystem instance and clones it to buffers for dependency injection.
#[derive(Debug)]
pub struct FileBufferManager<F: FileSystemTrait> {
    /// Cache of open file buffers by their absolute path
    buffers: HashMap<PathBuf, FileBuffer<F>>,
    /// Filesystem instance that gets cloned to buffers
    filesystem: F,
}

impl<F: FileSystemTrait + Clone> FileBufferManager<F> {
    /// Create a new file buffer manager with the given filesystem
    pub fn new(filesystem: F) -> Self {
        Self {
            buffers: HashMap::new(),
            filesystem,
        }
    }

    /// Get or create a file buffer for the given path
    ///
    /// Returns a mutable reference to the cached buffer, creating it if necessary.
    /// The filesystem instance is cloned to the buffer for dependency injection.
    pub fn get_buffer(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<&mut FileBuffer<F>, FileBufferError> {
        let path_buf = path.as_ref().to_path_buf();

        if !self.buffers.contains_key(&path_buf) {
            let buffer = FileBuffer::new_with_filesystem(&path_buf, self.filesystem.clone())?;
            self.buffers.insert(path_buf.clone(), buffer);
        }

        Ok(self.buffers.get_mut(&path_buf).unwrap())
    }

    /// Clear all cached buffers
    pub fn clear_cache(&mut self) {
        self.buffers.clear();
    }
}

// ============================================================================
// Convenience Type Aliases
// ============================================================================

use crate::io::file_system::RealFileSystem;

/// File buffer manager using the real filesystem
pub type RealFileBufferManager = FileBufferManager<RealFileSystem>;

impl RealFileBufferManager {
    /// Create a new file buffer manager using the real filesystem
    pub fn new_real() -> Self {
        Self::new(RealFileSystem)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::file_buffer::FilePosition;
    use crate::io::file_system::TestFileSystem;
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::tempdir;

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    #[test]
    fn test_file_buffer_manager_caching() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/cached.txt");
        let content = "Cached content";
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, content, time);

        let mut manager = FileBufferManager::new(filesystem);

        // First access creates and caches the buffer
        {
            let buffer1 = manager.get_buffer(&test_path).unwrap();

            let start = FilePosition::new(0, 0);
            let end = FilePosition::new(0, 6);
            let result = buffer1.text_between(start, end).unwrap();
            assert_eq!(result, "Cached");
        }

        // Subsequent access returns cached instance
        {
            let buffer2 = manager.get_buffer(&test_path).unwrap();
            let start = FilePosition::new(0, 0);
            let end = FilePosition::new(0, 6);
            let result = buffer2.text_between(start, end).unwrap();
            assert_eq!(result, "Cached");
        }
    }

    #[test]
    fn test_file_buffer_manager_cache_operations() {
        let filesystem = TestFileSystem::new();
        let mut manager = FileBufferManager::new(filesystem.clone());

        let path1 = PathBuf::from("/test/file1.txt");
        let path2 = PathBuf::from("/test/file2.txt");
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&path1, "Content 1", time);
        filesystem.set_file_content(&path2, "Content 2", time);

        // Access multiple buffers
        {
            let buffer1 = manager.get_buffer(&path1).unwrap();
            let start = FilePosition::new(0, 0);
            let end = FilePosition::new(0, 7);
            let result = buffer1.text_between(start, end).unwrap();
            assert_eq!(result, "Content");
        }

        {
            let buffer2 = manager.get_buffer(&path2).unwrap();
            let start = FilePosition::new(0, 0);
            let end = FilePosition::new(0, 7);
            let result = buffer2.text_between(start, end).unwrap();
            assert_eq!(result, "Content");
        }

        // Clear all buffers
        manager.clear_cache();

        // Post-clear access reloads from filesystem
        {
            let buffer1 = manager.get_buffer(&path1).unwrap();
            let start = FilePosition::new(0, 0);
            let end = FilePosition::new(0, 7);
            let result = buffer1.text_between(start, end).unwrap();
            assert_eq!(result, "Content");
        }
    }

    #[test]
    fn test_file_buffer_manager_with_real_filesystem() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Real filesystem content\nSecond line";
        std::fs::write(&file_path, content).unwrap();

        let mut manager = RealFileBufferManager::new_real();

        // Access real filesystem through manager
        {
            let buffer = manager.get_buffer(&file_path).unwrap();

            // Extract text
            let start = FilePosition::new(0, 0);
            let end = FilePosition::new(0, 4);
            let result = buffer.text_between(start, end).unwrap();
            assert_eq!(result, "Real");

            // Extract from second line
            let start = FilePosition::new(1, 0);
            let end = FilePosition::new(1, 6);
            let result = buffer.text_between(start, end).unwrap();
            assert_eq!(result, "Second");
        }
    }

    #[test]
    fn test_filemanager_concurrent_access_simulation() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/concurrent.txt");
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        // Simulate what would happen in concurrent access
        filesystem.set_file_content(&test_path, "Initial content", time);

        let mut manager1 = FileBufferManager::new(filesystem.clone());
        let mut manager2 = FileBufferManager::new(filesystem.clone());

        // Both managers access the same file
        let buffer1 = manager1.get_buffer(&test_path).unwrap();
        let buffer2 = manager2.get_buffer(&test_path).unwrap();

        // They should have separate buffer instances
        assert!(!std::ptr::eq(buffer1, buffer2));

        // But both should see the same content
        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 7);

        let result1 = buffer1.text_between(start, end).unwrap();
        let result2 = buffer2.text_between(start, end).unwrap();

        assert_eq!(result1, "Initial");
        assert_eq!(result2, "Initial");

        // Shared filesystem updates affect all manager instances
        let updated_time = time + Duration::from_secs(1000);
        filesystem.update_file_content(&test_path, "Updated content", updated_time);

        // Both buffers detect filesystem changes on access
        let result1 = buffer1.text_between(start, end).unwrap();
        let result2 = buffer2.text_between(start, end).unwrap();

        assert_eq!(result1, "Updated");
        assert_eq!(result2, "Updated");
    }
}
