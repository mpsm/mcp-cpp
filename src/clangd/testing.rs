//! Mock implementations for clangd session testing
//!
//! Provides mock implementations of all major components for comprehensive
//! unit testing without external dependencies.

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::clangd::config::ClangdConfig;
use crate::clangd::error::ClangdSessionError;
use crate::clangd::session::ClangdSessionTrait;
use crate::lsp::traits::{LspClientTrait, MockLspClientTrait};
use crate::project::{ProjectComponent, ProjectError, ProjectWorkspace};

// ============================================================================
// Mock Session Implementation
// ============================================================================

/// Mock implementation of clangd session for testing
pub struct MockClangdSession {
    config: ClangdConfig,
    mock_client: MockLspClientTrait,
    started_at: Instant,
    stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,
    // Test control flags
    should_fail_close: bool,
}

impl MockClangdSession {
    /// Create a new mock session
    pub fn new(config: ClangdConfig) -> Self {
        let mut mock_client = MockLspClientTrait::new();

        // Setup default expectations
        mock_client.expect_is_initialized().returning(|| true);
        mock_client
            .expect_shutdown()
            .returning(|| Box::pin(async { Ok(()) }));
        mock_client
            .expect_close()
            .returning(|| Box::pin(async { Ok(()) }));
        mock_client
            .expect_open_text_document()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));

        Self {
            config,
            mock_client,
            started_at: Instant::now(),
            stderr_handler: None,
            should_fail_close: false,
        }
    }

    /// Create a new mock session that can be configured to fail during construction
    pub async fn new_with_failure(
        config: ClangdConfig,
        should_fail: bool,
    ) -> Result<Self, ClangdSessionError> {
        if should_fail {
            Err(ClangdSessionError::startup_failed(
                "Mock constructor failure",
            ))
        } else {
            Ok(Self::new(config))
        }
    }

    /// Configure the session to fail on close (for testing error handling)
    pub fn set_close_failure(&mut self, should_fail: bool) {
        self.should_fail_close = should_fail;
    }

    /// Install a stderr handler for testing
    pub fn set_stderr_handler<F>(&mut self, handler: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.stderr_handler = Some(Arc::new(handler));
    }

    /// Simulate stderr output
    pub fn simulate_stderr(&self, line: impl Into<String>) {
        if let Some(handler) = &self.stderr_handler {
            handler(line.into());
        }
    }
}

#[async_trait]
impl ClangdSessionTrait for MockClangdSession {
    type Error = ClangdSessionError;
    type Client = MockLspClientTrait;

    /// Graceful async cleanup (consumes self)
    async fn close(self) -> Result<(), Self::Error> {
        if self.should_fail_close {
            Err(ClangdSessionError::shutdown_failed("Mock close failure"))
        } else {
            // Mock successful cleanup
            Ok(())
        }
    }

    /// Get LSP client
    ///
    /// Returns reference to the mock LSP client for testing operations.
    /// This enables proper polymorphic usage without panicking.
    fn client(&self) -> &Self::Client {
        &self.mock_client
    }

    /// Get mutable LSP client
    ///
    /// Returns mutable reference to the mock LSP client for operations
    /// that require client state modification during testing.
    fn client_mut(&mut self) -> &mut Self::Client {
        &mut self.mock_client
    }

    /// Get current configuration
    fn config(&self) -> &ClangdConfig {
        &self.config
    }

    /// Get session uptime
    fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
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
// Mock LSP Client (for session testing)
// ============================================================================

/// Mock LSP client for testing
#[derive(Debug)]
pub struct MockLspClient {
    initialized: bool,
}

impl MockLspClient {
    pub fn new() -> Self {
        Self { initialized: true }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

// ============================================================================
// Mock ProjectWorkspace for Testing
// ============================================================================

/// Mock ProjectWorkspace for testing project detection
pub struct MockProjectWorkspace {
    project_root: PathBuf,
    components: Vec<ProjectComponent>,
}

impl MockProjectWorkspace {
    /// Create a new mock project workspace
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            components: Vec::new(),
        }
    }

    /// Add a mock component
    pub fn add_component(
        &mut self,
        build_dir: PathBuf,
        provider_type: &str,
        generator: &str,
        build_type: &str,
    ) -> Result<(), ProjectError> {
        // Create a mock compile_commands.json path
        let compile_commands = build_dir.join("compile_commands.json");

        // Create mock compilation database
        let compilation_database = crate::project::CompilationDatabase::new(compile_commands)
            .map_err(|e| {
                ProjectError::Io(std::io::Error::other(format!(
                    "Failed to create compilation database: {e}"
                )))
            })?;

        let component = ProjectComponent {
            build_dir_path: build_dir,
            source_root_path: self.project_root.clone(),
            compilation_database,
            provider_type: provider_type.to_string(),
            generator: generator.to_string(),
            build_type: build_type.to_string(),
            build_options: std::collections::HashMap::new(),
        };

        self.components.push(component);
        Ok(())
    }

