//! Index triggering abstraction
//!
//! This module provides the IndexTrigger trait for decoupling index triggering
//! from the component monitor. This allows ComponentIndexMonitor to trigger
//! indexing without knowing about ClangdSession details.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::project::ProjectError;

/// Trait for triggering indexing operations
///
/// This trait encapsulates the mechanism for initiating indexing of specific files,
/// allowing ComponentIndexMonitor to trigger indexing without depending on
/// ClangdSession directly.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait IndexTrigger: Send + Sync {
    /// Trigger indexing by opening the specified file
    ///
    /// This method should cause the underlying indexing system (e.g., clangd)
    /// to begin indexing the specified file and its dependencies.
    ///
    /// # Arguments
    /// * `file_path` - Path to the source file to trigger indexing for
    ///
    /// # Returns
    /// * `Ok(())` if indexing was successfully triggered
    /// * `Err(ProjectError)` if triggering failed
    async fn trigger(&self, file_path: &Path) -> Result<(), ProjectError>;
}

/// Implementation of IndexTrigger that uses ClangdSession
///
/// This implementation encapsulates the ClangdSession dependency and provides
/// the bridge between ComponentIndexMonitor and clangd for triggering indexing.
pub struct ClangdIndexTrigger {
    /// The clangd session to use for triggering indexing
    session: Arc<Mutex<ClangdSession>>,
}

impl ClangdIndexTrigger {
    /// Create a new ClangdIndexTrigger with the given session
    ///
    /// # Arguments
    /// * `session` - The ClangdSession to use for triggering indexing
    pub fn new(session: Arc<Mutex<ClangdSession>>) -> Self {
        debug!("Created ClangdIndexTrigger");
        Self { session }
    }
}

#[async_trait]
impl IndexTrigger for ClangdIndexTrigger {
    async fn trigger(&self, file_path: &Path) -> Result<(), ProjectError> {
        debug!("Triggering indexing for file: {:?}", file_path);

        let mut session = self.session.lock().await;
        session.ensure_file_ready(file_path).await.map_err(|e| {
            ProjectError::IndexingTrigger(format!(
                "Failed to trigger indexing for {}: {}",
                file_path.display(),
                e
            ))
        })?;

        // Request document symbols to force additional symbol processing
        let file_uri = crate::symbol::uri_from_pathbuf(file_path);
        debug!("Requesting document symbols for file: {:?}", file_path);
        let client = session.client_mut();
        match client.text_document_document_symbol(file_uri).await {
            Ok(_) => {
                debug!(
                    "Successfully retrieved document symbols for file: {:?}",
                    file_path
                );
            }
            Err(e) => {
                debug!(
                    "Failed to retrieve document symbols for {}: {} (continuing anyway)",
                    file_path.display(),
                    e
                );
                // Don't fail the trigger operation if document symbols fails
            }
        }

        debug!("Successfully triggered indexing for file: {:?}", file_path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_mock_index_trigger() {
        let mut mock_trigger = MockIndexTrigger::new();

        let test_path = PathBuf::from("/test/file.cpp");
        let expected_path = test_path.clone();
        mock_trigger
            .expect_trigger()
            .with(mockall::predicate::function(move |path: &Path| {
                path == expected_path
            }))
            .times(1)
            .returning(|_| Ok(()));

        let result = mock_trigger.trigger(&test_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_index_trigger_failure() {
        let mut mock_trigger = MockIndexTrigger::new();

        let test_path = PathBuf::from("/test/file.cpp");
        let expected_path = test_path.clone();
        mock_trigger
            .expect_trigger()
            .with(mockall::predicate::function(move |path: &Path| {
                path == expected_path
            }))
            .times(1)
            .returning(|_| Err(ProjectError::IndexingTrigger("Test error".to_string())));

        let result = mock_trigger.trigger(&test_path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Test error"));
    }
}
