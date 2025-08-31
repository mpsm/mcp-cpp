//! Clangd indexing progress monitor
//!
//! Provides async monitoring of clangd's background indexing process by listening
//! to LSP progress notifications and enabling code to wait for indexing completion.

use crate::clangd::index::{IndexLatch, LatchError, ProgressEvent};
use crate::lsp::protocol::JsonRpcNotification;
use lsp_types::{notification::Notification, request::Request};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, trace, warn};

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
///
/// Re-exports LatchError for backward compatibility
#[allow(unused_imports)]
pub use crate::clangd::index::LatchError as IndexingError;

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
}

impl Default for IndexingState {
    fn default() -> Self {
        Self {
            status: IndexingStatus::NotStarted,
            progress_token: None,
        }
    }
}

// ============================================================================
// IndexMonitor
// ============================================================================

/// Monitor for clangd indexing progress
///
/// Listens to LSP progress notifications and provides async waiting capabilities
/// for indexing completion with single waiter enforcement and custom timeouts.
#[derive(Clone)]
pub struct IndexMonitor {
    /// Shared state protected by mutex
    state: Arc<Mutex<IndexingState>>,
    /// Event latch for completion waiting
    latch: IndexLatch,
    /// Optional progress event sender
    progress_sender: Option<mpsc::Sender<ProgressEvent>>,
}