    /// Convert to a real ProjectWorkspace
    pub fn into_project_workspace(self) -> ProjectWorkspace {
        ProjectWorkspace::new(self.project_root, self.components, 1)
    }

    /// Get components for a specific provider
    pub fn get_components_for_provider(&self, provider_type: &str) -> Vec<&ProjectComponent> {
        self.components
            .iter()
            .filter(|c| c.provider_type == provider_type)
            .collect()
    }
}

// ============================================================================
// Test Utilities
// ============================================================================

/// Helper functions for creating test configurations and sessions
pub mod test_helpers {
    use super::*;
    use crate::clangd::config::ClangdConfigBuilder;

    /// Common clangd path constants for test scenarios
    pub struct TestClangdPaths;

    impl TestClangdPaths {
        /// Mock clangd path for unit tests (no real process)
        pub const MOCK: &'static str = "mock-clangd";

        /// Invalid clangd path for failure testing
        pub const INVALID: &'static str = "nonexistent-clangd-binary";
    }

    /// Test configuration type for different testing scenarios
    pub enum TestConfigType<'a> {
        /// Mock clangd for unit tests
        Mock,
        /// Real clangd for integration tests
        #[cfg(feature = "clangd-integration-tests")]
        Integration,
        /// Custom clangd path
        Custom(&'a str),
        /// Invalid clangd path for failure testing
        Failing,
    }

    /// Create a test configuration with specified clangd type
    pub fn create_test_config(
        project_root: &PathBuf,
        build_dir: &PathBuf,
        config_type: TestConfigType,
    ) -> Result<ClangdConfig, crate::clangd::error::ClangdConfigError> {
        let clangd_path = match config_type {
            TestConfigType::Mock => TestClangdPaths::MOCK,
            #[cfg(feature = "clangd-integration-tests")]
            TestConfigType::Integration => {
                #[cfg(test)]
                {
                    &crate::test_utils::get_test_clangd_path()
                }
                #[cfg(not(test))]
                {
                    "/usr/bin/clangd" // fallback for non-test builds
                }
            }
            TestConfigType::Custom(path) => path,
            TestConfigType::Failing => TestClangdPaths::INVALID,
        };

        ClangdConfigBuilder::new()
            .working_directory(project_root)
            .build_directory(build_dir)
            .clangd_path(clangd_path)
            .build()
    }

    /// Create a MockClangdSession for trait-level testing
    pub fn create_mock_session(
        project_root: &PathBuf,
        build_dir: &PathBuf,
    ) -> Result<MockClangdSession, crate::clangd::error::ClangdConfigError> {
        let config = create_test_config(project_root, build_dir, TestConfigType::Mock)?;
        Ok(MockClangdSession::new(config))
    }

    /// Create a real ClangdSession with mock dependencies for unit testing
    #[cfg(test)]
    pub fn create_session_with_mock_dependencies(
        config: ClangdConfig,
    ) -> super::super::session::ClangdSession<
        crate::io::process::MockProcessManager,
        crate::lsp::testing::MockLspClientTrait,
    > {
        use crate::clangd::file_manager::ClangdFileManager;
        use crate::clangd::index::IndexProgressMonitor;
        use crate::io::process::MockProcessManager;
        use crate::lsp::testing::MockLspClientTrait;

        let mock_process = MockProcessManager::new();
        let mut mock_lsp = MockLspClientTrait::new();

        // Setup expectations for basic mock functionality
        mock_lsp.expect_is_initialized().returning(|| true);
        mock_lsp
            .expect_shutdown()
            .returning(|| Box::pin(async { Ok(()) }));
        mock_lsp
            .expect_close()
            .returning(|| Box::pin(async { Ok(()) }));
        mock_lsp
            .expect_open_text_document()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        let file_manager = ClangdFileManager::new();
        let index_progress_monitor = IndexProgressMonitor::new();
        let log_monitor = crate::clangd::log_monitor::LogMonitor::new();

        super::super::session::ClangdSession::with_dependencies(
            config,
            mock_process,
            mock_lsp,
            file_manager,
            index_progress_monitor,
            log_monitor,
        )
    }

