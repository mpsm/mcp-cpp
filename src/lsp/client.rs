//! LSP client implementation
//!
//! Provides LSP client functionality accessed through the LspClientTrait.
//! All LSP operations are implemented in the trait to avoid method duplication.

use crate::io::transport::Transport;
use crate::lsp::protocol::{
    JsonRpcClient, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
};
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, HoverParams, InitializeParams, InitializedParams, Location, Position,
    ReferenceContext, ReferenceParams, TextDocumentClientCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, VersionedTextDocumentIdentifier, WorkspaceClientCapabilities,
    WorkspaceSymbol, WorkspaceSymbolParams,
};
use tracing::{debug, info};

// ============================================================================
// LSP Client Errors
// ============================================================================

/// LSP client errors
#[derive(Debug, thiserror::Error)]

pub enum LspError {
    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] JsonRpcError),

    #[error("LSP client not initialized")]
    NotInitialized,

    #[error("LSP protocol error: {0}")]
    Protocol(String),

    #[error(
        "LSP request timeout: {method} - consider using a longer timeout or checking server responsiveness"
    )]
    RequestTimeout { method: String },
}

// ============================================================================
// LSP Client Structure
// ============================================================================

/// LSP client structure - functionality accessed through LspClientTrait
pub struct LspClient<T: Transport> {
    /// JSON-RPC client for communication
    rpc_client: JsonRpcClient<T>,

    /// Initialization state
    initialized: bool,

    /// Server capabilities from initialization
    server_capabilities: Option<lsp_types::ServerCapabilities>,
}

impl<T: Transport + 'static> LspClient<T> {
    /// Create a new LSP client with a transport
    pub fn new(transport: T) -> Self {
        Self {
            rpc_client: JsonRpcClient::new(transport),
            initialized: false,
            server_capabilities: None,
        }
    }
    /// Executes typed LSP requests using the lsp-types Request trait.
    /// Provides compile-time method validation and eliminates hardcoded strings,
    /// reducing protocol violation risks and improving maintainability.
    async fn request<R>(&mut self, params: R::Params) -> Result<R::Result, LspError>
    where
        R: lsp_types::request::Request,
        R::Params: serde::Serialize,
        R::Result: serde::de::DeserializeOwned,
    {
        match self.rpc_client.request(R::METHOD, Some(params)).await {
            Ok(result) => Ok(result),
            Err(JsonRpcError::Timeout) => Err(LspError::RequestTimeout {
                method: R::METHOD.to_string(),
            }),
            Err(e) => Err(LspError::JsonRpc(e)),
        }
    }

    /// Sends typed LSP notifications using the lsp-types Notification trait.
    /// Provides compile-time method validation and eliminates hardcoded strings,
    /// ensuring LSP specification compliance and reducing communication errors.
    async fn notify<N>(&mut self, params: N::Params) -> Result<(), LspError>
    where
        N: lsp_types::notification::Notification,
        N::Params: serde::Serialize,
    {
        self.rpc_client
            .notify(N::METHOD, Some(params))
            .await
            .map_err(LspError::JsonRpc)
    }
}

// ============================================================================
// LspClientTrait Implementation
// ============================================================================

use crate::lsp::traits::LspClientTrait;

