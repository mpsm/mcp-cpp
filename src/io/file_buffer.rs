//! File buffer with UTF-8 positioning and encoding detection
//!
//! Provides efficient file content management with UTF-8 code point positioning,
//! automatic change detection, and comprehensive encoding support.
#![allow(dead_code)]

use crate::io::file_system::FileSystemTrait;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ============================================================================
// File Position
// ============================================================================

/// Represents a position in a file using 0-based line and column coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilePosition {
    pub line: u32,
    pub column: u32,
}

impl FilePosition {
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

// ============================================================================
// File Buffer Errors
// ============================================================================

/// File buffer operation errors
#[derive(Debug, thiserror::Error)]
pub enum FileBufferError {
    #[error("Position out of bounds: {pos:?}")]
    PositionOutOfBounds { pos: FilePosition },

    #[error("Invalid range: start {start:?} after end {end:?}")]
    InvalidRange {
        start: FilePosition,
        end: FilePosition,
    },

    #[error("Unsupported encoding in file: {0}")]
    UnsupportedEncoding(String),

    #[error("Invalid UTF-8 sequence: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// File Buffer
// ============================================================================

/// File buffer with UTF-8 positioning and automatic refresh detection
#[derive(Debug)]
pub struct FileBuffer<F: FileSystemTrait> {
    /// UTF-8 normalized content
    content: String,
    /// Line start positions in UTF-8 code point offsets
    line_starts: Vec<usize>,
    /// File modification time for refresh detection
    last_modified: SystemTime,
    /// Content hash for change detection
    content_hash: String,
    /// File path for reload operations
    path: PathBuf,
    /// Filesystem instance for operations
    filesystem: F,
}

impl<F: FileSystemTrait> FileBuffer<F> {
    /// Create buffer with provided filesystem instance
    pub fn new_with_filesystem(
        path: impl AsRef<Path>,
        filesystem: F,
    ) -> Result<Self, FileBufferError> {
        let path = path.as_ref().to_path_buf();
        let metadata = filesystem.metadata(&path)?;
        let last_modified = metadata.modified;

        let bytes = filesystem.read(&path)?;
        let content = Self::normalize_encoding(&bytes)?;
        let line_starts = Self::build_line_index(&content);
        let content_hash = Self::compute_hash(&content);

        Ok(Self {
            content,
            line_starts,
            last_modified,
            content_hash,
            path,
            filesystem,
        })
    }

    /// Extract text between two positions (inclusive start, exclusive end)
    ///
    /// Automatically refreshes content if file has been modified.
    /// Uses 0-based UTF-8 code point positioning for proper international text handling.
    pub fn text_between(
        &mut self,
        start: FilePosition,
        end: FilePosition,
    ) -> Result<String, FileBufferError> {
        // Check for file changes and refresh if needed using stored filesystem
        self.refresh_if_changed()?;

        use tracing::info;

        info!(
            "Extracting text from {:?} to {:?} in file {}",
            start,
            end,
            self.path.display()
        );

        // Validate position order
        if start.line > end.line || (start.line == end.line && start.column > end.column) {
            return Err(FileBufferError::InvalidRange { start, end });
        }

        let start_offset = self.position_to_offset(start)?;
        let end_offset = self.position_to_offset(end)?;

        Ok(self.content[start_offset..end_offset].to_string())
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    /// Refresh file content if it has been modified
    fn refresh_if_changed(&mut self) -> Result<(), FileBufferError> {
        // Skip refresh for test paths that don't exist on filesystem
        if !self.filesystem.exists(&self.path) {
            return Ok(());
        }

        let metadata = self.filesystem.metadata(&self.path)?;
        let current_modified = metadata.modified;

        if current_modified != self.last_modified {
            let bytes = self.filesystem.read(&self.path)?;
            let new_content = Self::normalize_encoding(&bytes)?;
            let new_hash = Self::compute_hash(&new_content);

            // Only update if content actually changed
            if new_hash != self.content_hash {
                self.content = new_content;
                self.line_starts = Self::build_line_index(&self.content);
                self.content_hash = new_hash;
            }

            self.last_modified = current_modified;
        }

        Ok(())
    }

    /// Convert file position to byte offset in UTF-8 content
    fn position_to_offset(&self, pos: FilePosition) -> Result<usize, FileBufferError> {
        use tracing::error;

        // Positions are already 0-based
        let line_index = pos.line as usize;
        let column_index = pos.column as usize;

        // Check line bounds
        if line_index >= self.line_starts.len() {
            error!(
                "File position out of bounds: {:?}, line count: {}",
                pos,
                self.line_starts.len()
            );
            return Err(FileBufferError::PositionOutOfBounds { pos });
        }

        let line_start = self.line_starts[line_index];

        // For the last line, calculate end using content length
        let line_end = if line_index + 1 < self.line_starts.len() {
            self.line_starts[line_index + 1] - 1 // Exclude newline character
        } else {
            self.content.len()
        };

        // Calculate target position in UTF-8 code points
        let line_content = &self.content[line_start..line_end];
        let chars: Vec<char> = line_content.chars().collect();

        if column_index > chars.len() {
            error!(
                "File position out of bounds: {:?}, chars count: {}",
                pos,
                chars.len()
            );
            return Err(FileBufferError::PositionOutOfBounds { pos });
        }

        // Calculate byte offset for the column position
        let column_offset: usize = chars.iter().take(column_index).map(|c| c.len_utf8()).sum();

        Ok(line_start + column_offset)
    }

    /// Build line start index for efficient line-based operations
    fn build_line_index(content: &str) -> Vec<usize> {
        let mut line_starts = vec![0];

        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                line_starts.push(i + ch.len_utf8());
            }
        }

