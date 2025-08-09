//! Clangd indexing progress monitor
//!
//! Provides async monitoring of clangd's background indexing process by listening
//! to LSP progress notifications and enabling code to wait for indexing completion.

use crate::lsp_v2::protocol::JsonRpcNotification;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, trace};

// ============================================================================
// Types and Errors
// ============================================================================

/// Indexing status tracking
#[derive(Debug, Clone, PartialEq)]
pub enum IndexingStatus {
    /// Indexing has not started yet
    NotStarted,
    /// Indexing is in progress
    InProgress {
        current: u32,
        total: u32,
        percentage: u8,
        message: Option<String>,
    },
    /// Indexing has completed successfully
    Completed,
    /// Indexing failed with an error
    Failed(String),
}

/// Errors that can occur during indexing monitoring
#[derive(Debug, thiserror::Error)]
pub enum IndexingError {
    #[error("Indexing was cancelled")]
    Cancelled,

    #[error("Indexing failed: {0}")]
    Failed(String),

    #[error("Timeout waiting for indexing completion")]
    Timeout,
}

// ============================================================================
// Internal State
// ============================================================================

/// Internal state for the indexing monitor
#[derive(Debug)]
struct IndexingState {
    /// Current indexing status
    status: IndexingStatus,
    /// Progress token from clangd (usually "backgroundIndexProgress")
    progress_token: Option<String>,
    /// Channels to notify when indexing completes
    completion_waiters: Vec<oneshot::Sender<Result<(), IndexingError>>>,
}

impl Default for IndexingState {
    fn default() -> Self {
        Self {
            status: IndexingStatus::NotStarted,
            progress_token: None,
            completion_waiters: Vec::new(),
        }
    }
}

// ============================================================================
// IndexMonitor
// ============================================================================

/// Monitor for clangd indexing progress
///
/// Listens to LSP progress notifications and provides async waiting capabilities
/// for indexing completion.
#[derive(Clone)]
pub struct IndexMonitor {
    /// Shared state protected by mutex
    state: Arc<Mutex<IndexingState>>,
}

impl IndexMonitor {
    /// Create a new index monitor
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(IndexingState::default())),
        }
    }

    /// Create a notification handler that can be registered with LSP client
    ///
    /// Returns a handler that satisfies the 'static lifetime requirement
    /// by capturing only the shared state Arc.
    pub fn create_handler(&self) -> impl Fn(JsonRpcNotification) + Send + Sync + 'static {
        let state = Arc::clone(&self.state);
        move |notification| {
            let state = Arc::clone(&state);
            // Process notification in background to avoid blocking LSP transport
            tokio::spawn(async move {
                Self::process_notification_internal(notification, state).await;
            });
        }
    }

    /// Wait for indexing to complete
    ///
    /// Returns immediately if indexing is already complete, otherwise waits
    /// until completion or failure.
    pub async fn wait_for_indexing_completion(&self) -> Result<(), IndexingError> {
        let mut state = self.state.lock().await;

        match &state.status {
            IndexingStatus::Completed => Ok(()),
            IndexingStatus::Failed(err) => Err(IndexingError::Failed(err.clone())),
            _ => {
                // Create a oneshot channel to wait for completion
                let (sender, receiver) = oneshot::channel();
                state.completion_waiters.push(sender);
                drop(state); // Release lock before awaiting

                // Wait for completion signal
                receiver.await.map_err(|_| IndexingError::Cancelled)?
            }
        }
    }

    /// Get current indexing progress without waiting
    pub async fn get_progress(&self) -> IndexingStatus {
        let state = self.state.lock().await;
        state.status.clone()
    }

    /// Reset indexing state (useful for new indexing cycles)
    pub async fn reset(&self) {
        let mut state = self.state.lock().await;
        *state = IndexingState::default();
        debug!("IndexMonitor: Reset indexing state");
    }

    /// Internal notification processing
    async fn process_notification_internal(
        notification: JsonRpcNotification,
        state: Arc<Mutex<IndexingState>>,
    ) {
        trace!(
            "IndexMonitor: Processing notification: {}",
            notification.method
        );

        match notification.method.as_str() {
            "window/workDoneProgress/create" => {
                Self::handle_progress_create(notification.params, state).await;
            }
            "$/progress" => {
                Self::handle_progress_update(notification.params, state).await;
            }
            _ => {
                // Not a progress notification we care about
                trace!(
                    "IndexMonitor: Ignoring notification: {}",
                    notification.method
                );
            }
        }
    }

    /// Handle window/workDoneProgress/create notification
    async fn handle_progress_create(params: Option<Value>, state: Arc<Mutex<IndexingState>>) {
        if let Some(params) = params
            && let Some(token) = params.get("token").and_then(|t| t.as_str())
            && token == "backgroundIndexProgress"
        {
            let mut state = state.lock().await;
            state.progress_token = Some(token.to_string());
            debug!("IndexMonitor: Tracking progress token: {}", token);
        }
    }

    /// Handle $/progress notification
    async fn handle_progress_update(params: Option<Value>, state: Arc<Mutex<IndexingState>>) {
        if let Some(params) = params {
            let token = params.get("token").and_then(|t| t.as_str());

            // Only process notifications for our tracked token
            if token != Some("backgroundIndexProgress") {
                return;
            }

            if let Some(value) = params.get("value") {
                let kind = value.get("kind").and_then(|k| k.as_str());

                match kind {
                    Some("begin") => {
                        let percentage = value
                            .get("percentage")
                            .and_then(|p| p.as_u64())
                            .unwrap_or(0) as u8;
                        let title = value.get("title").and_then(|t| t.as_str());

                        let mut state = state.lock().await;
                        state.status = IndexingStatus::InProgress {
                            current: 0,
                            total: 1, // Will be updated in report messages
                            percentage,
                            message: title.map(|s| s.to_string()),
                        };

                        debug!(
                            "IndexMonitor: Indexing started - {}",
                            title.unwrap_or("indexing")
                        );
                    }
                    Some("report") => {
                        let percentage = value
                            .get("percentage")
                            .and_then(|p| p.as_u64())
                            .unwrap_or(0) as u8;
                        let message = value.get("message").and_then(|m| m.as_str());

                        // Parse "current/total" from message if available
                        let (current, total) = if let Some(msg) = message {
                            if let Some((c, t)) = parse_progress_message(msg) {
                                (c, t)
                            } else {
                                (0, 1)
                            }
                        } else {
                            (0, 1)
                        };

                        let mut state = state.lock().await;
                        state.status = IndexingStatus::InProgress {
                            current,
                            total,
                            percentage,
                            message: message.map(|s| s.to_string()),
                        };

                        trace!(
                            "IndexMonitor: Progress update - {}% ({})",
                            percentage,
                            message.unwrap_or("")
                        );
                    }
                    Some("end") => {
                        let mut state = state.lock().await;
                        state.status = IndexingStatus::Completed;

                        // Notify all waiters
                        for sender in state.completion_waiters.drain(..) {
                            let _ = sender.send(Ok(()));
                        }

                        debug!("IndexMonitor: Indexing completed");
                    }
                    _ => {
                        trace!("IndexMonitor: Unknown progress kind: {:?}", kind);
                    }
                }
            }
        }
    }
}

