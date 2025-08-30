//! IndexSession facade for tool indexing operations
//!
//! This module provides a clean API for tools to ensure indexing completion
//! without needing to understand WorkspaceSession internals.

use crate::project::{ProjectError, WorkspaceSession};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info};

/// Default timeout for tool indexing operations
const DEFAULT_TOOL_INDEXING_TIMEOUT: Duration = Duration::from_secs(30);

/// IndexSession facade provides a clean API for tools to manage indexing
///
/// This facade wraps WorkspaceSession and provides tool-focused indexing operations
/// without exposing the complexity of workspace session management.
pub struct IndexSession<'a> {
    workspace_session: &'a WorkspaceSession,
    build_dir: PathBuf,
    timeout: Duration,
}

impl<'a> IndexSession<'a> {
    /// Create a new IndexSession with default timeout
    pub fn new(workspace_session: &'a WorkspaceSession, build_dir: PathBuf) -> Self {
        debug!(
            "Creating IndexSession for build directory: {}",
            build_dir.display()
        );
        Self {
            workspace_session,
            build_dir,
            timeout: DEFAULT_TOOL_INDEXING_TIMEOUT,
        }
    }

    /// Configure a custom timeout for indexing operations
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        debug!("Setting IndexSession timeout to {:?}", timeout);
        self.timeout = timeout;
        self
    }

    /// Ensure the project is fully indexed before proceeding
    ///
    /// This method delegates to WorkspaceSession's wait_for_indexing_completion_with_timeout
    /// which uses the robust event-driven indexing system with single waiter enforcement.
    pub async fn ensure_indexed(&self) -> Result<(), ProjectError> {
        info!(
            "Ensuring indexing completion for build directory: {} (timeout: {:?})",
            self.build_dir.display(),
            self.timeout
        );

        // Use the workspace session's timeout-aware method directly
        // No need for double timeout wrapping since the IndexMonitor now handles timeouts internally
        self.workspace_session
            .wait_for_indexing_completion_with_timeout(&self.build_dir, self.timeout)
            .await
    }

    /// Get current indexing coverage as a percentage (0.0 to 1.0)
    ///
    /// This is provided for debugging and monitoring purposes.
    /// Tools should rely on ensure_indexed() for completion checking.
    pub async fn get_coverage(&self) -> Option<f64> {
        self.workspace_session
            .get_indexing_coverage(&self.build_dir)
            .await
            .map(|f| f as f64)
    }

    /// Get the build directory for this index session
    pub fn build_directory(&self) -> &PathBuf {
        &self.build_dir
    }
}

impl<'a> std::fmt::Debug for IndexSession<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexSession")
            .field("build_dir", &self.build_dir)
            .field("timeout", &self.timeout)
            .finish()
    }
}
