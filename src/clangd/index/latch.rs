//! Event latch for index completion waiting
//!
//! Provides a robust latch pattern for waiting on index completion with single waiter
//! enforcement, custom timeouts, and race condition protection.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, trace, warn};

/// Errors that can occur during latch operations
#[derive(Debug, thiserror::Error)]
pub enum LatchError {
    #[error("Multiple waiters not allowed - only one waiter can wait at a time")]
    MultipleWaiters,

    #[error("Timeout waiting for completion")]
    Timeout,

    #[error("Latch was cancelled")]
    Cancelled,

    #[error("Indexing failed: {0}")]
    IndexingFailed(String),
}

/// Internal state for the indexing latch
#[derive(Debug, Default)]
struct LatchState {
    /// Whether the latch has been triggered
    completed: bool,
    /// Error state if indexing failed
    error: Option<String>,
    /// Whether someone is currently waiting
    has_waiter: bool,
}

/// Event latch for index completion waiting
///
/// Uses a latch pattern with tokio::Notify to handle index completion waiting.
/// Once triggered (either success or failure), the latch stays triggered.
/// Enforces single waiter constraint and provides custom timeout support.
#[derive(Clone)]
pub struct IndexLatch {
    /// Shared state protected by mutex
    state: Arc<Mutex<LatchState>>,
    /// Notify for signaling completion
    notify: Arc<Notify>,
}

