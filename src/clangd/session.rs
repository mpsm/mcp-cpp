//! Clangd session management
//!
//! Provides ClangdSession trait and implementation for managing clangd process
//! lifecycle with direct integration to lsp_v2 components (no orchestrator).

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::clangd::config::ClangdConfig;
use crate::clangd::error::ClangdSessionError;
use crate::clangd::file_manager::ClangdFileManager;
use crate::clangd::index::IndexMonitor;
use crate::io::{ChildProcessManager, ProcessManager, StderrMonitor, StdioTransport, StopMode};
use crate::lsp_v2::LspClient;

// ============================================================================
// Clangd Session Trait
// ============================================================================

/// Trait for clangd session management with generic client abstraction
#[async_trait]
pub trait ClangdSessionTrait: Send + Sync {
    /// Associated error type for session operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Associated LSP client type - enables polymorphic client usage
    type Client: Send + Sync;

    /// Graceful async cleanup (consumes self)
    async fn close(self) -> Result<(), Self::Error>;

    /// Get LSP client (always available)
    ///
    /// Returns reference to the underlying LSP client, which can be either
    /// a real LspClient<StdioTransport> or MockLspClient depending on implementation.
    fn client(&self) -> &Self::Client;

    /// Get mutable LSP client (always available)
    ///
    /// Returns mutable reference to the underlying LSP client for operations
    /// that require client state modification.
    fn client_mut(&mut self) -> &mut Self::Client;

    /// Get current configuration
    fn config(&self) -> &ClangdConfig;

    /// Get session uptime
    fn uptime(&self) -> std::time::Duration;

    /// Get session working directory
    fn working_directory(&self) -> &PathBuf;

    /// Get session build directory
    fn build_directory(&self) -> &PathBuf;
}

// ============================================================================
// Clangd Session Implementation
// ============================================================================

/// Clangd session implementation
pub struct ClangdSession {
    /// Session configuration
    config: ClangdConfig,

    /// Process manager for clangd (always running)
    process_manager: ChildProcessManager,

    /// LSP client (always present and initialized)
    lsp_client: LspClient<StdioTransport>,

    /// File manager for tracking open files
    file_manager: ClangdFileManager,

    /// Indexing progress monitor
    index_monitor: IndexMonitor,

    /// Session start timestamp
    started_at: Instant,

