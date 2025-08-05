//! High-level LSP client
//!
//! Provides a typed, high-level API for Language Server Protocol
//! communication using the lsp-types crate for full type safety.

use crate::lsp_v2::protocol::{JsonRpcClient, JsonRpcError};
use crate::lsp_v2::transport::Transport;
use lsp_types::{
    ClientCapabilities, InitializeParams, InitializeResult, InitializedParams,
    TextDocumentClientCapabilities, WorkspaceClientCapabilities,
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
// High-level LSP Client
// ============================================================================

/// High-level LSP client that handles LSP protocol over any transport
#[allow(dead_code)]
pub struct LspClient<T: Transport> {
    /// JSON-RPC client for communication
    rpc_client: JsonRpcClient<T>,

    /// Initialization state
    initialized: bool,

    /// Server capabilities from initialization
    server_capabilities: Option<lsp_types::ServerCapabilities>,
}

#[allow(dead_code)]
impl<T: Transport + 'static> LspClient<T> {
    /// Create a new LSP client with a transport
    pub fn new(transport: T) -> Self {
        Self {
            rpc_client: JsonRpcClient::new(transport),
            initialized: false,
            server_capabilities: None,
        }
    }

    /// Initialize the LSP connection
    pub async fn initialize(
        &mut self,
        root_uri: Option<String>,
    ) -> Result<InitializeResult, LspError> {
        if self.initialized {
            return Err(LspError::Protocol("Client already initialized".to_string()));
        }

        info!("Initializing LSP client");

        // Create initialization parameters
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
                window: None,
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

        // Send initialized notification
        let initialized_params = InitializedParams {};
        self.rpc_client
            .notify("initialized", Some(initialized_params))
            .await?;

        self.initialized = true;
        info!("LSP client initialized successfully");

        Ok(result)
    }

    /// Shutdown the LSP connection
    pub async fn shutdown(&mut self) -> Result<(), LspError> {
        if !self.initialized {
            return Ok(());
        }

        info!("Shutting down LSP client");

        // Send shutdown request
        let _: () = match self.rpc_client.request("shutdown", None::<Value>).await {
            Ok(result) => result,
            Err(JsonRpcError::Timeout) => {
                return Err(LspError::RequestTimeout {
                    method: "shutdown".to_string(),
                });
            }
            Err(e) => return Err(LspError::JsonRpc(e)),
        };

        // Send exit notification
        self.rpc_client.notify("exit", None::<Value>).await?;

        self.initialized = false;
        info!("LSP client shutdown complete");

        Ok(())
    }

    /// Check if the client is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get server capabilities
    pub fn server_capabilities(&self) -> Option<&lsp_types::ServerCapabilities> {
        self.server_capabilities.as_ref()
    }

    /// Check if the connection is active
    pub async fn is_connected(&self) -> bool {
        self.rpc_client.is_connected().await
    }

    /// Close the connection (does not stop external process)
    pub async fn close(&mut self) -> Result<(), LspError> {
        if self.initialized {
            self.shutdown().await?;
        }
        self.rpc_client.close().await?;
        Ok(())
    }

    /// Get reference to the JSON-RPC client for advanced usage
    pub fn rpc_client(&self) -> &JsonRpcClient<T> {
        &self.rpc_client
    }

    /// Get mutable reference to the JSON-RPC client for advanced usage
    pub fn rpc_client_mut(&mut self) -> &mut JsonRpcClient<T> {
        &mut self.rpc_client
    }
}
