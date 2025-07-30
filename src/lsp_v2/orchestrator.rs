//! LSP Orchestrator - Coordinates process, transport, and client lifecycle
//!
//! Provides a high-level interface that manages the complete LSP client
//! lifecycle: starting external processes, creating transports, and
//! coordinating with the LSP client.

use crate::lsp_v2::client::{LspClient, LspError};
use crate::lsp_v2::process::{
    ChildProcessManager, ProcessError, ProcessManager, StderrMonitor, StopMode,
};
use crate::lsp_v2::transport::StdioTransport;
use async_trait::async_trait;
use tracing::{debug, info};

// ============================================================================
// Orchestrator Errors
// ============================================================================

/// Orchestrator errors that combine process, transport, and LSP errors
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum OrchestratorError {
    #[error("Process error: {0}")]
    Process(#[from] ProcessError),

    #[error("LSP error: {0}")]
    Lsp(#[from] LspError),

    #[error("Orchestrator not started")]
    NotStarted,

    #[error("Orchestrator already started")]
    AlreadyStarted,
}

// ============================================================================
// LSP Orchestrator Trait
// ============================================================================

/// Trait for orchestrating LSP client lifecycle
#[async_trait]
#[allow(dead_code)]
pub trait LspOrchestrator {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start the complete LSP session: process → transport → client → initialize
    async fn start(&mut self, root_uri: Option<String>) -> Result<(), Self::Error>;

    /// Shutdown the LSP session gracefully: shutdown LSP → stop process
    async fn shutdown(&mut self) -> Result<(), Self::Error>;

    /// Force kill everything: kill process, close transport, drop client
    async fn kill(&mut self) -> Result<(), Self::Error>;

    /// Check if the orchestrator is active (process running and LSP initialized)
    fn is_active(&self) -> bool;

    /// Get reference to the LSP client for making requests
    fn client(&self) -> Option<&LspClient<StdioTransport>>;

    /// Get mutable reference to the LSP client for making requests
    fn client_mut(&mut self) -> Option<&mut LspClient<StdioTransport>>;
}

// ============================================================================
// Simple LSP Orchestrator Implementation
// ============================================================================

/// Simple orchestrator that manages a child process with LSP client
pub struct StandardLspOrchestrator {
    /// Process manager for the external LSP server
    process_manager: ChildProcessManager,

    /// LSP client (created after process starts)
    client: Option<LspClient<StdioTransport>>,

    /// State tracking
    started: bool,
}

#[allow(dead_code)]
impl StandardLspOrchestrator {
    /// Create a new orchestrator for the given command and arguments
    pub fn new(command: String, args: Vec<String>) -> Self {
        Self {
            process_manager: ChildProcessManager::new(command, args),
            client: None,
            started: false,
        }
    }

    /// Install a stderr handler for monitoring LSP server output
    pub fn on_stderr_line<F>(&mut self, handler: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.process_manager.on_stderr_line(handler);
    }
}

#[async_trait]
impl LspOrchestrator for StandardLspOrchestrator {
    type Error = OrchestratorError;

    async fn start(&mut self, root_uri: Option<String>) -> Result<(), Self::Error> {
        if self.started {
            return Err(OrchestratorError::AlreadyStarted);
        }

        info!("Starting LSP orchestrator");

        // Step 1: Start the external process
        debug!("Starting LSP server process");
        self.process_manager.start().await?;

        // Step 2: Create transport from process stdin/stdout
        debug!("Creating stdio transport");
        let transport = self.process_manager.create_stdio_transport()?;

        // Step 3: Create LSP client with transport
        debug!("Creating LSP client");
        let mut client = LspClient::new(transport);

        // Step 4: Start the client's message processing loop
        debug!("Starting LSP client message loop");
        client.start().await?;

        // Step 5: Initialize the LSP connection
        debug!("Initializing LSP connection");
        client.initialize(root_uri).await?;

        // Step 6: Store the client and mark as started
        self.client = Some(client);
        self.started = true;

        info!("LSP orchestrator started successfully");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Self::Error> {
        if !self.started {
            return Ok(());
        }

        info!("Shutting down LSP orchestrator");

        // Step 1: Shutdown LSP client gracefully
        if let Some(client) = &mut self.client {
            debug!("Shutting down LSP client");
            client.shutdown().await?;
            client.close().await?;
        }

        // Step 2: Stop the external process
        debug!("Stopping LSP server process");
        self.process_manager.stop(StopMode::Graceful).await?;

        // Step 3: Clean up state
        self.client = None;
        self.started = false;

        info!("LSP orchestrator shutdown complete");
        Ok(())
    }

    async fn kill(&mut self) -> Result<(), Self::Error> {
        info!("Force killing LSP orchestrator");

        // Step 1: Close client connection immediately
        if let Some(client) = &mut self.client {
            debug!("Force closing LSP client");
            let _ = client.close().await; // Ignore errors on force close
        }

        // Step 2: Kill the external process
        debug!("Force killing LSP server process");
        self.process_manager.stop(StopMode::Force).await?;

        // Step 3: Clean up state
        self.client = None;
        self.started = false;

        info!("LSP orchestrator killed");
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.started
            && self.process_manager.is_running()
            && self.client.as_ref().is_some_and(|c| c.is_initialized())
    }

    fn client(&self) -> Option<&LspClient<StdioTransport>> {
        self.client.as_ref()
    }

    fn client_mut(&mut self) -> Option<&mut LspClient<StdioTransport>> {
        self.client.as_mut()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_creation() {
        let orchestrator =
            StandardLspOrchestrator::new("echo".to_string(), vec!["test".to_string()]);

        assert!(!orchestrator.is_active());
        assert!(orchestrator.client().is_none());
    }
}
