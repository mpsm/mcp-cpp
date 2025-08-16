//! LSP traits and abstractions
//!
//! Provides trait abstractions that enable polymorphic usage of LSP components
//! while maintaining type safety and testability.

use async_trait::async_trait;

use crate::lsp::client::LspError;
use crate::lsp::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

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
#[cfg_attr(test, mockall::automock)]
// Methods will be used when session management is fully integrated
pub trait LspClientTrait: Send + Sync {
    // ========================================================================
    // Core State Methods
    // ========================================================================

    /// Check if client is initialized (ready for LSP operations)
    fn is_initialized(&self) -> bool;

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

    // ========================================================================
    // Symbol and Navigation Methods
    // ========================================================================

    /// Search for symbols across the entire workspace
    async fn workspace_symbols(
        &mut self,
        query: String,
    ) -> Result<Vec<lsp_types::WorkspaceSymbol>, LspError>;

    /// Get the definition(s) of a symbol at the given position
    #[allow(dead_code)]
    async fn text_document_definition(
        &mut self,
        uri: String,
        position: lsp_types::Position,
    ) -> Result<lsp_types::GotoDefinitionResponse, LspError>;

    /// Get the declaration(s) of a symbol at the given position
    #[allow(dead_code)]
    async fn text_document_declaration(
        &mut self,
        uri: String,
        position: lsp_types::Position,
    ) -> Result<lsp_types::request::GotoDeclarationResponse, LspError>;

    /// Find all references to a symbol at the given position
    #[allow(dead_code)]
    async fn text_document_references(
        &mut self,
        uri: String,
        position: lsp_types::Position,
        include_declaration: bool,
    ) -> Result<Vec<lsp_types::Location>, LspError>;

    /// Get hover information for a symbol at the given position
    #[allow(dead_code)]
    async fn text_document_hover(
        &mut self,
        uri: String,
        position: lsp_types::Position,
    ) -> Result<Option<lsp_types::Hover>, LspError>;

    /// Get all symbols in a text document
    async fn text_document_document_symbol(
        &mut self,
        uri: String,
    ) -> Result<lsp_types::DocumentSymbolResponse, LspError>;

    // ========================================================================
    // Call Hierarchy Methods
    // ========================================================================

    /// Prepare call hierarchy for the symbol at the given position
    #[allow(dead_code)]
    async fn text_document_prepare_call_hierarchy(
        &mut self,
        uri: String,
        position: lsp_types::Position,
    ) -> Result<Vec<lsp_types::CallHierarchyItem>, LspError>;

    /// Get incoming calls for a call hierarchy item
    #[allow(dead_code)]
    async fn call_hierarchy_incoming_calls(
        &mut self,
        item: lsp_types::CallHierarchyItem,
    ) -> Result<Vec<lsp_types::CallHierarchyIncomingCall>, LspError>;

    /// Get outgoing calls for a call hierarchy item
    #[allow(dead_code)]
    async fn call_hierarchy_outgoing_calls(
        &mut self,
        item: lsp_types::CallHierarchyItem,
    ) -> Result<Vec<lsp_types::CallHierarchyOutgoingCall>, LspError>;

    // ========================================================================
    // Type Hierarchy Methods
    // ========================================================================

    /// Prepare type hierarchy for the symbol at the given position
    #[allow(dead_code)]
    async fn text_document_prepare_type_hierarchy(
        &mut self,
        uri: String,
        position: lsp_types::Position,
    ) -> Result<Option<Vec<lsp_types::TypeHierarchyItem>>, LspError>;

    /// Get supertypes (base classes) for a type hierarchy item
    #[allow(dead_code)]
    async fn type_hierarchy_supertypes(
        &mut self,
        item: lsp_types::TypeHierarchyItem,
    ) -> Result<Option<Vec<lsp_types::TypeHierarchyItem>>, LspError>;

    /// Get subtypes (derived classes) for a type hierarchy item
    #[allow(dead_code)]
    async fn type_hierarchy_subtypes(
        &mut self,
        item: lsp_types::TypeHierarchyItem,
    ) -> Result<Option<Vec<lsp_types::TypeHierarchyItem>>, LspError>;
}
