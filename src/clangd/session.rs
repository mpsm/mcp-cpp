//! Clangd session management
//!
//! Provides ClangdSession trait and implementation for managing clangd process
//! lifecycle with direct integration to lsp components (no orchestrator).

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::clangd::config::ClangdConfig;
use crate::clangd::error::ClangdSessionError;
use crate::clangd::file_manager::ClangdFileManager;
use crate::clangd::index::{IndexMonitor, ProgressHandler};
use crate::clangd::log_monitor::LogMonitor;
use crate::clangd::session_builder::ClangdSessionBuilder;
use crate::io::{ChildProcessManager, ProcessManager, StderrMonitor, StdioTransport, StopMode};
use crate::lsp::{LspClient, traits::LspClientTrait};

/// Type alias for testing sessions with mock dependencies
#[cfg(test)]
type TestSession =
    ClangdSession<crate::io::process::MockProcessManager, crate::lsp::testing::MockLspClientTrait>;

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

/// Clangd session implementation with dependency injection support
pub struct ClangdSession<P = ChildProcessManager, C = LspClient<StdioTransport>>
where
    P: ProcessManager + 'static,
    C: LspClientTrait + 'static,
{
    /// Session configuration
    config: ClangdConfig,

    /// Process manager for clangd (injected dependency)
    process_manager: Box<P>,

    /// LSP client (injected dependency)
    lsp_client: Box<C>,

    /// File manager for tracking open files
    file_manager: ClangdFileManager,

    /// Indexing progress monitor
    index_monitor: IndexMonitor,

    /// Log monitor for stderr parsing
    log_monitor: LogMonitor,

    /// Session start timestamp
    started_at: Instant,

    /// External progress handler (genuinely optional)
    progress_handler: Option<Arc<dyn ProgressHandler>>,
}

impl<P, C> ClangdSession<P, C>
where
    P: ProcessManager + 'static,
    C: LspClientTrait + 'static,
{
    /// Create a new clangd session with injected dependencies (for testing)
    ///
    /// This constructor enables dependency injection of both ProcessManager and LspClient,
    /// making the session fully unit testable without external processes.
    pub fn with_dependencies(
        config: ClangdConfig,
        process_manager: P,
        lsp_client: C,
        file_manager: ClangdFileManager,
        index_monitor: IndexMonitor,
        log_monitor: LogMonitor,
    ) -> Self {
        let started_at = Instant::now();

        Self {
            config,
            process_manager: Box::new(process_manager),
            lsp_client: Box::new(lsp_client),
            file_manager,
            index_monitor,
            log_monitor,
            started_at,
            progress_handler: None,
        }
    }
}

impl ClangdSession {
    /// Create a new clangd session with real dependencies using the builder
    ///
    /// Performs complete initialization: process start, LSP setup, and connection.
    /// If this method succeeds, the session is fully operational.
    pub async fn new(config: ClangdConfig) -> Result<Self, ClangdSessionError> {
        ClangdSessionBuilder::new()
            .with_config(config)
            .build()
            .await
    }
}

