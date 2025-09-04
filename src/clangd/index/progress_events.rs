//! Progress event types for clangd indexing monitoring
//!
//! This module defines the event types and traits used for monitoring clangd
//! indexing progress through multiple channels (LSP notifications and stderr logs).

use std::path::PathBuf;

/// Progress events emitted during clangd indexing
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressEvent {
    /// File indexing started
    FileIndexingStarted { path: PathBuf, digest: String },
    /// File indexing completed
    FileIndexingCompleted {
        path: PathBuf,
        symbols: u32,
        refs: u32,
    },
    /// File AST indexed and available
    FileAstIndexed { path: PathBuf },
    /// Standard library indexing started
    StandardLibraryStarted {
        context_file: PathBuf,
        stdlib_version: String,
    },
    /// Standard library indexing completed
    StandardLibraryCompleted { symbols: u32, filtered: u32 },
    /// Overall indexing started
    OverallIndexingStarted,
    /// Overall indexing progress update
    OverallProgress {
        current: u32,
        total: u32,
        percentage: u8,
        message: Option<String>,
    },
    /// Overall indexing completed
    OverallCompleted,
    /// File AST build failed
    FileAstFailed { path: PathBuf },
    /// Indexing failed
    IndexingFailed { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_event_creation() {
        let event = ProgressEvent::FileIndexingStarted {
            path: PathBuf::from("/test/file.cpp"),
            digest: "ABC123".to_string(),
        };

        match event {
            ProgressEvent::FileIndexingStarted { path, digest } => {
                assert_eq!(path, PathBuf::from("/test/file.cpp"));
                assert_eq!(digest, "ABC123");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_file_ast_failed_event_creation() {
        let event = ProgressEvent::FileAstFailed {
            path: PathBuf::from("/test/failed.cpp"),
        };

        match event {
            ProgressEvent::FileAstFailed { path } => {
                assert_eq!(path, PathBuf::from("/test/failed.cpp"));
            }
            _ => panic!("Wrong event type"),
        }
    }
}
