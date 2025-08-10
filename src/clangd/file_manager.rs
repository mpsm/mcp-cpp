//! File management for clangd sessions
//!
//! Tracks open files, detects changes, and manages file lifecycle through LSP notifications.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::lsp_v2::traits::LspClientTrait;

// ============================================================================
// File Manager Errors
// ============================================================================

/// File manager errors
#[derive(Debug, thiserror::Error)]
pub enum FileManagerError {
    #[error("Failed to read file: {0}")]
    FileReadError(#[from] std::io::Error),

    #[error("LSP error: {0}")]
    LspError(#[from] crate::lsp_v2::client::LspError),

    #[error("Invalid file path: {0}")]
    InvalidPath(String),
}

// ============================================================================
// File Entry
// ============================================================================

/// Represents an open file in the LSP server
#[derive(Debug, Clone)]
struct FileEntry {
    /// File URI for LSP protocol
    uri: String,

    /// SHA256 hash of the file content when opened
    content_hash: String,

    /// LSP document version number
    version: i32,
}

// ============================================================================
// Clangd File Manager
// ============================================================================

/// Manages open files in a clangd session
pub struct ClangdFileManager {
    /// Map of open files by their absolute path
    opened_files: HashMap<PathBuf, FileEntry>,

    /// Counter for document versions
    next_version: i32,
}

impl ClangdFileManager {
    /// Create a new file manager
    pub fn new() -> Self {
        Self {
            opened_files: HashMap::new(),
            next_version: 1,
        }
    }

    /// Ensure a file is ready for use in the LSP server
    ///
    /// This is the main method users should call. It will:
    /// - Open the file if not already open
    /// - Send a change notification if the file content has changed
    /// - Do nothing if the file is already open and unchanged
    pub async fn ensure_file_ready(
        &mut self,
        path: &Path,
        client: &mut impl LspClientTrait,
    ) -> Result<(), FileManagerError> {
        // Check if client is ready for operations
        if !client.is_initialized() {
            return Err(FileManagerError::LspError(
                crate::lsp_v2::client::LspError::NotInitialized,
            ));
        }

        // Convert to absolute path for consistency
        let abs_path = path
            .canonicalize()
            .map_err(|e| FileManagerError::InvalidPath(format!("{}: {}", path.display(), e)))?;

        // Read current file content
        let content = std::fs::read_to_string(&abs_path)?;
        let content_hash = Self::compute_hash(&content);

        // Generate file URI
        let uri = format!("file://{}", abs_path.display());

        // Check if file is already open
        if let Some(entry) = self.opened_files.get(&abs_path) {
            if entry.content_hash == content_hash {
                // File is open and unchanged
                debug!("File {} is already open and unchanged", abs_path.display());
                return Ok(());
            }

            // File has changed, send change notification
            info!(
                "File {} has changed, sending change notification",
                abs_path.display()
            );

            let new_version = self.next_version;
            self.next_version += 1;

            client
                .change_text_document(uri.clone(), new_version, content)
                .await?;

            // Update entry with new hash and version
            self.opened_files.insert(
                abs_path,
                FileEntry {
                    uri,
                    content_hash,
                    version: new_version,
                },
            );
        } else {
            // File is not open, send open notification
            info!("Opening file {}", abs_path.display());

            let version = self.next_version;
            self.next_version += 1;

            // Determine language ID based on file extension
            let language_id = Self::get_language_id(&abs_path);

            client
                .open_text_document(uri.clone(), language_id.to_string(), version, content)
                .await?;

            // Track the opened file
            self.opened_files.insert(
                abs_path,
                FileEntry {
                    uri,
                    content_hash,
                    version,
                },
            );
        }

        Ok(())
    }

    /// Close a file in the LSP server
    pub async fn close_file(
        &mut self,
        path: &Path,
        client: &mut impl LspClientTrait,
    ) -> Result<(), FileManagerError> {
        // Convert to absolute path
        let abs_path = path
            .canonicalize()
            .map_err(|e| FileManagerError::InvalidPath(format!("{}: {}", path.display(), e)))?;

        if let Some(entry) = self.opened_files.remove(&abs_path) {
            info!("Closing file {}", abs_path.display());
            client.close_text_document(entry.uri).await?;
            Ok(())
        } else {
            debug!("File {} was not open", abs_path.display());
            Ok(())
        }
    }

    /// Check if a file is currently open
    pub fn is_file_open(&self, path: &Path) -> bool {
        if let Ok(abs_path) = path.canonicalize() {
            self.opened_files.contains_key(&abs_path)
        } else {
            false
        }
    }

    /// Get the number of currently open files
    pub fn get_open_files_count(&self) -> usize {
        self.opened_files.len()
    }

    /// Close all open files
    pub async fn close_all_files(
        &mut self,
        client: &mut impl LspClientTrait,
    ) -> Result<(), FileManagerError> {
        let files: Vec<PathBuf> = self.opened_files.keys().cloned().collect();

        for file in files {
            if let Err(e) = self.close_file(&file, client).await {
                warn!("Failed to close file {}: {}", file.display(), e);
            }
        }

        Ok(())
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Compute SHA256 hash of content
    fn compute_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Determine language ID based on file extension
    fn get_language_id(path: &Path) -> &'static str {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("c") => "c",
            Some("cpp") | Some("cc") | Some("cxx") | Some("c++") => "cpp",
            Some("h") | Some("hpp") | Some("hh") | Some("hxx") | Some("h++") => "cpp",
            _ => "cpp", // Default to C++ for clangd
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    #[test]
    fn test_compute_hash() {
        let content1 = "Hello, world!";
        let content2 = "Hello, world!";
        let content3 = "Hello, World!"; // Different content

        let hash1 = ClangdFileManager::compute_hash(content1);
        let hash2 = ClangdFileManager::compute_hash(content2);
        let hash3 = ClangdFileManager::compute_hash(content3);

        // Same content should produce same hash
        assert_eq!(hash1, hash2);

        // Different content should produce different hash
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_language_id_detection() {
        use std::path::PathBuf;

        assert_eq!(
            ClangdFileManager::get_language_id(&PathBuf::from("test.c")),
            "c"
        );
        assert_eq!(
            ClangdFileManager::get_language_id(&PathBuf::from("test.cpp")),
            "cpp"
        );
        assert_eq!(
            ClangdFileManager::get_language_id(&PathBuf::from("test.cc")),
            "cpp"
        );
        assert_eq!(
            ClangdFileManager::get_language_id(&PathBuf::from("test.h")),
            "cpp"
        );
        assert_eq!(
            ClangdFileManager::get_language_id(&PathBuf::from("test.hpp")),
            "cpp"
        );
        assert_eq!(
            ClangdFileManager::get_language_id(&PathBuf::from("test.txt")),
            "cpp"
        );
    }

    #[test]
    fn test_file_tracking() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.cpp");
        fs::write(&file_path, "int main() { return 0; }").unwrap();

        let manager = ClangdFileManager::new();

        // Initially no files should be open
        assert_eq!(manager.get_open_files_count(), 0);
        assert!(!manager.is_file_open(&file_path));
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_ensure_file_ready_opens_new_file() {
        use crate::clangd::{ClangdConfigBuilder, ClangdSession};

        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let test_file = temp_dir.path().join("test.cpp");
        fs::write(&test_file, "int main() { return 0; }").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();

        // Ensure file is ready through session API
        session.ensure_file_ready(&test_file).await.unwrap();

        assert!(session.is_file_open(&test_file));
        assert_eq!(session.get_open_files_count(), 1);

        session.close().await.unwrap();
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_ensure_file_ready_detects_changes() {
        use crate::clangd::{ClangdConfigBuilder, ClangdSession};

        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let test_file = temp_dir.path().join("test.cpp");
        let initial_content = "int main() { return 0; }";
        fs::write(&test_file, initial_content).unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();

        // First call should open the file
        session.ensure_file_ready(&test_file).await.unwrap();
        assert_eq!(session.get_open_files_count(), 1);

        // Second call with same content should do nothing
        session.ensure_file_ready(&test_file).await.unwrap();
        assert_eq!(session.get_open_files_count(), 1);

        // Modify the file
        let new_content = "int main() { return 42; }";
        fs::write(&test_file, new_content).unwrap();

        // Third call should detect the change and send notification
        session.ensure_file_ready(&test_file).await.unwrap();
        assert_eq!(session.get_open_files_count(), 1);
        assert!(session.is_file_open(&test_file));

        session.close().await.unwrap();
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_file_not_found_error() {
        use crate::clangd::{ClangdConfigBuilder, ClangdSession};

        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();

        let non_existent_file = temp_dir.path().join("does_not_exist.cpp");

        // Should return an error for non-existent file
        let result = session.ensure_file_ready(&non_existent_file).await;
        assert!(result.is_err());
        assert_eq!(session.get_open_files_count(), 0);

        session.close().await.unwrap();
    }
}