impl<P, C> ClangdSession<P, C>
where
    P: ProcessManager + 'static,
    C: LspClientTrait + 'static,
{
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
        self.process_manager
            .stop(StopMode::Graceful)
            .await
            .map_err(|e| {
                ClangdSessionError::unexpected_failure(format!("Process stop failed: {}", e))
            })?;

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

    /// Set a progress handler for indexing events
    pub fn set_progress_handler(&mut self, handler: Arc<dyn ProgressHandler>) {
        self.progress_handler = Some(handler.clone());
        self.log_monitor.set_handler(handler);
    }

    /// Get reference to the log monitor
    pub fn log_monitor(&self) -> &LogMonitor {
        &self.log_monitor
    }

    /// Setup stderr processing for the log monitor
    /// This must be called after session creation to wire stderr to log monitor
    pub fn setup_stderr_monitoring(&mut self)
    where
        P: StderrMonitor,
    {
        let processor = self.log_monitor.create_stderr_processor();

        // Install the stderr processor
        self.process_manager.on_stderr_line(move |line: String| {
            processor(line);
        });

        debug!("LogMonitor stderr processing wired to process manager");
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
            .ensure_file_ready(path, self.lsp_client.as_mut())
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
impl<P, C> Drop for ClangdSession<P, C>
where
    P: ProcessManager + 'static,
    C: LspClientTrait + 'static,
{
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
impl<P, C> ClangdSessionTrait for ClangdSession<P, C>
where
    P: ProcessManager + 'static,
    C: LspClientTrait + 'static,
{
    type Error = ClangdSessionError;
    type Client = C;

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

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    #[tokio::test]
    async fn test_session_construction_failure() {
        use crate::clangd::testing::test_helpers::*;

        // Test constructor failure with invalid clangd path
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config =
            create_test_config(&project_root, &build_dir, TestConfigType::Failing).unwrap();

        // Constructor should fail due to invalid clangd path
        let result = ClangdSession::new(config).await;
        assert!(result.is_err());
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_session_ready_when_constructed() {
        use crate::clangd::testing::test_helpers::*;

        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config =
            create_test_config(&project_root, &build_dir, TestConfigType::Integration).unwrap();

        // Constructor should succeed and return ready session
        let session = ClangdSession::new(config).await.unwrap();

        // Session should be immediately ready to use
        assert_eq!(session.working_directory(), &project_root);
        assert_eq!(session.build_directory(), &build_dir);
        assert!(session.uptime().as_nanos() > 0);

        session.close().await.unwrap();
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_session_close() {
        use crate::clangd::testing::test_helpers::*;

        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config =
            create_test_config(&project_root, &build_dir, TestConfigType::Integration).unwrap();
        let session = ClangdSession::new(config).await.unwrap();

        // Close should succeed and consume the session
        let result = session.close().await;
        assert!(result.is_ok());

        // Session is now consumed and cannot be used further
    }

    #[tokio::test]
    async fn test_trait_polymorphism_with_mocks() {
        use crate::clangd::testing::test_helpers::*;

        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let session = create_mock_session(&project_root, &build_dir).unwrap();

        // Verify client access works correctly with mock dependencies
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

        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let mock_session = create_mock_session(&project_root, &build_dir).unwrap();

        // Test polymorphic trait usage with proper cleanup
        let result = use_session_polymorphically(mock_session).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Session ran for"));
    }

    #[tokio::test]
    async fn test_dependency_injection_with_mocks() {
        use crate::clangd::testing::test_helpers::*;

        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config = create_test_config(&project_root, &build_dir, TestConfigType::Mock).unwrap();

        // Create session with dependency injection using helper
        let session = create_session_with_mock_dependencies(config);

        // Verify session is properly configured
        assert_eq!(session.working_directory(), &project_root);
        assert_eq!(session.build_directory(), &build_dir);
        assert!(session.uptime().as_nanos() > 0);

        // Verify client is accessible and initialized (mocked)
        let client = session.client();
        assert!(client.is_initialized());

        // Clean shutdown should work with mocks
        session.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_session_factory_for_testing() {
        use crate::clangd::testing::test_helpers::*;

        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config = create_test_config(&project_root, &build_dir, TestConfigType::Mock).unwrap();

        // Use proper fluent builder API with mock dependencies
        use crate::io::process::MockProcessManager;
        use crate::lsp::testing::MockLspClientTrait;
        use crate::lsp::traits::LspClientTrait;

        let process_manager = MockProcessManager::new();
        let mut lsp_client = MockLspClientTrait::new();

        // Setup expectations for basic mock functionality
        lsp_client.expect_is_initialized().returning(|| true);
        lsp_client
            .expect_shutdown()
            .returning(|| Box::pin(async { Ok(()) }));
        lsp_client
            .expect_close()
            .returning(|| Box::pin(async { Ok(()) }));
        lsp_client
            .expect_open_text_document()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));

        let session = ClangdSessionBuilder::new()
            .with_config(config)
            .with_process_manager(process_manager)
            .with_lsp_client(lsp_client)
            .build()
            .await
            .unwrap();

        // Session should be immediately ready with mock dependencies
        assert_eq!(session.working_directory(), &project_root);
        assert_eq!(session.build_directory(), &build_dir);
        assert!(session.uptime().as_nanos() > 0);

        // Mock client should be pre-initialized
        let client = session.client();
        assert!(client.is_initialized());

        // File operations should work with mocks
        assert_eq!(session.get_open_files_count(), 0);

        session.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_unit_testing_without_external_processes() {
        use crate::clangd::testing::test_helpers::*;

        let (temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config = create_test_config(&project_root, &build_dir, TestConfigType::Mock).unwrap();

        // Demonstrate comprehensive unit testing with mock dependencies using fluent API
        use crate::io::process::MockProcessManager;
        use crate::lsp::testing::MockLspClientTrait;
        use crate::lsp::traits::LspClientTrait;

        let process_manager = MockProcessManager::new();
        let mut lsp_client = MockLspClientTrait::new();

        // Setup expectations for basic mock functionality
        lsp_client.expect_is_initialized().returning(|| true);
        lsp_client
            .expect_shutdown()
            .returning(|| Box::pin(async { Ok(()) }));
        lsp_client
            .expect_close()
            .returning(|| Box::pin(async { Ok(()) }));
        lsp_client
            .expect_open_text_document()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));

        let mut session = ClangdSessionBuilder::new()
            .with_config(config)
            .with_process_manager(process_manager)
            .with_lsp_client(lsp_client)
            .build()
            .await
            .unwrap();

        // Test session behavior with mocked dependencies
        assert!(session.client().is_initialized());
        assert!(!session.process_manager.is_running()); // Mock starts not running

        // File management operations work with mocks
        let fake_file_path = temp_dir.path().join("fake.cpp");
        std::fs::write(&fake_file_path, "// test content").unwrap();

        // File operations work with mock LSP client
        session.ensure_file_ready(&fake_file_path).await.unwrap();
        assert_eq!(session.get_open_files_count(), 1);
        assert!(session.is_file_open(&fake_file_path));

        // Graceful shutdown works with mock dependencies
        session.close().await.unwrap();

        // Test validates isolated unit testing without external dependencies
    }

    #[cfg(all(test, feature = "clangd-integration-tests"))]
    #[tokio::test]
    async fn test_clangd_session_with_real_project() {
        use crate::clangd::testing::test_helpers::*;

        let (test_project, session) = create_integration_test_session().await.unwrap();

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
        use crate::clangd::testing::test_helpers::*;

        let (test_project, mut session) = create_integration_test_session().await.unwrap();

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
    #[cfg(feature = "clangd-integration-tests")]
    async fn test_file_change_detection() {
        use crate::clangd::testing::test_helpers::create_integration_test_session;
        use std::fs;

        let (test_project, mut session) = create_integration_test_session().await.unwrap();

        // Create a test file
        let test_file = test_project.project_root.join("test_change.cpp");
        let initial_content = "// Initial content\nint main() { return 0; }";
        fs::write(&test_file, initial_content).unwrap();

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
        use crate::clangd::testing::test_helpers::*;

        let (test_project, mut session) = create_integration_test_session().await.unwrap();

        let non_existent = test_project.project_root.join("does_not_exist.cpp");

        // Should fail for non-existent file
        let result = session.ensure_file_ready(&non_existent).await;
        assert!(result.is_err());
        assert_eq!(session.get_open_files_count(), 0);

        session.close().await.unwrap();
    }
}
