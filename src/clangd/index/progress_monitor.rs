//! Clangd indexing progress monitor
//!
//! Provides async monitoring of clangd's background indexing process by listening
//! to LSP progress notifications and emitting progress events.

use crate::clangd::index::ProgressEvent;
use crate::lsp::protocol::JsonRpcNotification;
use lsp_types::{notification::Notification, request::Request};
use serde_json::Value;
use std::sync::Arc;
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
// IndexProgressMonitor
// ============================================================================

/// Monitor for clangd indexing progress
///
/// Listens to LSP progress notifications and emits progress events via mpsc channel.
/// This monitor focuses solely on tracking progress - completion signaling is handled elsewhere.
#[derive(Clone)]
pub struct IndexProgressMonitor {
    /// Shared state protected by mutex
    state: Arc<Mutex<IndexingState>>,
    /// Optional progress event sender
    progress_sender: Option<mpsc::Sender<ProgressEvent>>,
}

impl IndexProgressMonitor {
    /// Create a new index progress monitor
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(IndexingState::default())),
            progress_sender: None,
        }
    }

    /// Create a new index progress monitor with progress event sender
    pub fn with_sender(sender: mpsc::Sender<ProgressEvent>) -> Self {
        Self {
            state: Arc::new(Mutex::new(IndexingState::default())),
            progress_sender: Some(sender),
        }
    }

    /// Create a notification handler that can be registered with LSP client
    ///
    /// Returns a handler that satisfies the 'static lifetime requirement
    /// by capturing only the shared state Arc.
    pub fn create_handler(&self) -> impl Fn(JsonRpcNotification) + Send + Sync + 'static {
        let state = Arc::clone(&self.state);
        let progress_sender = self.progress_sender.clone();
        move |notification| {
            let state = Arc::clone(&state);
            let progress_sender = progress_sender.clone();
            // Process notification in background to avoid blocking LSP transport
            tokio::spawn(async move {
                Self::process_notification_internal(notification, state, progress_sender).await;
            });
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
        debug!("IndexProgressMonitor: Reset indexing state");
    }

    /// Internal notification processing
    async fn process_notification_internal(
        notification: JsonRpcNotification,
        state: Arc<Mutex<IndexingState>>,
        progress_sender: Option<mpsc::Sender<ProgressEvent>>,
    ) {
        trace!(
            "IndexProgressMonitor: Processing notification: {}",
            notification.method
        );

        match notification.method.as_str() {
            lsp_types::request::WorkDoneProgressCreate::METHOD => {
                Self::handle_progress_create(notification.params, state).await;
            }
            lsp_types::notification::Progress::METHOD => {
                Self::handle_progress_update(notification.params, state, progress_sender).await;
            }
            _ => {
                // Not a progress notification we care about
                trace!(
                    "IndexProgressMonitor: Ignoring notification: {}",
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
            debug!("IndexProgressMonitor: Tracking progress token: {}", token);
        }
    }

    /// Handle $/progress notification
    async fn handle_progress_update(
        params: Option<Value>,
        state: Arc<Mutex<IndexingState>>,
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
                            "IndexProgressMonitor: Indexing started - {}",
                            title.unwrap_or("indexing")
                        );

                        // Emit overall indexing started event
                        if let Some(ref sender) = progress_sender
                            && sender
                                .try_send(ProgressEvent::OverallIndexingStarted)
                                .is_err()
                        {
                            warn!(
                                "IndexProgressMonitor: Failed to send OverallIndexingStarted event"
                            );
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
                            "IndexProgressMonitor: Progress update - {}% ({})",
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
                                warn!("IndexProgressMonitor: Failed to send OverallProgress event");
                            }
                        }
                    }
                    Some("end") => {
                        let mut state = state.lock().await;
                        state.status = IndexingStatus::Completed;
                        drop(state); // Release lock before emitting event

                        debug!("IndexProgressMonitor: Indexing completed");

                        // Emit overall indexing completed event
                        if let Some(ref sender) = progress_sender
                            && sender.try_send(ProgressEvent::OverallCompleted).is_err()
                        {
                            warn!("IndexProgressMonitor: Failed to send OverallCompleted event");
                        }
                    }
                    _ => {
                        trace!("IndexProgressMonitor: Unknown progress kind: {:?}", kind);
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
    async fn test_index_progress_monitor_creation() {
        let monitor = IndexProgressMonitor::new();
        let status = monitor.get_progress().await;
        assert_eq!(status, IndexingStatus::NotStarted);
    }

    #[tokio::test]
    async fn test_index_progress_monitor_reset() {
        let monitor = IndexProgressMonitor::new();

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
        let monitor = IndexProgressMonitor::new();
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
        let monitor = IndexProgressMonitor::new();
        let state = Arc::clone(&monitor.state);

        let params = json!({
            "token": "backgroundIndexProgress"
        });

        IndexProgressMonitor::handle_progress_create(Some(params), state.clone()).await;

        let state = state.lock().await;
        assert_eq!(
            state.progress_token,
            Some("backgroundIndexProgress".to_string())
        );
    }

    #[tokio::test]
    async fn test_progress_update_begin() {
        let monitor = IndexProgressMonitor::new();
        let state = Arc::clone(&monitor.state);

        let params = json!({
            "token": "backgroundIndexProgress",
            "value": {
                "kind": "begin",
                "percentage": 0,
                "title": "indexing"
            }
        });

        IndexProgressMonitor::handle_progress_update(Some(params), state.clone(), None).await;

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
        let monitor = IndexProgressMonitor::new();
        let state = Arc::clone(&monitor.state);

        let params = json!({
            "token": "backgroundIndexProgress",
            "value": {
                "kind": "end"
            }
        });

        IndexProgressMonitor::handle_progress_update(Some(params), state.clone(), None).await;

        let state = state.lock().await;
        assert_eq!(state.status, IndexingStatus::Completed);
    }
}
