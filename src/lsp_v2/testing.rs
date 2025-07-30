//! Testing utilities and mock implementations
//!
//! Provides mock implementations of all traits for comprehensive
//! testing of LSP client functionality.

use crate::lsp_v2::process::{ProcessError, ProcessManager};
use crate::lsp_v2::transport::StdioTransport;

// Re-export MockTransport from transport module for convenience
#[allow(unused_imports)]
pub use crate::lsp_v2::transport::MockTransport;

/// Mock process manager for testing
pub struct MockProcessManager {
    running: bool,
    process_id: Option<u32>,
}

impl MockProcessManager {
    /// Create a new mock process manager
    pub fn new() -> Self {
        Self {
            running: false,
            process_id: None,
        }
    }
}

impl Default for MockProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ProcessManager for MockProcessManager {
    type Error = ProcessError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.running {
            return Err(ProcessError::AlreadyStarted);
        }
        self.running = true;
        self.process_id = Some(12345); // Mock PID
        Ok(())
    }

    async fn stop(&mut self, _mode: crate::lsp_v2::process::StopMode) -> Result<(), Self::Error> {
        self.running = false;
        self.process_id = None;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn process_id(&self) -> Option<u32> {
        self.process_id
    }

    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error> {
        // For testing purposes, we can't create a real StdioTransport
        // In practice, tests would use MockTransport directly
        Err(ProcessError::NotStarted)
    }

    fn on_process_exit<H>(&mut self, _handler: H)
    where
        H: crate::lsp_v2::process::ProcessExitHandler + 'static,
    {
        // Mock implementation - no-op for testing
    }
}