    /// Stderr handler for process monitoring (genuinely optional)
    stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

impl ClangdSession {
    /// Create a new clangd session
    ///
    /// Performs complete initialization: process start, LSP setup, and connection.
    /// If this method succeeds, the session is fully operational.
    pub async fn new(config: ClangdConfig) -> Result<Self, ClangdSessionError> {
        info!("Starting clangd session");
        debug!("Working directory: {:?}", config.working_directory);
        debug!("Build directory: {:?}", config.build_directory);
        debug!("Clangd path: {}", config.clangd_path);

        // Step 1: Create and start the clangd process with working directory
        let args = config.get_clangd_args();
        let mut process_manager = ChildProcessManager::new(
            config.clangd_path.clone(),
            args,
            Some(config.working_directory.clone()),
        );

        // Install stderr handler before starting process
        if let Some(handler) = &config.stderr_handler {
            let handler_clone = Arc::clone(handler);
            process_manager.on_stderr_line(move |line| {
                handler_clone(line);
            });
        }

        debug!("Starting clangd process");
        process_manager.start().await?;

        // Step 2: Create stdio transport from process
        debug!("Creating stdio transport");
        let transport = process_manager.create_stdio_transport()?;

        // Step 3: Create LSP client with transport
        debug!("Creating LSP client");
        let mut lsp_client = LspClient::new(transport);

        // Step 4: Initialize the LSP connection
        debug!("Initializing LSP connection");
        let root_uri = config.get_root_uri();

        // Use the configured timeout for initialization
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

        // Step 5: Create and wire IndexMonitor
        debug!("Creating and wiring IndexMonitor");
        let index_monitor = IndexMonitor::new();
        let notification_handler = index_monitor.create_handler();
        lsp_client
            .rpc_client()
            .on_notification(notification_handler)
            .await;

        // Wire request handler for window/workDoneProgress/create
        lsp_client
            .rpc_client()
            .on_request(move |request| {
                use crate::lsp_v2::protocol::{JsonRpcErrorObject, JsonRpcResponse};

                if request.method == "window/workDoneProgress/create" {
                    debug!(
                        "Accepting workDoneProgress/create request: {:?}",
                        request.id
                    );
                    // Accept the progress token by sending success response
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(serde_json::Value::Null),
                        error: None,
                    }
                } else {
                    // Unsupported request - return method not found error
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcErrorObject {
                            code: -32601,
                            message: format!("Method not found: {}", request.method),
                            data: None,
                        }),
                    }
                }
            })
            .await;

        debug!("IndexMonitor and request handler wired successfully");

        let started_at = Instant::now();
        info!("Clangd session started successfully");

        // Step 6: Create file manager
        let file_manager = ClangdFileManager::new();

        // Step 7: Return fully initialized session
        let stderr_handler = config.stderr_handler.clone();
        Ok(Self {
            config,
            process_manager,
            lsp_client,
            file_manager,
            index_monitor,
            started_at,
            stderr_handler,
        })
    }

    /// Graceful async cleanup - consumes self to prevent further use
    ///
    /// Performs orderly shutdown: LSP client shutdown, then process termination.
    /// Prefer this over letting Drop trait handle cleanup.
    pub async fn close(mut self) -> Result<(), ClangdSessionError> {
        info!("Gracefully shutting down clangd session");

        // Step 1: Shutdown LSP client gracefully
        debug!("Shutting down LSP client");
        let shutdown_result = tokio::time::timeout(
            self.config.lsp_config.request_timeout,
            self.lsp_client.shutdown(),
        )
        .await;

        match shutdown_result {
            Ok(Ok(())) => debug!("LSP client shutdown completed"),
            Ok(Err(e)) => warn!("LSP client shutdown error: {}", e),
            Err(_) => warn!("LSP client shutdown timed out"),
        }

        // Always close the client connection
        let _ = self.lsp_client.close().await;

        // Step 2: Stop the clangd process gracefully
        debug!("Stopping clangd process");
        self.process_manager.stop(StopMode::Graceful).await?;

        info!("Clangd session shutdown completed");
        Ok(())
    }

    /// Get session uptime
    pub fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Get reference to the indexing monitor
    pub fn index_monitor(&self) -> &IndexMonitor {
        &self.index_monitor
    }

    /// Ensure a file is ready for use in the language server
    ///
    /// This will open the file if not already open, or send a change notification
    /// if the file has been modified on disk since it was opened.
    pub async fn ensure_file_ready(
        &mut self,
        path: &std::path::Path,
    ) -> Result<(), ClangdSessionError> {
        self.file_manager
            .ensure_file_ready(path, &mut self.lsp_client)
            .await
            .map_err(|e| {
                ClangdSessionError::unexpected_failure(format!("File management failed: {}", e))
            })
    }

    /// Get the number of currently open files
    pub fn get_open_files_count(&self) -> usize {
        self.file_manager.get_open_files_count()
    }

    /// Check if a file is currently open
    pub fn is_file_open(&self, path: &std::path::Path) -> bool {
        self.file_manager.is_file_open(path)
    }
}

/// Drop trait implementation - force cleanup fallback
///
/// This provides a sync fallback if close() wasn't called explicitly.
/// Issues a warning and performs immediate process cleanup.
impl Drop for ClangdSession {
    fn drop(&mut self) {
        // Check if process is still running
        if self.process_manager.is_running() {
            eprintln!(
                "Warning: ClangdSession dropped without calling close() - force killing process"
            );

            // Clean sync kill - no async runtime needed
            self.process_manager.kill_sync();
        }
    }
}

#[async_trait]
impl ClangdSessionTrait for ClangdSession {
    type Error = ClangdSessionError;
    type Client = LspClient<StdioTransport>;

    /// Graceful async cleanup (consumes self)
    async fn close(self) -> Result<(), Self::Error> {
        // Call the close method directly (avoid recursive call)
        ClangdSession::close(self).await
    }

    /// Get LSP client
    fn client(&self) -> &Self::Client {
        &self.lsp_client
    }

    /// Get mutable LSP client
    fn client_mut(&mut self) -> &mut Self::Client {
        &mut self.lsp_client
    }

    /// Get current configuration
    fn config(&self) -> &ClangdConfig {
        &self.config
    }

    /// Get session uptime
    fn uptime(&self) -> std::time::Duration {
        self.uptime()
    }

    /// Get session working directory
    fn working_directory(&self) -> &PathBuf {
        &self.config.working_directory
    }

