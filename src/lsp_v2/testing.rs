//! Testing utilities and mock implementations
//!
//! Provides mock implementations of all traits for comprehensive
//! testing of LSP client functionality.

use std::collections::HashMap;

use crate::lsp_v2::client::LspError;
use crate::lsp_v2::traits::LspClientTrait;

// Re-export MockTransport from transport module for convenience
#[allow(unused_imports)]
pub use crate::io::transport::MockTransport;

// Re-export MockProcessManager from process module for convenience
#[cfg(test)]
#[allow(unused_imports)]
pub use crate::io::process::MockProcessManager;

// ============================================================================
// Mock LSP Client
// ============================================================================

/// Mock LSP client for testing
#[allow(dead_code)]
#[derive(Debug)]
pub struct MockLspClient {
    /// Whether the client is initialized
    initialized: bool,
    /// Track open documents by URI
    open_documents: HashMap<String, MockDocument>,
}

/// Mock document state
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct MockDocument {
    language_id: String,
    version: i32,
    text: String,
}

impl MockLspClient {
    /// Create a new mock LSP client
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            initialized: false,
            open_documents: HashMap::new(),
        }
    }

    /// Set the client as initialized
    #[allow(dead_code)]
    pub fn set_initialized(&mut self, initialized: bool) {
        self.initialized = initialized;
    }

    /// Get the list of open document URIs
    #[allow(dead_code)]
    pub fn open_document_uris(&self) -> Vec<&String> {
        self.open_documents.keys().collect()
    }

    /// Check if a document is open
    #[allow(dead_code)]
    pub fn is_document_open(&self, uri: &str) -> bool {
        self.open_documents.contains_key(uri)
    }
}

impl Default for MockLspClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LspClientTrait for MockLspClient {
    fn is_initialized(&self) -> bool {
        self.initialized
    }

    async fn open_text_document(
        &mut self,
        uri: String,
        language_id: String,
        version: i32,
        text: String,
    ) -> Result<(), LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let doc = MockDocument {
            language_id,
            version,
            text,
        };
        self.open_documents.insert(uri, doc);
        Ok(())
    }

    async fn close_text_document(&mut self, uri: String) -> Result<(), LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        self.open_documents.remove(&uri);
        Ok(())
    }

    async fn change_text_document(
        &mut self,
        uri: String,
        version: i32,
        text: String,
    ) -> Result<(), LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        if let Some(doc) = self.open_documents.get_mut(&uri) {
            doc.version = version;
            doc.text = text;
            Ok(())
        } else {
            Err(LspError::Protocol(format!("Document not open: {}", uri)))
        }
    }

    // ========================================================================
    // Core State Methods
    // ========================================================================

    async fn is_connected(&self) -> bool {
        // Mock is always "connected"
        true
    }

    fn server_capabilities(&self) -> Option<&lsp_types::ServerCapabilities> {
        // Mock client doesn't track server capabilities
        None
    }

    // ========================================================================
    // Lifecycle Management
    // ========================================================================

    async fn initialize(
        &mut self,
        _root_uri: Option<String>,
    ) -> Result<lsp_types::InitializeResult, LspError> {
        self.initialized = true;
        Ok(lsp_types::InitializeResult {
            capabilities: lsp_types::ServerCapabilities::default(),
            server_info: None,
        })
    }

    async fn shutdown(&mut self) -> Result<(), LspError> {
        self.initialized = false;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), LspError> {
        self.initialized = false;
        self.open_documents.clear();
        Ok(())
    }

    // ========================================================================
    // Handler Registration
    // ========================================================================

    async fn register_notification_handler<F>(&self, _handler: F)
    where
        F: Fn(crate::lsp_v2::protocol::JsonRpcNotification) + Send + Sync + 'static,
    {
        // Mock implementation - just ignore handlers
    }

    async fn register_request_handler<F>(&self, _handler: F)
    where
        F: Fn(crate::lsp_v2::protocol::JsonRpcRequest) -> crate::lsp_v2::protocol::JsonRpcResponse
            + Send
            + Sync
            + 'static,
    {
        // Mock implementation - just ignore handlers
    }
}
