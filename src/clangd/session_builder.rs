//! Session builder for ClangdSession creation

use lsp_types::request::Request;
use std::marker::PhantomData;
use std::sync::Arc;
use tracing::{debug, info};

use crate::clangd::config::ClangdConfig;
use crate::clangd::error::ClangdSessionError;
use crate::clangd::file_manager::ClangdFileManager;
use crate::clangd::index::IndexMonitor;
use crate::clangd::session::ClangdSession;
use crate::io::{ChildProcessManager, ProcessManager, StderrMonitor, StdioTransport};
use crate::lsp::{LspClient, traits::LspClientTrait};

/// Phantom type markers for builder state
pub struct HasConfig;
pub struct NoConfig;
pub struct HasProcessManager;
pub struct NoProcessManager;
pub struct HasLspClient;
pub struct NoLspClient;

/// Builder for creating ClangdSession instances with optional dependency injection
///
/// Uses phantom types to ensure compile-time safety and eliminate runtime panics.
///
/// # Examples
///
/// Production usage:
/// ```rust
/// let session = ClangdSessionBuilder::new()
///     .with_config(config)
///     .build()
///     .await?;
/// ```
///
/// Testing with mocks:
/// ```rust
/// let session = ClangdSessionBuilder::new()
///     .with_config(config)
///     .with_process_manager(mock_process)
///     .with_lsp_client(mock_client)
///     .build()
///     .await?;
/// ```
pub struct ClangdSessionBuilder<ConfigState = NoConfig, P = NoProcessManager, C = NoLspClient> {
    config: Option<ClangdConfig>,
    process_manager: Option<P>,
    lsp_client: Option<C>,
    _phantom: PhantomData<(ConfigState, P, C)>,
}

impl ClangdSessionBuilder<NoConfig, NoProcessManager, NoLspClient> {
    /// Create a new empty builder
    pub fn new() -> Self {
        Self {
            config: None,
            process_manager: None,
            lsp_client: None,
            _phantom: PhantomData,
        }
    }
}

impl<P, C> ClangdSessionBuilder<NoConfig, P, C> {
    /// Inject configuration
    pub fn with_config(self, config: ClangdConfig) -> ClangdSessionBuilder<HasConfig, P, C> {
        ClangdSessionBuilder {
            config: Some(config),
            process_manager: self.process_manager,
            lsp_client: self.lsp_client,
            _phantom: PhantomData,
        }
    }
}

impl<ConfigState, C> ClangdSessionBuilder<ConfigState, NoProcessManager, C> {
    /// Inject a custom process manager
    pub fn with_process_manager<P>(
        self,
        process_manager: P,
    ) -> ClangdSessionBuilder<ConfigState, P, C>
    where
        P: ProcessManager + 'static,
    {
        ClangdSessionBuilder {
            config: self.config,
            process_manager: Some(process_manager),
            lsp_client: self.lsp_client,
            _phantom: PhantomData,
        }
    }
}

impl<ConfigState, P> ClangdSessionBuilder<ConfigState, P, NoLspClient> {
    /// Inject a custom LSP client  
    pub fn with_lsp_client<C>(self, lsp_client: C) -> ClangdSessionBuilder<ConfigState, P, C>
    where
        C: LspClientTrait + 'static,
    {
        ClangdSessionBuilder {
            config: self.config,
            process_manager: self.process_manager,
            lsp_client: Some(lsp_client),
            _phantom: PhantomData,
        }
    }
}

// Production build (config required, no dependencies injected)
impl ClangdSessionBuilder<HasConfig, NoProcessManager, NoLspClient> {
    /// Build a production session with real dependencies
    pub async fn build(
        self,
    ) -> Result<ClangdSession<ChildProcessManager, LspClient<StdioTransport>>, ClangdSessionError>
    {
        info!("Starting clangd session");

        let config = self.config.unwrap(); // Safe: HasConfig guarantees this
        let mut process_manager = Self::create_process_manager(&config).await?;
        let mut lsp_client =
            Self::create_lsp_client(&config, process_manager.create_stdio_transport()?).await?;
        let index_monitor = Self::setup_monitoring(&mut lsp_client).await;

        Self::finalize_session(config, process_manager, lsp_client, index_monitor)
    }
}

