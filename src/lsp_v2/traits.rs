//! LSP traits and abstractions
//!
//! Provides trait abstractions that enable polymorphic usage of LSP components
//! while maintaining type safety and testability.

use async_trait::async_trait;

use crate::lsp_v2::client::LspError;
use crate::lsp_v2::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

// ============================================================================
// LSP Client Trait Abstraction
// ============================================================================

/// Trait abstraction for LSP client functionality
///
/// Enables polymorphic usage of real LspClient and MockLspClient while
/// maintaining type safety and proper error handling.
///
/// This trait provides the complete LSP client interface needed by session
/// management and other components that interact with LSP servers.
#[async_trait]
#[allow(dead_code)] // Methods will be used when session management is fully integrated
pub trait LspClientTrait: Send + Sync {
    // ========================================================================
    // Core State Methods
    // ========================================================================

    /// Check if client is initialized (ready for LSP operations)
    fn is_initialized(&self) -> bool;

    /// Check if the connection is active
    async fn is_connected(&self) -> bool;

    /// Get server capabilities (available after initialization)
    fn server_capabilities(&self) -> Option<&lsp_types::ServerCapabilities>;

    // ========================================================================
    // Lifecycle Management
    // ========================================================================

    /// Initialize the LSP connection
    async fn initialize(
        &mut self,
        root_uri: Option<String>,
    ) -> Result<lsp_types::InitializeResult, LspError>;

    /// Shutdown the LSP connection
    async fn shutdown(&mut self) -> Result<(), LspError>;

    /// Close the connection (does not stop external process)
    async fn close(&mut self) -> Result<(), LspError>;

    // ========================================================================
    // Handler Registration (for bidirectional LSP communication)
    // ========================================================================

    /// Register notification handler for server-to-client notifications
    async fn register_notification_handler<F>(&self, handler: F)
    where
        F: Fn(JsonRpcNotification) + Send + Sync + 'static;

    /// Register request handler for server-to-client requests
    async fn register_request_handler<F>(&self, handler: F)
    where
        F: Fn(JsonRpcRequest) -> JsonRpcResponse + Send + Sync + 'static;

    // ========================================================================
    // Document Synchronization
    // ========================================================================

    /// Open a text document in the language server
    async fn open_text_document(
        &mut self,
        uri: String,
        language_id: String,
        version: i32,
        text: String,
    ) -> Result<(), LspError>;

    /// Close a text document in the language server
    async fn close_text_document(&mut self, uri: String) -> Result<(), LspError>;

    /// Notify the server that a text document has changed
    async fn change_text_document(
        &mut self,
        uri: String,
        version: i32,
        text: String,
    ) -> Result<(), LspError>;
}