#[async_trait::async_trait]
impl<T: Transport + 'static> LspClientTrait for LspClient<T> {
    fn is_initialized(&self) -> bool {
        self.initialized
    }

    // ========================================================================
    // Lifecycle Management
    // ========================================================================

    async fn initialize(
        &mut self,
        root_uri: Option<String>,
    ) -> Result<lsp_types::InitializeResult, LspError> {
        if self.initialized {
            return Err(LspError::Protocol("Client already initialized".to_string()));
        }

        info!("Initializing LSP client");

        // Build LSP initialize request
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            #[allow(deprecated)]
            root_path: None, // Deprecated
            #[allow(deprecated)]
            root_uri: root_uri.map(|uri| uri.parse().unwrap()),
            initialization_options: None,
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            capabilities: ClientCapabilities {
                workspace: Some(WorkspaceClientCapabilities {
                    workspace_folders: Some(true),
                    ..Default::default()
                }),
                text_document: Some(TextDocumentClientCapabilities {
                    hover: Some(lsp_types::HoverClientCapabilities {
                        dynamic_registration: Some(false),
                        content_format: Some(vec![lsp_types::MarkupKind::Markdown]),
                    }),
                    definition: Some(lsp_types::GotoCapability {
                        dynamic_registration: Some(false),
                        link_support: Some(false),
                    }),
                    references: Some(lsp_types::ReferenceClientCapabilities {
                        dynamic_registration: Some(false),
                    }),
                    document_symbol: Some(lsp_types::DocumentSymbolClientCapabilities {
                        dynamic_registration: Some(false),
                        symbol_kind: None,
                        hierarchical_document_symbol_support: Some(true),
                        tag_support: None,
                    }),
                    ..Default::default()
                }),
                window: Some(
                    serde_json::from_value(serde_json::json!({
                        "workDoneProgress": true
                    }))
                    .unwrap(),
                ),
                general: None,
                experimental: None,
                notebook_document: None,
            },
            trace: Some(lsp_types::TraceValue::Verbose),
            workspace_folders: None,
            client_info: Some(lsp_types::ClientInfo {
                name: "mcp-cpp-lsp-client".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            locale: None,
        };

        // Send initialize request
        let result = self
            .request::<lsp_types::request::Initialize>(params)
            .await?;

        debug!("LSP server capabilities: {:?}", result.capabilities);
        self.server_capabilities = Some(result.capabilities.clone());

        // Complete initialization
        let initialized_params = InitializedParams {};
        self.notify::<lsp_types::notification::Initialized>(initialized_params)
            .await?;

        self.initialized = true;
        info!("LSP client initialized successfully");

        Ok(result)
    }

    async fn shutdown(&mut self) -> Result<(), LspError> {
        if !self.initialized {
            return Ok(());
        }

        info!("Shutting down LSP client");
        let _: () = self.request::<lsp_types::request::Shutdown>(()).await?;

        self.notify::<lsp_types::notification::Exit>(()).await?;

        self.initialized = false;
        info!("LSP client shutdown complete");

        Ok(())
    }

    async fn close(&mut self) -> Result<(), LspError> {
        if self.initialized {
            self.shutdown().await?;
        }
        self.rpc_client.close().await?;
        Ok(())
    }

    async fn register_notification_handler<F>(&self, handler: F)
    where
        F: Fn(JsonRpcNotification) + Send + Sync + 'static,
    {
        self.rpc_client.on_notification(handler).await
    }

    async fn register_request_handler<F>(&self, handler: F)
    where
        F: Fn(JsonRpcRequest) -> JsonRpcResponse + Send + Sync + 'static,
    {
        self.rpc_client.on_request(handler).await
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

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri
                    .parse()
                    .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
                language_id,
                version,
                text,
            },
        };

        debug!("Opening text document: {:?}", params.text_document.uri);
        self.notify::<lsp_types::notification::DidOpenTextDocument>(params)
            .await?;

        Ok(())
    }

    async fn close_text_document(&mut self, uri: String) -> Result<(), LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: uri
                    .parse()
                    .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
            },
        };

        debug!("Closing text document: {:?}", params.text_document.uri);
        self.notify::<lsp_types::notification::DidCloseTextDocument>(params)
            .await?;

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

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri
                    .parse()
                    .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text,
            }],
        };

        debug!(
            "Changing text document: {:?} (version {})",
            params.text_document.uri, params.text_document.version
        );
        self.notify::<lsp_types::notification::DidChangeTextDocument>(params)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Symbol and Navigation Methods
    // ========================================================================

    async fn workspace_symbols(&mut self, query: String) -> Result<Vec<WorkspaceSymbol>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = WorkspaceSymbolParams {
            query,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        debug!(
            "Requesting workspace symbols with query: {:?}",
            params.query
        );
        let result = self
            .request::<lsp_types::request::WorkspaceSymbolRequest>(params)
            .await?;

        match result {
            Some(lsp_types::WorkspaceSymbolResponse::Flat(symbol_infos)) => {
                // Convert SymbolInformation to WorkspaceSymbol
                let workspace_symbols = symbol_infos
                    .into_iter()
                    .map(|si| lsp_types::WorkspaceSymbol {
                        name: si.name,
                        kind: si.kind,
                        tags: si.tags,
                        container_name: si.container_name,
                        location: lsp_types::OneOf::Left(si.location),
                        data: None,
                    })
                    .collect();
                Ok(workspace_symbols)
            }
            Some(lsp_types::WorkspaceSymbolResponse::Nested(workspace_symbols)) => {
                Ok(workspace_symbols)
            }
            None => Ok(vec![]),
        }
    }

    async fn text_document_definition(
        &mut self,
        uri: String,
        position: Position,
    ) -> Result<GotoDefinitionResponse, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: uri
                        .parse()
                        .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
                },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        debug!(
            "Requesting definition at {:?}:{:?}",
            params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position
        );
        let result = self
            .request::<lsp_types::request::GotoDefinition>(params)
            .await?;

        Ok(result.unwrap_or(lsp_types::GotoDefinitionResponse::Array(vec![])))
    }

    async fn text_document_declaration(
        &mut self,
        uri: String,
        position: Position,
    ) -> Result<lsp_types::request::GotoDeclarationResponse, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = lsp_types::request::GotoDeclarationParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: uri
                        .parse()
                        .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
                },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        debug!(
            "Requesting declaration at {:?}:{:?}",
            params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position
        );
        let result = self
            .request::<lsp_types::request::GotoDeclaration>(params)
            .await?;

        Ok(result.unwrap_or(lsp_types::request::GotoDeclarationResponse::Array(vec![])))
    }

    async fn text_document_references(
        &mut self,
        uri: String,
        position: Position,
        include_declaration: bool,
    ) -> Result<Vec<Location>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: uri
                        .parse()
                        .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
                },
                position,
            },
            context: ReferenceContext {
                include_declaration,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        debug!(
            "Requesting references at {:?}:{:?} (include_declaration: {})",
            params.text_document_position.text_document.uri,
            params.text_document_position.position,
            include_declaration
        );
        let result = self
            .request::<lsp_types::request::References>(params)
            .await?;

        Ok(result.unwrap_or_default())
    }

    async fn text_document_hover(
        &mut self,
        uri: String,
        position: Position,
    ) -> Result<Option<lsp_types::Hover>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: uri
                        .parse()
                        .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
                },
                position,
            },
            work_done_progress_params: Default::default(),
        };

        debug!(
            "Requesting hover at {:?}:{:?}",
            params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position
        );
        let result = self
            .request::<lsp_types::request::HoverRequest>(params)
            .await?;

        Ok(result)
    }

    async fn text_document_document_symbol(
        &mut self,
        uri: String,
    ) -> Result<DocumentSymbolResponse, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: uri
                    .parse()
                    .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        debug!(
            "Requesting document symbols for: {:?}",
            params.text_document.uri
        );
        let result = self
            .request::<lsp_types::request::DocumentSymbolRequest>(params)
            .await?;

        Ok(result.unwrap_or(lsp_types::DocumentSymbolResponse::Flat(vec![])))
    }

    // ========================================================================
    // Call Hierarchy Methods
    // ========================================================================

    async fn text_document_prepare_call_hierarchy(
        &mut self,
        uri: String,
        position: Position,
    ) -> Result<Vec<CallHierarchyItem>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = CallHierarchyPrepareParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: uri
                        .parse()
                        .map_err(|e| LspError::Protocol(format!("Invalid URI: {}", e)))?,
                },
                position,
            },
            work_done_progress_params: Default::default(),
        };

        debug!(
            "Preparing call hierarchy at {:?}:{:?}",
            params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position
        );
        let result = self
            .request::<lsp_types::request::CallHierarchyPrepare>(params)
            .await?;

        Ok(result.unwrap_or_default())
    }

    async fn call_hierarchy_incoming_calls(
        &mut self,
        item: CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyIncomingCall>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = CallHierarchyIncomingCallsParams {
            item,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        debug!("Requesting incoming calls for: {:?}", params.item.name);
        let result = self
            .request::<lsp_types::request::CallHierarchyIncomingCalls>(params)
            .await?;

        Ok(result.unwrap_or_default())
    }

    async fn call_hierarchy_outgoing_calls(
        &mut self,
        item: CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyOutgoingCall>, LspError> {
        if !self.initialized {
            return Err(LspError::NotInitialized);
        }

        let params = CallHierarchyOutgoingCallsParams {
            item,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        debug!("Requesting outgoing calls for: {:?}", params.item.name);
        let result = self
            .request::<lsp_types::request::CallHierarchyOutgoingCalls>(params)
            .await?;

        Ok(result.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::testing::MockLspClient;
    use lsp_types::{Position, Range, SymbolKind};

    #[tokio::test]
    async fn test_mock_client_workspace_symbols_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let result = client.workspace_symbols("test".to_string()).await;
        assert!(result.is_ok());

        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "MockFunction");
        assert_eq!(symbols[1].name, "MockClass");
    }

    #[tokio::test]
    async fn test_mock_client_workspace_symbols_not_initialized() {
        let mut client = MockLspClient::new();
        // Client not initialized

        let result = client.workspace_symbols("test".to_string()).await;
        assert!(matches!(result, Err(LspError::NotInitialized)));
    }

    #[tokio::test]
    async fn test_mock_client_definition_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let position = Position {
            line: 10,
            character: 5,
        };
        let result = client
            .text_document_definition("file:///test.cpp".to_string(), position)
            .await;

        assert!(result.is_ok());
        match result.unwrap() {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri.to_string(), "file:///mock/definition.cpp");
                assert_eq!(location.range.start.line, 42);
            }
            _ => panic!("Expected scalar location response"),
        }
    }

    #[tokio::test]
    async fn test_mock_client_references_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let position = Position {
            line: 10,
            character: 5,
        };
        let result = client
            .text_document_references("file:///test.cpp".to_string(), position, true)
            .await;

        assert!(result.is_ok());
        let references = result.unwrap();
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].uri.to_string(), "file:///mock/usage1.cpp");
        assert_eq!(references[1].uri.to_string(), "file:///mock/usage2.cpp");
    }

    #[tokio::test]
    async fn test_mock_client_hover_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let position = Position {
            line: 10,
            character: 5,
        };
        let result = client
            .text_document_hover("file:///test.cpp".to_string(), position)
            .await;

        assert!(result.is_ok());
        let hover = result.unwrap();
        assert!(hover.is_some());

        match &hover.unwrap().contents {
            lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(content)) => {
                assert!(content.contains("Mock hover information"));
            }
            _ => panic!("Expected string content in hover"),
        }
    }

    #[tokio::test]
    async fn test_mock_client_document_symbols_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let result = client
            .text_document_document_symbol("file:///test.cpp".to_string())
            .await;

        assert!(result.is_ok());
        match result.unwrap() {
            DocumentSymbolResponse::Flat(symbols) => {
                assert_eq!(symbols.len(), 1);
                assert_eq!(symbols[0].name, "MockSymbol");
                assert_eq!(symbols[0].kind, SymbolKind::FUNCTION);
            }
            _ => panic!("Expected flat symbol response"),
        }
    }

    #[tokio::test]
    async fn test_mock_client_prepare_call_hierarchy_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let position = Position {
            line: 10,
            character: 5,
        };
        let result = client
            .text_document_prepare_call_hierarchy("file:///test.cpp".to_string(), position)
            .await;

        assert!(result.is_ok());
        let items = result.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "mockFunction");
        assert_eq!(items[0].kind, SymbolKind::FUNCTION);
    }

    #[tokio::test]
    async fn test_mock_client_incoming_calls_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let item = CallHierarchyItem {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: "file:///test.cpp".parse().unwrap(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            },
            data: None,
        };

        let result = client.call_hierarchy_incoming_calls(item).await;
        assert!(result.is_ok());

        let calls = result.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].from.name, "callerFunction");
    }

    #[tokio::test]
    async fn test_mock_client_outgoing_calls_success() {
        let mut client = MockLspClient::new();
        client.set_initialized(true);

        let item = CallHierarchyItem {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: "file:///test.cpp".parse().unwrap(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            },
            data: None,
        };

        let result = client.call_hierarchy_outgoing_calls(item).await;
        assert!(result.is_ok());

        let calls = result.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].to.name, "calleeFunction");
    }

    #[tokio::test]
    async fn test_mock_client_all_methods_require_initialization() {
        let mut client = MockLspClient::new();
        // Client not initialized
        let position = Position {
            line: 0,
            character: 0,
        };

        // Test that all new methods return NotInitialized error
        assert!(matches!(
            client.workspace_symbols("test".to_string()).await,
            Err(LspError::NotInitialized)
        ));

        assert!(matches!(
            client
                .text_document_definition("file:///test.cpp".to_string(), position)
                .await,
            Err(LspError::NotInitialized)
        ));

        assert!(matches!(
            client
                .text_document_declaration("file:///test.cpp".to_string(), position)
                .await,
            Err(LspError::NotInitialized)
        ));

        assert!(matches!(
            client
                .text_document_references("file:///test.cpp".to_string(), position, true)
                .await,
            Err(LspError::NotInitialized)
        ));

        assert!(matches!(
            client
                .text_document_hover("file:///test.cpp".to_string(), position)
                .await,
            Err(LspError::NotInitialized)
        ));

        assert!(matches!(
            client
                .text_document_document_symbol("file:///test.cpp".to_string())
                .await,
            Err(LspError::NotInitialized)
        ));

        assert!(matches!(
            client
                .text_document_prepare_call_hierarchy("file:///test.cpp".to_string(), position)
                .await,
            Err(LspError::NotInitialized)
        ));

        let dummy_item = CallHierarchyItem {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: "file:///test.cpp".parse().unwrap(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 4,
                },
            },
            data: None,
        };

        assert!(matches!(
            client
                .call_hierarchy_incoming_calls(dummy_item.clone())
                .await,
            Err(LspError::NotInitialized)
        ));

        assert!(matches!(
            client.call_hierarchy_outgoing_calls(dummy_item).await,
            Err(LspError::NotInitialized)
        ));
    }
}