// Testing build (config required, both dependencies injected)
impl<P, C> ClangdSessionBuilder<HasConfig, P, C>
where
    P: ProcessManager + 'static,
    C: LspClientTrait + 'static,
{
    /// Build the session with injected dependencies
    pub async fn build(self) -> Result<ClangdSession<P, C>, ClangdSessionError> {
        let config = self.config.unwrap(); // Safe: HasConfig guarantees this
        let process_manager = self.process_manager.unwrap(); // Safe: P != NoProcessManager guarantees this
        let lsp_client = self.lsp_client.unwrap(); // Safe: C != NoLspClient guarantees this

        let file_manager = ClangdFileManager::new();
        let index_monitor = IndexMonitor::new();

        Ok(ClangdSession::with_dependencies(
            config,
            process_manager,
            lsp_client,
            file_manager,
            index_monitor,
        ))
    }
}

impl ClangdSessionBuilder<HasConfig, NoProcessManager, NoLspClient> {
    /// Create and start the clangd process
    async fn create_process_manager(
        config: &ClangdConfig,
    ) -> Result<ChildProcessManager, ClangdSessionError> {
        debug!("Working directory: {:?}", config.working_directory);
        debug!("Build directory: {:?}", config.build_directory);
        debug!("Clangd path: {}", config.clangd_path);

        let args = config.get_clangd_args();
        let mut process_manager = ChildProcessManager::new(
            config.clangd_path.clone(),
            args,
            Some(config.working_directory.clone()),
        );

        // Install stderr handler if configured
        if let Some(handler) = &config.stderr_handler {
            let handler_clone = Arc::clone(handler);
            process_manager.on_stderr_line(move |line| {
                handler_clone(line);
            });
        }

        debug!("Starting clangd process");
        process_manager.start().await?;
        Ok(process_manager)
    }

    /// Create and initialize the LSP client
    async fn create_lsp_client(
        config: &ClangdConfig,
        transport: StdioTransport,
    ) -> Result<LspClient<StdioTransport>, ClangdSessionError> {
        debug!("Creating LSP client");
        let mut lsp_client = LspClient::new(transport);

        debug!("Initializing LSP connection");
        let root_uri = config.get_root_uri();

        let init_result = tokio::time::timeout(
            config.lsp_config.initialization_timeout,
            lsp_client.initialize(root_uri),
        )
        .await
        .map_err(|_| {
            ClangdSessionError::operation_timeout(
                "LSP initialization",
                config.lsp_config.initialization_timeout,
            )
        })??;

        debug!(
            "LSP initialization completed: {:?}",
            init_result.capabilities
        );
        Ok(lsp_client)
    }

    /// Setup monitoring and request handlers
    async fn setup_monitoring(lsp_client: &mut LspClient<StdioTransport>) -> IndexMonitor {
        debug!("Creating and wiring IndexMonitor");
        let index_monitor = IndexMonitor::new();
        let notification_handler = index_monitor.create_handler();
        lsp_client
            .register_notification_handler(notification_handler)
            .await;

        lsp_client
            .register_request_handler(Self::create_request_handler())
            .await;

        debug!("IndexMonitor and request handler wired successfully");
        index_monitor
    }

    /// Create the standard LSP request handler
    fn create_request_handler()
    -> impl Fn(crate::lsp::protocol::JsonRpcRequest) -> crate::lsp::protocol::JsonRpcResponse
    + Send
    + Sync
    + 'static {
        move |request| {
            use crate::lsp::jsonrpc_utils;

            match request.method.as_str() {
                lsp_types::request::WorkDoneProgressCreate::METHOD => {
                    debug!(
                        "Accepting {} request: {:?}",
                        lsp_types::request::WorkDoneProgressCreate::METHOD,
                        request.id
                    );
                    jsonrpc_utils::null_success_response(request.id)
                }
                _ => jsonrpc_utils::method_not_found_response(request.id, &request.method),
            }
        }
    }

    /// Finalize session creation with all components
    fn finalize_session(
        config: ClangdConfig,
        process_manager: ChildProcessManager,
        lsp_client: LspClient<StdioTransport>,
        index_monitor: IndexMonitor,
    ) -> Result<ClangdSession<ChildProcessManager, LspClient<StdioTransport>>, ClangdSessionError>
    {
        info!("Clangd session started successfully");
        let file_manager = ClangdFileManager::new();

        Ok(ClangdSession::with_dependencies(
            config,
            process_manager,
            lsp_client,
            file_manager,
            index_monitor,
        ))
    }
}