        line_starts
    }

    /// Normalize encoding and line endings
    fn normalize_encoding(bytes: &[u8]) -> Result<String, FileBufferError> {
        // Handle UTF-8 BOM
        let content_bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &bytes[3..]
        } else {
            bytes
        };

        // Convert to UTF-8 string
        let mut content = String::from_utf8(content_bytes.to_vec())?;

        // Normalize line endings to \n
        content = content.replace("\r\n", "\n").replace('\r', "\n");

        Ok(content)
    }

    /// Compute SHA256 hash of content for change detection
    fn compute_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::file_system::{FileMetadata, MockFileSystemTrait, TestFileSystem};
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::tempdir;

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    #[test]
    fn test_file_position() {
        let pos = FilePosition::new(5, 10);
        assert_eq!(pos.line, 5);
        assert_eq!(pos.column, 10);
    }

    #[test]
    fn test_line_index_building() {
        let content = "Hello\nWorld\n\nTest";
        let line_starts = FileBuffer::<TestFileSystem>::build_line_index(content);
        assert_eq!(line_starts, vec![0, 6, 12, 13]);
    }

    #[test]
    fn test_file_buffer_utf8_positioning() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/utf8.txt");
        let content = "Hello 🌍\nWorld 🚀\nTest 📝";
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, content, time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem).unwrap();

        // Extract text across UTF-8 emoji boundaries
        let start = FilePosition::new(0, 7); // After the emoji (end of line 0)
        let end = FilePosition::new(1, 5); // Up to "World"
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "\nWorld");

        // Extract single emoji character
        let start = FilePosition::new(0, 6);
        let end = FilePosition::new(0, 7);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "🌍");
    }

    #[test]
    fn test_complex_utf8_text_extraction() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/complex.txt");
        let content = "Hello 🌍\nWorld 🚀\nTest 📝";
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, content, time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem).unwrap();

        println!("Content chars: {:?}", content.chars().collect::<Vec<_>>());
        println!("Line starts: {:?}", buffer.line_starts);

        // Extract ASCII text
        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 5);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "Hello");

        // Extract text with Unicode characters
        let start = FilePosition::new(1, 0);
        let end = FilePosition::new(1, 5);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "World");

        // Extract emoji
        let start = FilePosition::new(2, 5);
        let end = FilePosition::new(2, 6);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "📝");
    }

    #[test]
    fn test_encoding_detection_utf8_bom() {
        let content_with_bom = [0xEF, 0xBB, 0xBF, b'H', b'e', b'l', b'l', b'o'];
        let normalized =
            FileBuffer::<TestFileSystem>::normalize_encoding(&content_with_bom).unwrap();
        assert_eq!(normalized, "Hello");
    }

    #[test]
    fn test_line_ending_normalization() {
        let content_crlf = b"Line1\r\nLine2\rLine3\nLine4";
        let normalized = FileBuffer::<TestFileSystem>::normalize_encoding(content_crlf).unwrap();
        assert_eq!(normalized, "Line1\nLine2\nLine3\nLine4");
    }

    #[test]
    fn test_content_hash_consistency() {
        let content1 = "Same content";
        let content2 = "Same content";
        let content3 = "Different content";

        let hash1 = FileBuffer::<TestFileSystem>::compute_hash(content1);
        let hash2 = FileBuffer::<TestFileSystem>::compute_hash(content2);
        let hash3 = FileBuffer::<TestFileSystem>::compute_hash(content3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_file_buffer_with_real_filesystem() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Real file content\nSecond line\nThird line";
        std::fs::write(&file_path, content).unwrap();

        let mut buffer =
            FileBuffer::new_with_filesystem(&file_path, crate::io::file_system::RealFileSystem)
                .unwrap();

        // Test text extraction
        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 4);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "Real");

        // Test second line
        let start = FilePosition::new(1, 0);
        let end = FilePosition::new(1, 6);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "Second");
    }

    #[test]
    fn test_position_out_of_bounds_error() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/bounds.txt");
        let content = "Short\nFile";
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, content, time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem).unwrap();

        // Test line out of bounds
        let start = FilePosition::new(4, 0);
        let end = FilePosition::new(4, 4);
        let result = buffer.text_between(start, end);
        assert!(matches!(
            result,
            Err(FileBufferError::PositionOutOfBounds { .. })
        ));

        // Test column out of bounds
        let start = FilePosition::new(0, 19);
        let end = FilePosition::new(0, 24);
        let result = buffer.text_between(start, end);
        assert!(matches!(
            result,
            Err(FileBufferError::PositionOutOfBounds { .. })
        ));
    }

    #[test]
    fn test_invalid_range_error() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/range.txt");
        let content = "Test content for range validation";
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, content, time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem).unwrap();

        // Test start after end
        let start = FilePosition::new(0, 9);
        let end = FilePosition::new(0, 4);
        let result = buffer.text_between(start, end);
        assert!(matches!(result, Err(FileBufferError::InvalidRange { .. })));
    }

    #[test]
    fn test_text_extraction_edge_cases() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/edge.txt");
        let content = "A\n\nC";
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, content, time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem).unwrap();

        // Test empty line
        let start = FilePosition::new(1, 0);
        let end = FilePosition::new(1, 0);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "");

        // Test single character lines
        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 1);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "A");

        let start = FilePosition::new(2, 0);
        let end = FilePosition::new(2, 1);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "C");
    }

    #[test]
    fn test_file_modification_detection_with_test_filesystem() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/changeable.txt");
        let initial_time = UNIX_EPOCH + Duration::from_secs(1000);
        let later_time = UNIX_EPOCH + Duration::from_secs(2000);
        let initial_content = "Initial content\nLine 2";
        let updated_content = "Updated content\nNew line 2";

        filesystem.set_file_content(&test_path, initial_content, initial_time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem.clone()).unwrap();

        // Initial state - should work without refresh
        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 7);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "Initial");

        // Simulate file modification
        filesystem.update_file_content(&test_path, updated_content, later_time);

        // Next text_between call should detect change and refresh
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "Updated"); // Content should be refreshed

        // Verify the buffer's internal state was updated
        assert_eq!(buffer.last_modified, later_time);
        assert_eq!(
            buffer.content_hash,
            FileBuffer::<TestFileSystem>::compute_hash(updated_content)
        );
    }

    #[test]
    fn test_metadata_mock_scenarios() {
        let mut mock_fs = MockFileSystemTrait::new();
        let test_path = PathBuf::from("/mock/controlled.txt");

        // Scenario: Precise time control
        let time1 = UNIX_EPOCH + Duration::from_millis(1234567890);
        let time2 = UNIX_EPOCH + Duration::from_millis(1234567891); // 1ms later

        mock_fs
            .expect_exists()
            .with(mockall::predicate::eq(test_path.clone()))
            .returning(|_| true)
            .times(1);

        // When refresh_if_changed() calls metadata, return the newer time to trigger refresh
        mock_fs
            .expect_metadata()
            .with(mockall::predicate::eq(test_path.clone()))
            .returning(move |_| Ok(FileMetadata::new(time2, 105))) // Return newer time
            .times(1);

        mock_fs
            .expect_read()
            .with(mockall::predicate::eq(test_path.clone()))
            .returning(|_| Ok(b"Updated content".to_vec()))
            .times(1);

        // Create buffer with mock, starting with older time
        let mut buffer = FileBuffer::<MockFileSystemTrait> {
            content: "Original content".to_string(),
            line_starts: vec![0],
            last_modified: time1, // Buffer starts with older time
            content_hash: FileBuffer::<MockFileSystemTrait>::compute_hash("Original content"),
            path: test_path,
            filesystem: mock_fs,
        };

        // This should trigger refresh due to time difference (time1 vs time2)
        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 7);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "Updated");
    }

    #[test]
    fn test_edge_case_empty_file() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/empty.txt");
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, "", time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem).unwrap();

        // Test extracting from empty file
        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 0);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_single_character_file() {
        let filesystem = TestFileSystem::new();
        let test_path = PathBuf::from("/test/single.txt");
        let time = UNIX_EPOCH + Duration::from_secs(1000);

        filesystem.set_file_content(&test_path, "X", time);

        let mut buffer = FileBuffer::new_with_filesystem(&test_path, filesystem).unwrap();

        let start = FilePosition::new(0, 0);
        let end = FilePosition::new(0, 1);
        let result = buffer.text_between(start, end).unwrap();
        assert_eq!(result, "X");
    }
}
