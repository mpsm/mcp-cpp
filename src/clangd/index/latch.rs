//! Event latch for index completion waiting
//!
//! Provides a robust latch pattern for waiting on index completion with single waiter
//! enforcement, custom timeouts, and race condition protection.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, trace};

/// Errors that can occur during latch operations
#[derive(Debug, thiserror::Error)]
pub enum LatchError {
    #[error("Timeout waiting for completion")]
    Timeout,

    #[error("Latch was cancelled")]
    Cancelled,

    #[error("Indexing failed: {0}")]
    IndexingFailed(String),
}

/// Internal state for the indexing latch
#[derive(Clone, Debug, Default)]
enum LatchState {
    /// Initial state - not completed
    #[default]
    Pending,
    /// Indexing completed successfully
    Completed,
    /// Indexing failed with error message
    Failed(String),
}

/// Event latch for index completion waiting
///
/// Uses a watch channel to broadcast completion state to multiple waiters.
/// Once triggered (either success or failure), the latch stays triggered.
/// Supports multiple concurrent waiters and provides race-condition safety.
#[derive(Clone)]
pub struct IndexLatch {
    /// Sender for state updates (single writer)
    state_tx: Arc<watch::Sender<LatchState>>,
    /// Template receiver for cloning to waiters
    state_rx: watch::Receiver<LatchState>,
}

impl IndexLatch {
    /// Create a new index latch
    pub fn new() -> Self {
        debug!("Creating new IndexLatch");
        let (state_tx, state_rx) = watch::channel(LatchState::default());
        Self {
            state_tx: Arc::new(state_tx),
            state_rx,
        }
    }

    /// Wait for the latch to be triggered with custom timeout
    ///
    /// Returns error if:
    /// - Timeout is reached
    /// - Indexing failed
    ///
    /// Supports multiple concurrent waiters and handles race conditions
    /// where completion occurs before waiting starts.
    pub async fn wait(&self, timeout: Duration) -> Result<(), LatchError> {
        let mut rx = self.state_rx.clone();

        // Check current state first (handles race where completion happens before wait)
        match *rx.borrow() {
            LatchState::Completed => {
                debug!("IndexLatch: Already completed, returning immediately");
                return Ok(());
            }
            LatchState::Failed(ref error) => {
                debug!("IndexLatch: Already failed, returning error immediately");
                return Err(LatchError::IndexingFailed(error.clone()));
            }
            LatchState::Pending => {
                trace!("IndexLatch: Waiting for completion");
            }
        }

        // Wait for state change with timeout
        let result = tokio::time::timeout(timeout, rx.changed()).await;

        match result {
            Ok(Ok(())) => {
                // State changed, check new state
                match *rx.borrow() {
                    LatchState::Completed => {
                        debug!("IndexLatch: Completed successfully");
                        Ok(())
                    }
                    LatchState::Failed(ref error) => {
                        debug!("IndexLatch: Failed with error: {}", error);
                        Err(LatchError::IndexingFailed(error.clone()))
                    }
                    LatchState::Pending => {
                        // Shouldn't happen since we got a change notification
                        trace!("IndexLatch: Got change notification but still pending");
                        Err(LatchError::Cancelled)
                    }
                }
            }
            Ok(Err(_)) => {
                // Sender dropped
                debug!("IndexLatch: Sender dropped");
                Err(LatchError::Cancelled)
            }
            Err(_) => {
                debug!("IndexLatch: Timeout after {:?}", timeout);
                Err(LatchError::Timeout)
            }
        }
    }

    /// Trigger the latch with successful completion
    pub async fn trigger_success(&self) {
        // Only update if still pending (idempotent)
        if matches!(*self.state_rx.borrow(), LatchState::Pending) {
            let _ = self.state_tx.send(LatchState::Completed);
            debug!("IndexLatch: Triggered success");
        } else {
            trace!("IndexLatch: Already triggered, ignoring success trigger");
        }
    }

    /// Trigger the latch with failure
    pub async fn trigger_failure(&self, error: String) {
        // Only update if still pending (idempotent)
        if matches!(*self.state_rx.borrow(), LatchState::Pending) {
            let _ = self.state_tx.send(LatchState::Failed(error.clone()));
            debug!("IndexLatch: Triggered failure: {}", error);
        } else {
            trace!("IndexLatch: Already triggered, ignoring failure trigger");
        }
    }
}