    /// Create a configured TestProject and ClangdSession for integration tests
    #[cfg(all(test, feature = "clangd-integration-tests"))]
    pub async fn create_integration_test_session() -> Result<
        (
            crate::test_utils::integration::TestProject,
            crate::clangd::session::ClangdSession,
        ),
        Box<dyn std::error::Error>,
    > {
        let test_project = crate::test_utils::integration::TestProject::new().await?;
        test_project.cmake_configure().await?;

        let config = create_test_config(
            &test_project.project_root,
            &test_project.build_dir,
            TestConfigType::Integration,
        )?;
        let session = crate::clangd::session::ClangdSession::new(config).await?;

        Ok((test_project, session))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use test_helpers::*;

    #[tokio::test]
    async fn test_mock_session_lifecycle() {
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let session = create_mock_session(&project_root, &build_dir).unwrap();

        // Session is immediately ready when constructed
        assert_eq!(session.working_directory(), &project_root);
        assert_eq!(session.build_directory(), &build_dir);
        assert!(session.uptime().as_nanos() > 0);

        // Graceful cleanup consumes the session
        let result = session.close().await;
        assert!(result.is_ok());

        // Session is now consumed and cannot be used further
    }

    #[tokio::test]
    async fn test_mock_session_construction_failure() {
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config = create_test_config(&project_root, &build_dir, TestConfigType::Mock).unwrap();

        // Constructor failure means no session object exists
        let result = MockClangdSession::new_with_failure(config, true).await;
        assert!(result.is_err());

        // No session object exists after constructor failure
    }

    #[tokio::test]
    async fn test_mock_session_close_failure() {
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let mut session = create_mock_session(&project_root, &build_dir).unwrap();

        // Configure the session to fail on close
        session.set_close_failure(true);
        let result = session.close().await;
        assert!(result.is_err());

        // Session is still consumed even if close fails
    }

    #[tokio::test]
    async fn test_mock_session_stderr_handling() {
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let mut session = create_mock_session(&project_root, &build_dir).unwrap();

        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_lines_clone = Arc::clone(&stderr_lines);

        // Install stderr handler on ready session
        session.set_stderr_handler(move |line| {
            stderr_lines_clone.lock().unwrap().push(line);
        });

        session.simulate_stderr("test error line");

        // Check stderr lines before await to avoid holding lock across await point
        {
            let lines = stderr_lines.lock().unwrap();
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0], "test error line");
        }

        // Clean shutdown
        session.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_resource_cleanup_on_construction_failure() {
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let config = create_test_config(&project_root, &build_dir, TestConfigType::Mock).unwrap();

        // Constructor failure means no session object exists
        let result = MockClangdSession::new_with_failure(config, true).await;
        assert!(result.is_err());

        // No cleanup needed - if constructor fails, no resources were allocated
        // No partial states or cleanup concerns
    }

    #[tokio::test]
    async fn test_session_drop_behavior() {
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let session = create_mock_session(&project_root, &build_dir).unwrap();

        // Session is immediately ready when constructed
        assert_eq!(session.working_directory(), &project_root);
        assert_eq!(session.build_directory(), &build_dir);
        assert!(session.uptime().as_nanos() > 0);

        // Session cleanup happens automatically when dropped
    }

    #[test]
    fn test_mock_meta_project() {
        let project_root = PathBuf::from("/test/project");
        let mock_meta = MockProjectWorkspace::new(project_root.clone());

        // Note: This test requires creating actual files for ProjectComponent validation
        // In a real test, you'd need to create the compile_commands.json file
        // For now, we'll just test the structure
        assert_eq!(mock_meta.project_root, project_root);
        assert_eq!(mock_meta.components.len(), 0);
    }

    #[tokio::test]
    async fn test_trait_design_violation_fix() {
        // This test demonstrates the fix for the critical trait design violation
        // Previously, calling client() on MockClangdSession would panic
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let session = create_mock_session(&project_root, &build_dir).unwrap();

        // This should NOT panic - demonstrates the fix ✅
        let client = session.client();
        assert!(client.is_initialized());

        // This should also NOT panic ✅
        let mut session = create_mock_session(&project_root, &build_dir).unwrap();
        let _client_mut = session.client_mut();

        // Clean shutdown should work
        session.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_polymorphic_session_usage() {
        // Test that we can use sessions polymorphically through the trait
        let (_temp_dir, project_root, build_dir) =
            crate::test_utils::project::create_mock_build_folder();
        let session = create_mock_session(&project_root, &build_dir).unwrap();

        // This function accepts any session implementing ClangdSessionTrait
        async fn use_session_polymorphically<S>(session: S) -> Result<(), S::Error>
        where
            S: ClangdSessionTrait,
        {
            // Should work with both real and mock sessions!
            let _client = session.client();
            session.close().await
        }

        // Should work without type constraints
        use_session_polymorphically(session).await.unwrap();
    }
}
