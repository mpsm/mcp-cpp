//! Testing utilities and mock implementations
//!
//! Provides mock implementations of all traits for comprehensive
//! testing of LSP client functionality.

use std::collections::HashMap;

use crate::lsp::client::LspError;
use crate::lsp::traits::LspClientTrait;
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall,
    DocumentSymbolResponse, GotoDefinitionResponse, Location, Position, Range, SymbolInformation,
    SymbolKind, WorkspaceSymbol,
};

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
        F: Fn(crate::lsp::protocol::JsonRpcNotification) + Send + Sync + 'static,
    {
        // Mock implementation - just ignore handlers
    }

    async fn register_request_handler<F>(&self, _handler: F)
    where
        F: Fn(crate::lsp::protocol::JsonRpcRequest) -> crate::lsp::protocol::JsonRpcResponse
            + Send
            + Sync
            + 'static,
    {
        // Mock implementation - just ignore handlers
    }

    // ========================================================================
    // Symbol and Navigation Methods
    // ========================================================================

    async fn workspace_symbols(
        &mut self,
        _query: String,
    ) -> Result<Vec<WorkspaceSymbol>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock workspace symbols
        Ok(vec![
            WorkspaceSymbol {
                name: "MockFunction".to_string(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                container_name: Some("MockClass".to_string()),
                location: lsp_types::OneOf::Left(Location {
                    uri: "file:///mock/file.cpp".parse().unwrap(),
                    range: Range {
                        start: Position {
                            line: 10,
                            character: 0,
                        },
                        end: Position {
                            line: 15,
                            character: 0,
                        },
                    },
                }),
                data: None,
            },
            WorkspaceSymbol {
                name: "MockClass".to_string(),
                kind: SymbolKind::CLASS,
                tags: None,
                container_name: None,
                location: lsp_types::OneOf::Left(Location {
                    uri: "file:///mock/file.cpp".parse().unwrap(),
                    range: Range {
                        start: Position {
                            line: 5,
                            character: 0,
                        },
                        end: Position {
                            line: 20,
                            character: 0,
                        },
                    },
                }),
                data: None,
            },
        ])
    }

    async fn text_document_definition(
        &mut self,
        _uri: String,
        _position: Position,
    ) -> Result<GotoDefinitionResponse, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock definition location
        Ok(GotoDefinitionResponse::Scalar(Location {
            uri: "file:///mock/definition.cpp".parse().unwrap(),
            range: Range {
                start: Position {
                    line: 42,
                    character: 8,
                },
                end: Position {
                    line: 42,
                    character: 20,
                },
            },
        }))
    }

    async fn text_document_declaration(
        &mut self,
        _uri: String,
        _position: Position,
    ) -> Result<lsp_types::request::GotoDeclarationResponse, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock declaration location
        Ok(lsp_types::request::GotoDeclarationResponse::Scalar(
            Location {
                uri: "file:///mock/declaration.h".parse().unwrap(),
                range: Range {
                    start: Position {
                        line: 15,
                        character: 0,
                    },
                    end: Position {
                        line: 15,
                        character: 12,
                    },
                },
            },
        ))
    }

    async fn text_document_references(
        &mut self,
        _uri: String,
        _position: Position,
        _include_declaration: bool,
    ) -> Result<Vec<Location>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock references
        Ok(vec![
            Location {
                uri: "file:///mock/usage1.cpp".parse().unwrap(),
                range: Range {
                    start: Position {
                        line: 25,
                        character: 4,
                    },
                    end: Position {
                        line: 25,
                        character: 16,
                    },
                },
            },
            Location {
                uri: "file:///mock/usage2.cpp".parse().unwrap(),
                range: Range {
                    start: Position {
                        line: 30,
                        character: 8,
                    },
                    end: Position {
                        line: 30,
                        character: 20,
                    },
                },
            },
        ])
    }

    async fn text_document_hover(
        &mut self,
        _uri: String,
        _position: Position,
    ) -> Result<Option<lsp_types::Hover>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock hover information
        Ok(Some(lsp_types::Hover {
            contents: lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(
                "Mock hover information: int mockFunction(const std::string& param)".to_string(),
            )),
            range: Some(Range {
                start: Position {
                    line: 10,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 12,
                },
            }),
        }))
    }

    async fn text_document_document_symbol(
        &mut self,
        _uri: String,
    ) -> Result<DocumentSymbolResponse, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock document symbols
        Ok(DocumentSymbolResponse::Flat(vec![SymbolInformation {
            name: "MockSymbol".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            #[allow(deprecated)]
            deprecated: None,
            location: Location {
                uri: "file:///mock/file.cpp".parse().unwrap(),
                range: Range {
                    start: Position {
                        line: 5,
                        character: 0,
                    },
                    end: Position {
                        line: 10,
                        character: 0,
                    },
                },
            },
            container_name: Some("MockContainer".to_string()),
        }]))
    }

    // ========================================================================
    // Call Hierarchy Methods
    // ========================================================================

    async fn text_document_prepare_call_hierarchy(
        &mut self,
        _uri: String,
        _position: Position,
    ) -> Result<Vec<CallHierarchyItem>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock call hierarchy items
        Ok(vec![CallHierarchyItem {
            name: "mockFunction".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: Some("Mock function detail".to_string()),
            uri: "file:///mock/file.cpp".parse().unwrap(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 0,
                },
                end: Position {
                    line: 15,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 10,
                    character: 4,
                },
                end: Position {
                    line: 10,
                    character: 16,
                },
            },
            data: None,
        }])
    }

    async fn call_hierarchy_incoming_calls(
        &mut self,
        _item: CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyIncomingCall>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock incoming calls
        Ok(vec![CallHierarchyIncomingCall {
            from: CallHierarchyItem {
                name: "callerFunction".to_string(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                detail: Some("Caller function".to_string()),
                uri: "file:///mock/caller.cpp".parse().unwrap(),
                range: Range {
                    start: Position {
                        line: 20,
                        character: 0,
                    },
                    end: Position {
                        line: 25,
                        character: 0,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: 20,
                        character: 4,
                    },
                    end: Position {
                        line: 20,
                        character: 18,
                    },
                },
                data: None,
            },
            from_ranges: vec![Range {
                start: Position {
                    line: 22,
                    character: 4,
                },
                end: Position {
                    line: 22,
                    character: 16,
                },
            }],
        }])
    }

    async fn call_hierarchy_outgoing_calls(
        &mut self,
        _item: CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyOutgoingCall>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        // Return mock outgoing calls
        Ok(vec![CallHierarchyOutgoingCall {
            to: CallHierarchyItem {
                name: "calleeFunction".to_string(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                detail: Some("Called function".to_string()),
                uri: "file:///mock/callee.cpp".parse().unwrap(),
                range: Range {
                    start: Position {
                        line: 30,
                        character: 0,
                    },
                    end: Position {
                        line: 35,
                        character: 0,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: 30,
                        character: 4,
                    },
                    end: Position {
                        line: 30,
                        character: 18,
                    },
                },
                data: None,
            },
            from_ranges: vec![Range {
                start: Position {
                    line: 12,
                    character: 8,
                },
                end: Position {
                    line: 12,
                    character: 22,
                },
            }],
        }])
    }
}