impl IndexLatch {
    /// Create a new index latch
    pub fn new() -> Self {
        debug!("Creating new IndexLatch");
        Self {
            state: Arc::new(Mutex::new(LatchState::default())),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Wait for the latch to be triggered with custom timeout
    ///
    /// Returns error if:
    /// - Another waiter is already waiting (single waiter enforcement)
    /// - Timeout is reached
    /// - Indexing failed
    pub async fn wait(&self, timeout: Duration) -> Result<(), LatchError> {
        let mut state = self.state.lock().await;

        // Check if already completed (handles race where completion happens before wait)
        if state.completed {
            debug!("IndexLatch: Already completed, returning immediately");
            return Ok(());
        }

        // Check if indexing failed
        if let Some(error) = &state.error {
            debug!("IndexLatch: Already failed, returning error immediately");
            return Err(LatchError::IndexingFailed(error.clone()));
        }

        // Enforce single waiter constraint
        if state.has_waiter {
            warn!("IndexLatch: Multiple waiters not allowed");
            return Err(LatchError::MultipleWaiters);
        }

        // Mark that we have a waiter
        state.has_waiter = true;
        trace!("IndexLatch: Waiter registered, waiting for completion");
        drop(state); // Release lock before waiting

        // Wait with timeout
        let result = tokio::time::timeout(timeout, self.notify.notified()).await;

        // Clear waiter flag regardless of outcome
        {
            let mut state = self.state.lock().await;
            state.has_waiter = false;
        }

        match result {
            Ok(_) => {
                // Check final state after notification
                let state = self.state.lock().await;
                if state.completed {
                    debug!("IndexLatch: Completed successfully");
                    Ok(())
                } else if let Some(error) = &state.error {
                    debug!("IndexLatch: Failed with error: {}", error);
                    Err(LatchError::IndexingFailed(error.clone()))
                } else {
                    // This shouldn't happen but handle gracefully
                    warn!("IndexLatch: Notified but neither completed nor failed");
                    Err(LatchError::Cancelled)
                }
            }
            Err(_) => {
                debug!("IndexLatch: Timeout after {:?}", timeout);
                Err(LatchError::Timeout)
            }
        }
    }

    /// Wait for the latch with default timeout
    pub async fn wait_default(&self) -> Result<(), LatchError> {
        self.wait(Duration::from_secs(30)).await
    }

    /// Trigger the latch with successful completion
    pub async fn trigger_success(&self) {
        let mut state = self.state.lock().await;
        if !state.completed && state.error.is_none() {
            state.completed = true;
            debug!("IndexLatch: Triggered success");
            self.notify.notify_waiters(); // Use notify_waiters for robustness
        } else {
            trace!("IndexLatch: Already triggered, ignoring success trigger");
        }
    }

    /// Trigger the latch with failure
    pub async fn trigger_failure(&self, error: String) {
        let mut state = self.state.lock().await;
        if !state.completed && state.error.is_none() {
            state.error = Some(error.clone());
            debug!("IndexLatch: Triggered failure: {}", error);
            self.notify.notify_waiters(); // Use notify_waiters for robustness
        } else {
            trace!("IndexLatch: Already triggered, ignoring failure trigger");
        }
    }

    /// Check if the latch is triggered (completed or failed)
    pub async fn is_triggered(&self) -> bool {
        let state = self.state.lock().await;
        state.completed || state.error.is_some()
    }

    /// Check if the latch completed successfully
    pub async fn is_completed(&self) -> bool {
        let state = self.state.lock().await;
        state.completed
    }

    /// Check if the latch has failed
    pub async fn has_failed(&self) -> Option<String> {
        let state = self.state.lock().await;
        state.error.clone()
    }

    /// Reset the latch to initial state
    pub async fn reset(&self) {
        let mut state = self.state.lock().await;
        *state = LatchState::default();
        debug!("IndexLatch: Reset to initial state");
    }

    /// Check if someone is currently waiting
    pub async fn has_waiter(&self) -> bool {
        let state = self.state.lock().await;
        state.has_waiter
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
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn test_latch_creation() {
        let latch = IndexLatch::new();
        assert!(!latch.is_triggered().await);
        assert!(!latch.is_completed().await);
        assert!(latch.has_failed().await.is_none());
        assert!(!latch.has_waiter().await);
    }

    #[tokio::test]
    async fn test_trigger_success() {
        let latch = IndexLatch::new();
        latch.trigger_success().await;

        assert!(latch.is_triggered().await);
        assert!(latch.is_completed().await);
        assert!(latch.has_failed().await.is_none());
    }

    #[tokio::test]
    async fn test_trigger_failure() {
        let latch = IndexLatch::new();
        let error_msg = "test error".to_string();
        latch.trigger_failure(error_msg.clone()).await;

        assert!(latch.is_triggered().await);
        assert!(!latch.is_completed().await);
        assert_eq!(latch.has_failed().await, Some(error_msg));
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
    async fn test_single_waiter_enforcement() {
        let latch = IndexLatch::new();

        // First waiter should succeed
        let latch1 = latch.clone();
        let waiter1 = tokio::spawn(async move { latch1.wait(Duration::from_secs(1)).await });

        // Give first waiter time to register
        sleep(Duration::from_millis(10)).await;

        // Second waiter should fail immediately
        let result = latch.wait(Duration::from_millis(100)).await;
        match result {
            Err(LatchError::MultipleWaiters) => {} // Expected
            _ => panic!("Expected MultipleWaiters error, got: {:?}", result),
        }

        // Trigger success for first waiter
        latch.trigger_success().await;
        let result1 = waiter1.await.unwrap();
        assert!(result1.is_ok());
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

        // Should no longer have waiter after timeout
        assert!(!latch.has_waiter().await);
    }

    #[tokio::test]
    async fn test_concurrent_trigger_and_wait() {
        let latch = IndexLatch::new();

        // Start waiter
        let latch1 = latch.clone();
        let waiter = tokio::spawn(async move { latch1.wait(Duration::from_secs(1)).await });

        // Trigger success after short delay
        let latch2 = latch.clone();
        let trigger = tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            latch2.trigger_success().await;
        });

        // Both should complete successfully
        let (wait_result, _) = tokio::join!(waiter, trigger);
        assert!(wait_result.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_reset() {
        let latch = IndexLatch::new();
        latch.trigger_success().await;

        assert!(latch.is_completed().await);

        latch.reset().await;

        assert!(!latch.is_triggered().await);
        assert!(!latch.is_completed().await);
        assert!(latch.has_failed().await.is_none());
        assert!(!latch.has_waiter().await);
    }

    #[tokio::test]
    async fn test_race_condition_pre_completion() {
        // Test the race where completion happens before waiter registers
        let latch = IndexLatch::new();

        // Trigger completion first
        latch.trigger_success().await;

        // Then try to wait - should return immediately
        let result = latch.wait(Duration::from_millis(100)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_double_trigger_ignored() {
        let latch = IndexLatch::new();

        // First trigger should work
        latch.trigger_success().await;
        assert!(latch.is_completed().await);

        // Second trigger should be ignored
        latch.trigger_failure("error".to_string()).await;
        assert!(latch.is_completed().await);
        assert!(latch.has_failed().await.is_none());
    }
}
