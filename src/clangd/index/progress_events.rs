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
    /// Standard library indexing started
    StandardLibraryStarted {
        context_file: PathBuf,
        stdlib_version: String,
    },
    /// Standard library indexing completed
    StandardLibraryCompleted { symbols: u32, filtered: u32 },
    /// Overall indexing progress update
    OverallProgress {
        current: u32,
        total: u32,
        percentage: u8,
        message: Option<String>,
    },
    /// Overall indexing completed
    OverallCompleted,
    /// Indexing failed
    IndexingFailed { error: String },
}

/// Trait for handling progress events
pub trait ProgressHandler: Send + Sync {
    /// Handle a progress event
    fn handle_event(&self, event: ProgressEvent);
}

/// Blanket implementation for closures
impl<F> ProgressHandler for F
where
    F: Fn(ProgressEvent) + Send + Sync,
{
    fn handle_event(&self, event: ProgressEvent) {
        self(event);
    }
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
    fn test_progress_handler_trait() {
        let handler = |event: ProgressEvent| {
            if let ProgressEvent::FileIndexingStarted { .. } = event {
                // Handle the event
            }
        };

        let event = ProgressEvent::FileIndexingStarted {
            path: PathBuf::from("/test.cpp"),
            digest: "TEST".to_string(),
        };

        handler.handle_event(event);
    }
}
