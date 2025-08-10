//! LSP client implementation
//!
//! Provides LSP client functionality accessed through the LspClientTrait.
//! All LSP operations are implemented in the trait to avoid method duplication.

use crate::io::transport::Transport;
use crate::lsp_v2::protocol::{
    JsonRpcClient, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
};
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams,
    TextDocumentClientCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, VersionedTextDocumentIdentifier, WorkspaceClientCapabilities,
};
use serde_json::Value;
use tracing::{debug, info};

// ============================================================================
// LSP Client Errors
// ============================================================================

/// LSP client errors
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum LspError {
    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] JsonRpcError),

    #[error("LSP client not initialized")]
    NotInitialized,

    #[error("Server capability not supported: {0}")]
    UnsupportedCapability(String),

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
#[allow(dead_code)]
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
}

// ============================================================================
// LspClientTrait Implementation
// ============================================================================

use crate::lsp_v2::traits::LspClientTrait;

#[async_trait::async_trait]
impl<T: Transport + 'static> LspClientTrait for LspClient<T> {
    fn is_initialized(&self) -> bool {
        self.initialized
    }

    async fn is_connected(&self) -> bool {
        self.rpc_client.is_connected().await
    }

    fn server_capabilities(&self) -> Option<&lsp_types::ServerCapabilities> {
        self.server_capabilities.as_ref()
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
        let result: InitializeResult =
            match self.rpc_client.request("initialize", Some(params)).await {
                Ok(result) => result,
                Err(JsonRpcError::Timeout) => {
                    return Err(LspError::RequestTimeout {
                        method: "initialize".to_string(),
                    });
                }
                Err(e) => return Err(LspError::JsonRpc(e)),
            };

        debug!("LSP server capabilities: {:?}", result.capabilities);
        self.server_capabilities = Some(result.capabilities.clone());

        // Complete initialization
        let initialized_params = InitializedParams {};
        self.rpc_client
            .notify("initialized", Some(initialized_params))
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
        let _: () = match self.rpc_client.request("shutdown", None::<Value>).await {
            Ok(result) => result,
            Err(JsonRpcError::Timeout) => {
                return Err(LspError::RequestTimeout {
                    method: "shutdown".to_string(),
                });
            }
            Err(e) => return Err(LspError::JsonRpc(e)),
        };

        self.rpc_client.notify("exit", None::<Value>).await?;

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
        self.rpc_client
            .notify("textDocument/didOpen", Some(params))
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
        self.rpc_client
            .notify("textDocument/didClose", Some(params))
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
        self.rpc_client
            .notify("textDocument/didChange", Some(params))
            .await?;

        Ok(())
    }
}