/// Parse progress message in format "current/total" -> (current, total)
fn parse_progress_message(message: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = message.split('/').collect();
    if parts.len() == 2
        && let (Ok(current), Ok(total)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
    {
        return Some((current, total));
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_progress_message() {
        assert_eq!(parse_progress_message("0/7"), Some((0, 7)));
        assert_eq!(parse_progress_message("3/7"), Some((3, 7)));
        assert_eq!(parse_progress_message("invalid"), None);
        assert_eq!(parse_progress_message("1/2/3"), None);
        assert_eq!(parse_progress_message("a/b"), None);
    }

    #[tokio::test]
    async fn test_index_monitor_creation() {
        let monitor = IndexMonitor::new();
        let status = monitor.get_progress().await;
        assert_eq!(status, IndexingStatus::NotStarted);
    }

    #[tokio::test]
    async fn test_index_monitor_reset() {
        let monitor = IndexMonitor::new();

        // Simulate some progress
        {
            let mut state = monitor.state.lock().await;
            state.status = IndexingStatus::InProgress {
                current: 1,
                total: 5,
                percentage: 20,
                message: Some("test".to_string()),
            };
        }

        // Reset should clear state
        monitor.reset().await;
        let status = monitor.get_progress().await;
        assert_eq!(status, IndexingStatus::NotStarted);
    }

    #[tokio::test]
    async fn test_notification_handler_creation() {
        let monitor = IndexMonitor::new();
        let handler = monitor.create_handler();

        // Handler should be callable
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        };

        handler(notification);
        // Should not panic or block
    }

    #[tokio::test]
    async fn test_progress_create_notification() {
        let monitor = IndexMonitor::new();
        let state = Arc::clone(&monitor.state);

        let params = json!({
            "token": "backgroundIndexProgress"
        });

        IndexMonitor::handle_progress_create(Some(params), state.clone()).await;

        let state = state.lock().await;
        assert_eq!(
            state.progress_token,
            Some("backgroundIndexProgress".to_string())
        );
    }

    #[tokio::test]
    async fn test_progress_update_begin() {
        let monitor = IndexMonitor::new();
        let state = Arc::clone(&monitor.state);

        let params = json!({
            "token": "backgroundIndexProgress",
            "value": {
                "kind": "begin",
                "percentage": 0,
                "title": "indexing"
            }
        });

        IndexMonitor::handle_progress_update(Some(params), state.clone()).await;

        let state = state.lock().await;
        if let IndexingStatus::InProgress {
            percentage,
            message,
            ..
        } = &state.status
        {
            assert_eq!(*percentage, 0);
            assert_eq!(*message, Some("indexing".to_string()));
        } else {
            panic!("Expected InProgress status");
        }
    }

    #[tokio::test]
    async fn test_progress_update_end() {
        let monitor = IndexMonitor::new();
        let state = Arc::clone(&monitor.state);

        let params = json!({
            "token": "backgroundIndexProgress",
            "value": {
                "kind": "end"
            }
        });

        IndexMonitor::handle_progress_update(Some(params), state.clone()).await;

        let state = state.lock().await;
        assert_eq!(state.status, IndexingStatus::Completed);
    }

    #[tokio::test]
    async fn test_wait_for_completion_already_done() {
        let monitor = IndexMonitor::new();

        // Set status to completed
        {
            let mut state = monitor.state.lock().await;
            state.status = IndexingStatus::Completed;
        }

        // Should return immediately
        let result = monitor.wait_for_indexing_completion().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_for_completion_already_failed() {
        let monitor = IndexMonitor::new();

        // Set status to failed
        {
            let mut state = monitor.state.lock().await;
            state.status = IndexingStatus::Failed("test error".to_string());
        }

        // Should return error immediately
        let result = monitor.wait_for_indexing_completion().await;
        assert!(result.is_err());
        if let Err(IndexingError::Failed(msg)) = result {
            assert_eq!(msg, "test error");
        } else {
            panic!("Expected Failed error");
        }
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[cfg(all(test, feature = "clangd-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::clangd::config::ClangdConfigBuilder;
    use crate::clangd::session::ClangdSession;
    use crate::test_utils::integration::TestProject;
    use std::time::Duration;

    #[tokio::test]
    async fn test_index_monitor_with_real_clangd() {
        // Create test project with real C++ files
        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        // Create clangd session (which auto-wires IndexMonitor)
        let config = ClangdConfigBuilder::new()
            .working_directory(&test_project.project_root)
            .build_directory(&test_project.build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .add_arg("--log=verbose")
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();

        // Verify initial state
        let initial_status = session.index_monitor().get_progress().await;
        assert_eq!(initial_status, IndexingStatus::NotStarted);

        // Open main.cpp to trigger indexing using the proper file manager API
        let main_cpp_path = test_project.project_root.join("src/main.cpp");
        session.ensure_file_ready(&main_cpp_path).await.unwrap();

        // Wait for indexing to complete with timeout
        let completion_result = tokio::time::timeout(
            Duration::from_secs(30),
            session.index_monitor().wait_for_indexing_completion(),
        )
        .await;

        // Verify indexing completed successfully
        match completion_result {
            Ok(Ok(())) => {
                let final_status = session.index_monitor().get_progress().await;
                assert_eq!(final_status, IndexingStatus::Completed);
            }
            Ok(Err(e)) => panic!("Indexing failed: {e}"),
            Err(_) => panic!("Indexing timed out after 30 seconds"),
        }

        // Clean up
        session.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_waiters() {
        // Create test project
        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        // Create clangd session
        let config = ClangdConfigBuilder::new()
            .working_directory(&test_project.project_root)
            .build_directory(&test_project.build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .add_arg("--log=verbose")
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();
        let monitor = session.index_monitor();

        // Start multiple waiters concurrently
        let waiter1 = tokio::spawn({
            let monitor = monitor.clone();
            async move { monitor.wait_for_indexing_completion().await }
        });
        let waiter2 = tokio::spawn({
            let monitor = monitor.clone();
            async move { monitor.wait_for_indexing_completion().await }
        });
        let waiter3 = tokio::spawn({
            let monitor = monitor.clone();
            async move { monitor.wait_for_indexing_completion().await }
        });

        // Open main.cpp to trigger indexing using the proper file manager API
        let main_cpp_path = test_project.project_root.join("src/main.cpp");
        session.ensure_file_ready(&main_cpp_path).await.unwrap();

        // All waiters should complete successfully
        let (result1, result2, result3) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(waiter1, waiter2, waiter3)
        })
        .await
        .expect("Timeout waiting for multiple waiters");

        // Check that all tasks completed successfully
        result1.unwrap().unwrap(); // Task result -> IndexingError result
        result2.unwrap().unwrap();
        result3.unwrap().unwrap();

        session.close().await.unwrap();
    }
}