impl IndexMonitor {
    /// Create a new index monitor
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(IndexingState::default())),
            latch: IndexLatch::new(),
            progress_sender: None,
        }
    }

    /// Create a new index monitor with progress event sender
    pub fn with_sender(sender: mpsc::Sender<ProgressEvent>) -> Self {
        Self {
            state: Arc::new(Mutex::new(IndexingState::default())),
            latch: IndexLatch::new(),
            progress_sender: Some(sender),
        }
    }

    /// Create a notification handler that can be registered with LSP client
    ///
    /// Returns a handler that satisfies the 'static lifetime requirement
    /// by capturing only the shared state Arc and latch.
    pub fn create_handler(&self) -> impl Fn(JsonRpcNotification) + Send + Sync + 'static {
        let state = Arc::clone(&self.state);
        let latch = self.latch.clone();
        let progress_sender = self.progress_sender.clone();
        move |notification| {
            let state = Arc::clone(&state);
            let latch = latch.clone();
            let progress_sender = progress_sender.clone();
            // Process notification in background to avoid blocking LSP transport
            tokio::spawn(async move {
                Self::process_notification_internal(notification, state, latch, progress_sender)
                    .await;
            });
        }
    }

    /// Wait for indexing to complete with default timeout
    ///
    /// Returns immediately if indexing is already complete, otherwise waits
    /// until completion or failure. Uses default 30 second timeout.
    /// Enforces single waiter constraint.
    pub async fn wait_for_indexing_completion(&self) -> Result<(), LatchError> {
        self.latch.wait_default().await
    }

    /// Wait for indexing to complete with custom timeout
    ///
    /// Returns immediately if indexing is already complete, otherwise waits
    /// until completion or failure. Enforces single waiter constraint.
    pub async fn wait_for_indexing_completion_with_timeout(
        &self,
        timeout: Duration,
    ) -> Result<(), LatchError> {
        self.latch.wait(timeout).await
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
        self.latch.reset().await;
        debug!("IndexMonitor: Reset indexing state and latch");
    }

    /// Mark indexing as complete based on disk state
    ///
    /// This is used when we detect from disk that indexing is already complete
    /// and clangd won't send progress notifications (e.g., when index files already exist).
    pub async fn mark_complete_from_disk(&self) {
        let mut state = self.state.lock().await;
        state.status = IndexingStatus::Completed;
        drop(state); // Release lock before triggering latch

        // Trigger completion latch
        self.latch.trigger_success().await;
        debug!("IndexMonitor: Marked indexing as complete based on disk state");
    }

    /// Mark indexing as failed and emit failure event
    pub async fn mark_failed(&self, error: String) {
        let mut state = self.state.lock().await;
        state.status = IndexingStatus::Failed(error.clone());
        drop(state); // Release lock before triggering latch

        // Trigger failure latch
        self.latch.trigger_failure(error.clone()).await;

        // Emit indexing failed event
        if let Some(ref sender) = self.progress_sender
            && sender
                .try_send(ProgressEvent::IndexingFailed { error })
                .is_err()
        {
            warn!("IndexMonitor: Failed to send IndexingFailed event");
        }

        debug!("IndexMonitor: Marked indexing as failed");
    }

    /// Internal notification processing
    async fn process_notification_internal(
        notification: JsonRpcNotification,
        state: Arc<Mutex<IndexingState>>,
        latch: IndexLatch,
        progress_sender: Option<mpsc::Sender<ProgressEvent>>,
    ) {
        trace!(
            "IndexMonitor: Processing notification: {}",
            notification.method
        );

        match notification.method.as_str() {
            lsp_types::request::WorkDoneProgressCreate::METHOD => {
                Self::handle_progress_create(notification.params, state).await;
            }
            lsp_types::notification::Progress::METHOD => {
                Self::handle_progress_update(notification.params, state, latch, progress_sender)
                    .await;
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
    async fn handle_progress_update(
        params: Option<Value>,
        state: Arc<Mutex<IndexingState>>,
        latch: IndexLatch,
        progress_sender: Option<mpsc::Sender<ProgressEvent>>,
    ) {
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

                        // Emit overall indexing started event
                        if let Some(ref sender) = progress_sender
                            && sender
                                .try_send(ProgressEvent::OverallIndexingStarted)
                                .is_err()
                        {
                            warn!("IndexMonitor: Failed to send OverallIndexingStarted event");
                        }
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

                        // Emit overall progress event
                        if let Some(ref sender) = progress_sender {
                            let event = ProgressEvent::OverallProgress {
                                current,
                                total,
                                percentage,
                                message: message.map(|s| s.to_string()),
                            };
                            if sender.try_send(event).is_err() {
                                warn!("IndexMonitor: Failed to send OverallProgress event");
                            }
                        }
                    }
                    Some("end") => {
                        let mut state = state.lock().await;
                        state.status = IndexingStatus::Completed;
                        drop(state); // Release lock before triggering latch

                        // Trigger completion latch
                        latch.trigger_success().await;

                        debug!("IndexMonitor: Indexing completed");

                        // Emit overall indexing completed event
                        if let Some(ref sender) = progress_sender
                            && sender.try_send(ProgressEvent::OverallCompleted).is_err()
                        {
                            warn!("IndexMonitor: Failed to send OverallCompleted event");
                        }
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
            jsonrpc: crate::lsp::jsonrpc_utils::JSONRPC_VERSION.to_string(),
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

        IndexMonitor::handle_progress_update(Some(params), state.clone(), IndexLatch::new(), None)
            .await;

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

        let latch = IndexLatch::new();
        IndexMonitor::handle_progress_update(Some(params), state.clone(), latch.clone(), None)
            .await;

        let state = state.lock().await;
        assert_eq!(state.status, IndexingStatus::Completed);
        drop(state); // Release lock before checking latch

        // Check that latch was triggered
        assert!(latch.is_completed().await);
    }

    #[tokio::test]
    async fn test_wait_for_completion_already_done() {
        let monitor = IndexMonitor::new();

        // Trigger latch success
        monitor.latch.trigger_success().await;

        // Update status for consistency
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

        // Trigger latch failure
        let error_msg = "test error".to_string();
        monitor.latch.trigger_failure(error_msg.clone()).await;

        // Update status for consistency
        {
            let mut state = monitor.state.lock().await;
            state.status = IndexingStatus::Failed(error_msg.clone());
        }

        // Should return error immediately
        let result = monitor.wait_for_indexing_completion().await;
        assert!(result.is_err());
        match result {
            Err(LatchError::IndexingFailed(msg)) => assert_eq!(msg, error_msg),
            _ => panic!("Expected IndexingFailed error"),
        }
    }

    #[tokio::test]
    async fn test_single_waiter_enforcement() {
        let monitor = IndexMonitor::new();

        // Start first waiter
        let monitor1 = monitor.clone();
        let waiter1 = tokio::spawn(async move { monitor1.wait_for_indexing_completion().await });

        // Give first waiter time to register
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Second waiter should fail immediately
        let result = monitor.wait_for_indexing_completion().await;
        match result {
            Err(LatchError::MultipleWaiters) => {} // Expected
            _ => panic!("Expected MultipleWaiters error, got: {:?}", result),
        }

        // Trigger success for first waiter
        monitor.latch.trigger_success().await;
        let result1 = waiter1.await.unwrap();
        assert!(result1.is_ok());
    }

    #[tokio::test]
    async fn test_timeout_with_custom_duration() {
        let monitor = IndexMonitor::new();

        // Wait with short timeout, should timeout
        let result = monitor
            .wait_for_indexing_completion_with_timeout(Duration::from_millis(50))
            .await;
        match result {
            Err(LatchError::Timeout) => {} // Expected
            _ => panic!("Expected Timeout error, got: {:?}", result),
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
    async fn test_single_waiter_enforcement() {
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

        // Start first waiter
        let waiter1 = tokio::spawn({
            let monitor = monitor.clone();
            async move { monitor.wait_for_indexing_completion().await }
        });

        // Give first waiter time to register
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Try to start additional waiters - these should fail with MultipleWaiters error
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

        // Wait for all tasks to complete
        let (result1, result2, result3) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(waiter1, waiter2, waiter3)
        })
        .await
        .expect("Timeout waiting for waiters");

        // First waiter should succeed
        result1.unwrap().unwrap();

        // Additional waiters should fail with MultipleWaiters error
        match result2.unwrap() {
            Err(LatchError::MultipleWaiters) => {} // Expected
            other => panic!("Expected MultipleWaiters error, got: {:?}", other),
        }
        match result3.unwrap() {
            Err(LatchError::MultipleWaiters) => {} // Expected
            other => panic!("Expected MultipleWaiters error, got: {:?}", other),
        }

        session.close().await.unwrap();
    }
}
