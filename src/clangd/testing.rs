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
use crate::project::{MetaProject, ProjectComponent, ProjectError};

// ============================================================================
// Mock Session Implementation
// ============================================================================

/// Mock implementation of clangd session for testing
pub struct MockClangdSession {
    config: ClangdConfig,
    mock_client: MockLspClient,
    started_at: Instant,
    stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,
    // Test control flags
    should_fail_close: bool,
}

impl MockClangdSession {
    /// Create a new mock session
    pub fn new(config: ClangdConfig) -> Self {
        Self {
            config,
            mock_client: MockLspClient::new(),
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
    type Client = MockLspClient;

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
// Mock MetaProject for Testing
// ============================================================================

/// Mock MetaProject for testing project detection
pub struct MockMetaProject {
    project_root: PathBuf,
    components: Vec<ProjectComponent>,
}

impl MockMetaProject {
    /// Create a new mock meta project
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

    /// Convert to a real MetaProject
    pub fn into_meta_project(self) -> MetaProject {
        MetaProject::new(self.project_root, self.components, 1)
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
    #[cfg(test)]
    use std::fs;

    /// Create a temporary directory with a mock project structure (for mock tests only)
    ///
    /// Note: This creates a minimal structure for mock testing. For integration tests
    /// with real C++ files, use `crate::test_utils::integration::TestProject` instead.
    #[cfg(test)]
    pub fn create_mock_test_project() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        let build_dir = project_root.join("build");

        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        (temp_dir, project_root, build_dir)
    }

    /// Create a valid test configuration
    pub fn create_test_config(
        project_root: &PathBuf,
        build_dir: &PathBuf,
    ) -> Result<ClangdConfig, crate::clangd::error::ClangdConfigError> {
        ClangdConfigBuilder::new()
            .working_directory(project_root)
            .build_directory(build_dir)
            .clangd_path("mock-clangd")
            .build()
    }

    /// Create a mock session with test configuration
    pub fn create_mock_session(
        project_root: &PathBuf,
        build_dir: &PathBuf,
    ) -> Result<MockClangdSession, crate::clangd::error::ClangdConfigError> {
        let config = create_test_config(project_root, build_dir)?;
        Ok(MockClangdSession::new(config))
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
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
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
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
        let config = create_test_config(&project_root, &build_dir).unwrap();

        // Constructor failure means no session object exists
        let result = MockClangdSession::new_with_failure(config, true).await;
        assert!(result.is_err());

        // No session object exists after constructor failure
    }

    #[tokio::test]
    async fn test_mock_session_close_failure() {
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
        let mut session = create_mock_session(&project_root, &build_dir).unwrap();

        // Configure the session to fail on close
        session.set_close_failure(true);
        let result = session.close().await;
        assert!(result.is_err());

        // Session is still consumed even if close fails
    }

    #[tokio::test]
    async fn test_mock_session_stderr_handling() {
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
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
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
        let config = create_test_config(&project_root, &build_dir).unwrap();

        // Constructor failure means no session object exists
        let result = MockClangdSession::new_with_failure(config, true).await;
        assert!(result.is_err());

        // No cleanup needed - if constructor fails, no resources were allocated
        // No partial states or cleanup concerns
    }

    #[tokio::test]
    async fn test_session_drop_behavior() {
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
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
        let mock_meta = MockMetaProject::new(project_root.clone());

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
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
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
        let (_temp_dir, project_root, build_dir) = create_mock_test_project();
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