    /// Get session build directory
    fn build_directory(&self) -> &PathBuf {
        &self.config.build_directory
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clangd::config::ClangdConfigBuilder;
    use tempfile::tempdir;

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    // Sessions are either successfully constructed or construction fails

    #[tokio::test]
    async fn test_session_construction_failure() {
        // Test constructor failure with invalid clangd path
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        std::fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .clangd_path("nonexistent-clangd-binary") // This should cause process start to fail
            .build()
            .unwrap();

        // Constructor should fail due to invalid clangd path
        let result = ClangdSession::new(config).await;
        assert!(result.is_err());
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_session_ready_when_constructed() {
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        std::fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path()) // Use configured clangd path
            .build()
            .unwrap();

        // Constructor should succeed and return ready session
        let session = ClangdSession::new(config).await.unwrap();

        // Session should be immediately ready to use
        assert_eq!(session.working_directory(), temp_dir.path());
        assert_eq!(session.build_directory(), &build_dir);
        assert!(session.uptime().as_nanos() > 0);

        session.close().await.unwrap();
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_session_close() {
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        std::fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path()) // Use configured clangd path
            .build()
            .unwrap();

        let session = ClangdSession::new(config).await.unwrap();

        // Close should succeed and consume the session
        let result = session.close().await;
        assert!(result.is_ok());

        // Session is now consumed and cannot be used further
    }

    #[tokio::test]
    async fn test_trait_polymorphism_with_mocks() {
        use crate::clangd::testing::test_helpers::*;

        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
        let session = create_mock_session(&project_root, &build_dir).unwrap();

        // This should NOT panic - demonstrates the fix
        let client = session.client();
        assert!(client.is_initialized());

        session.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_polymorphic_session_usage() {
        use crate::clangd::testing::test_helpers::*;

        async fn use_session_polymorphically<S>(session: S) -> Result<String, S::Error>
        where
            S: ClangdSessionTrait,
        {
            let uptime = session.uptime();
            session.close().await?;
            Ok(format!("Session ran for {uptime:?}"))
        }

        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
        let mock_session = create_mock_session(&project_root, &build_dir).unwrap();

        // This demonstrates proper polymorphic usage without panics
        let result = use_session_polymorphically(mock_session).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Session ran for"));
    }

    #[cfg(all(test, feature = "clangd-integration-tests"))]
    #[tokio::test]
    async fn test_clangd_session_with_real_project() {
        use crate::test_utils::integration::TestProject;

        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(&test_project.project_root)
            .build_directory(&test_project.build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .build()
            .unwrap();

        let session = ClangdSession::new(config).await.unwrap();

        assert!(session.uptime().as_nanos() > 0);
        assert_eq!(session.working_directory(), &test_project.project_root);
        assert_eq!(session.build_directory(), &test_project.build_dir);

        let client = session.client();
        assert!(client.is_initialized());

        session.close().await.unwrap();
    }

    #[cfg(all(test, feature = "clangd-integration-tests"))]
    #[tokio::test]
    async fn test_clangd_session_file_operations() {
        use crate::test_utils::integration::TestProject;

        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(&test_project.project_root)
            .build_directory(&test_project.build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();

        let file_path = test_project.project_root.join("src/Math.cpp");
        let file_content = std::fs::read_to_string(&file_path).unwrap();
        assert!(!file_content.is_empty(), "Test file should have content");

        // Test file operations
        assert_eq!(session.get_open_files_count(), 0);
        assert!(!session.is_file_open(&file_path));

        // Open the file
        session.ensure_file_ready(&file_path).await.unwrap();
        assert_eq!(session.get_open_files_count(), 1);
        assert!(session.is_file_open(&file_path));

        // Opening again should be a no-op
        session.ensure_file_ready(&file_path).await.unwrap();
        assert_eq!(session.get_open_files_count(), 1);

        session.close().await.unwrap();
    }

    #[cfg(all(test, feature = "clangd-integration-tests"))]
    #[tokio::test]
    async fn test_file_change_detection() {
        use crate::test_utils::integration::TestProject;
        use std::fs;

        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        // Create a test file
        let test_file = test_project.project_root.join("test_change.cpp");
        let initial_content = "// Initial content\nint main() { return 0; }";
        fs::write(&test_file, initial_content).unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(&test_project.project_root)
            .build_directory(&test_project.build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();

        // Open the file initially
        session.ensure_file_ready(&test_file).await.unwrap();
        assert!(session.is_file_open(&test_file));

        // Modify the file on disk
        let new_content = "// Modified content\nint main() { return 42; }";
        fs::write(&test_file, new_content).unwrap();

        // Ensure file ready should detect the change
        session.ensure_file_ready(&test_file).await.unwrap();
        assert!(session.is_file_open(&test_file));
        assert_eq!(session.get_open_files_count(), 1);

        session.close().await.unwrap();
    }

    #[cfg(all(test, feature = "clangd-integration-tests"))]
    #[tokio::test]
    async fn test_non_existent_file_error() {
        use crate::test_utils::integration::TestProject;

        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(&test_project.project_root)
            .build_directory(&test_project.build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path())
            .build()
            .unwrap();

        let mut session = ClangdSession::new(config).await.unwrap();

        let non_existent = test_project.project_root.join("does_not_exist.cpp");

        // Should fail for non-existent file
        let result = session.ensure_file_ready(&non_existent).await;
        assert!(result.is_err());
        assert_eq!(session.get_open_files_count(), 0);

        session.close().await.unwrap();
    }
}