impl Default for IndexLatch {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for IndexLatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexLatch").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_latch_creation() {
        let latch = IndexLatch::new();
        match *latch.state_rx.borrow() {
            LatchState::Pending => {} // Expected initial state
            _ => panic!("Expected Pending state on creation"),
        }
    }

    #[tokio::test]
    async fn test_trigger_success() {
        let latch = IndexLatch::new();
        latch.trigger_success().await;

        match *latch.state_rx.borrow() {
            LatchState::Completed => {} // Expected after success
            _ => panic!("Expected Completed state after trigger_success"),
        }
    }

    #[tokio::test]
    async fn test_trigger_failure() {
        let latch = IndexLatch::new();
        let error_msg = "test error".to_string();
        latch.trigger_failure(error_msg.clone()).await;

        match &*latch.state_rx.borrow() {
            LatchState::Failed(msg) => assert_eq!(msg, &error_msg),
            _ => panic!("Expected Failed state after trigger_failure"),
        }
    }

    #[tokio::test]
    async fn test_wait_already_completed() {
        let latch = IndexLatch::new();
        latch.trigger_success().await;

        // Wait should return immediately
        let result = latch.wait(Duration::from_millis(100)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_already_failed() {
        let latch = IndexLatch::new();
        let error_msg = "test error".to_string();
        latch.trigger_failure(error_msg.clone()).await;

        // Wait should return error immediately
        let result = latch.wait(Duration::from_millis(100)).await;
        match result {
            Err(LatchError::IndexingFailed(msg)) => assert_eq!(msg, error_msg),
            _ => panic!("Expected IndexingFailed error"),
        }
    }

    #[tokio::test]
    async fn test_multiple_waiters_sequential() {
        let latch = IndexLatch::new();

        // Trigger completion first
        latch.trigger_success().await;

        // Multiple sequential waiters should all succeed immediately
        let result1 = latch.wait(Duration::from_millis(100)).await;
        assert!(result1.is_ok(), "First waiter should succeed");

        let result2 = latch.wait(Duration::from_millis(100)).await;
        assert!(result2.is_ok(), "Second waiter should succeed");

        let result3 = latch.wait(Duration::from_millis(100)).await;
        assert!(result3.is_ok(), "Third waiter should succeed");
    }

    #[tokio::test]
    async fn test_timeout() {
        let latch = IndexLatch::new();

        // Wait with short timeout, should timeout
        let result = latch.wait(Duration::from_millis(50)).await;
        match result {
            Err(LatchError::Timeout) => {} // Expected
            _ => panic!("Expected Timeout error, got: {:?}", result),
        }

        // State should still be pending after timeout
        match *latch.state_rx.borrow() {
            LatchState::Pending => {} // Expected - timeout doesn't change state
            _ => panic!("Expected Pending state after timeout"),
        }
    }

    #[tokio::test]
    async fn test_trigger_then_wait() {
        let latch = IndexLatch::new();

        // Trigger success first
        latch.trigger_success().await;

        // Then wait - should return immediately
        let result = latch.wait(Duration::from_millis(100)).await;
        assert!(
            result.is_ok(),
            "Wait should succeed immediately after trigger"
        );
    }

    #[tokio::test]
    async fn test_multiple_waiters_after_failure() {
        let latch = IndexLatch::new();
        let error_msg = "test failure".to_string();

        // Trigger failure first
        latch.trigger_failure(error_msg.clone()).await;

        // Multiple waiters should all get the same error
        let result1 = latch.wait(Duration::from_millis(100)).await;
        match result1 {
            Err(LatchError::IndexingFailed(msg)) => assert_eq!(msg, error_msg),
            _ => panic!("Expected IndexingFailed error"),
        }

        let result2 = latch.wait(Duration::from_millis(100)).await;
        match result2 {
            Err(LatchError::IndexingFailed(msg)) => assert_eq!(msg, error_msg),
            _ => panic!("Expected IndexingFailed error"),
        }
    }

    #[tokio::test]
    async fn test_double_trigger_ignored() {
        let latch = IndexLatch::new();

        // First trigger should work
        latch.trigger_success().await;
        match *latch.state_rx.borrow() {
            LatchState::Completed => {} // Expected
            _ => panic!("Expected Completed state after first trigger"),
        }

        // Second trigger should be ignored - state should remain Completed
        latch.trigger_failure("error".to_string()).await;
        match *latch.state_rx.borrow() {
            LatchState::Completed => {} // Should remain completed
            _ => panic!("State should remain Completed after second trigger"),
        }
    }
}
