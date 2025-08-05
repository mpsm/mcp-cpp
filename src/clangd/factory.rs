//! Clangd session factory for creating and configuring sessions
//!
//! Provides ClangdSessionFactory trait and implementation for creating
//! clangd sessions with automatic build directory detection using project::MetaProject.

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;

use crate::clangd::config::{ClangdConfig, ClangdConfigBuilder};
use crate::clangd::error::ClangdSessionError;
use crate::clangd::session::{ClangdSession, ClangdSessionTrait};

// ============================================================================
// Clangd Session Factory Trait
// ============================================================================

/// Factory trait for creating configured clangd sessions
#[async_trait]
pub trait ClangdSessionFactoryTrait: Send + Sync {
    type Session: ClangdSessionTrait;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Create a new session with the given configuration
    async fn create_session(&self, config: ClangdConfig) -> Result<Self::Session, Self::Error>;

    /// Create session with provided build directory
    async fn create_session_with_build_dir(
        &self,
        project_root: PathBuf,
        build_directory: PathBuf,
    ) -> Result<Self::Session, Self::Error>;

    /// Validate configuration before session creation
    fn validate_config(&self, config: &ClangdConfig) -> Result<(), Self::Error>;
}

// ============================================================================
// Clangd Session Factory Implementation
// ============================================================================

/// Factory implementation for creating clangd sessions
pub struct ClangdSessionFactory {
    /// Default clangd executable path
    default_clangd_path: String,

    /// Global stderr handler for all created sessions
    stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

impl ClangdSessionFactory {
    /// Create a new session factory with default settings
    pub fn new() -> Self {
        Self {
            default_clangd_path: "clangd".to_string(),
            stderr_handler: None,
        }
    }

    /// Set the default clangd executable path
    pub fn with_clangd_path(mut self, path: impl Into<String>) -> Self {
        self.default_clangd_path = path.into();
        self
    }

    /// Install a stderr handler for all created sessions
    pub fn with_stderr_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.stderr_handler = Some(Arc::new(handler));
        self
    }
}

#[async_trait]
impl ClangdSessionFactoryTrait for ClangdSessionFactory {
    type Session = ClangdSession;
    type Error = ClangdSessionError;

    async fn create_session(&self, config: ClangdConfig) -> Result<Self::Session, Self::Error> {
        self.validate_config(&config)?;
        ClangdSession::new(config).await
    }

    async fn create_session_with_build_dir(
        &self,
        project_root: PathBuf,
        build_directory: PathBuf,
    ) -> Result<Self::Session, Self::Error> {
        debug!(
            "Creating session with build directory: {:?} for project: {:?}",
            build_directory, project_root
        );

        // Build configuration
        let mut builder = ClangdConfigBuilder::new()
            .working_directory(&project_root)
            .clangd_path(&self.default_clangd_path)
            .build_directory(build_directory)
            .root_uri(format!("file://{}", project_root.to_string_lossy()));

        // Add stderr handler if present
        if let Some(handler) = &self.stderr_handler {
            let handler_clone = Arc::clone(handler);
            builder = builder.stderr_handler(move |line| handler_clone(line));
        }

        let config = builder.build()?;

        self.create_session(config).await
    }

    fn validate_config(&self, config: &ClangdConfig) -> Result<(), Self::Error> {
        // Validate working directory exists
        if !config.working_directory.exists() {
            return Err(ClangdSessionError::InvalidWorkingDirectory {
                path: config.working_directory.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Working directory does not exist",
                ),
            });
        }

        if !config.working_directory.is_dir() {
            return Err(ClangdSessionError::InvalidWorkingDirectory {
                path: config.working_directory.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Working directory path is not a directory",
                ),
            });
        }

        // Validate build directory has compile_commands.json
        let compile_commands = config.build_directory.join("compile_commands.json");
        if !compile_commands.exists() {
            return Err(ClangdSessionError::MissingCompileCommands {
                build_dir: config.build_directory.clone(),
            });
        }

        // Validate clangd executable (basic check)
        if config.clangd_path.is_empty() {
            return Err(ClangdSessionError::InvalidClangdExecutable {
                clangd_path: config.clangd_path.clone(),
            });
        }

        Ok(())
    }
}

impl Default for ClangdSessionFactory {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    // Helper to create a minimal test project (for non-integration tests)
    fn create_minimal_test_project() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        let build_dir = project_root.join("build");

        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        // Create a CMakeCache.txt to make it look like a CMake project
        fs::write(
            build_dir.join("CMakeCache.txt"),
            "CMAKE_BUILD_TYPE:STRING=Debug\n",
        )
        .unwrap();

        (temp_dir, project_root, build_dir)
    }

    #[test]
    fn test_factory_builder() {
        let factory = ClangdSessionFactory::new()
            .with_clangd_path("/usr/bin/clangd")
            .with_stderr_handler(|line| println!("stderr: {line}"));

        assert_eq!(factory.default_clangd_path, "/usr/bin/clangd");
        assert!(factory.stderr_handler.is_some());
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_create_session_with_config() {
        let (_temp_dir, project_root, build_dir) = create_minimal_test_project();

        let config = ClangdConfigBuilder::new()
            .working_directory(&project_root)
            .build_directory(&build_dir)
            .clangd_path(crate::test_utils::get_test_clangd_path()) // Use configured clangd path
            .build()
            .unwrap();

        let factory = ClangdSessionFactory::new();
        let session = factory.create_session(config).await.unwrap();

        assert_eq!(session.working_directory(), &project_root);
        assert_eq!(session.build_directory(), &build_dir);
        // Sessions are always ready when constructed
        assert!(session.uptime().as_nanos() > 0);

        // Clean shutdown
        session.close().await.unwrap();
    }

    #[test]
    fn test_validate_config_missing_working_directory() {
        // Create a temporary directory, build config, then delete the working directory
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .build()
            .unwrap();

        // Now delete the working directory to make validation fail
        drop(temp_dir); // This removes the temp directory

        let factory = ClangdSessionFactory::new();
        let result = factory.validate_config(&config);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ClangdSessionError::InvalidWorkingDirectory { .. }
        ));
    }

    #[test]
    fn test_validate_config_missing_compile_commands() {
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .build()
            .unwrap();

        // Remove the compile_commands.json after building the config
        fs::remove_file(build_dir.join("compile_commands.json")).unwrap();

        let factory = ClangdSessionFactory::new();
        let result = factory.validate_config(&config);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ClangdSessionError::MissingCompileCommands { .. }
        ));
    }

    #[test]
    fn test_validate_config_success() {
        let (_temp_dir, project_root, build_dir) = create_minimal_test_project();

        let config = ClangdConfigBuilder::new()
            .working_directory(&project_root)
            .build_directory(&build_dir)
            .build()
            .unwrap();

        let factory = ClangdSessionFactory::new();
        let result = factory.validate_config(&config);

        assert!(result.is_ok());
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_real_clangd_session_lifecycle() {
        use std::process::Command;

        // Check if clangd is available
        let clangd_path = crate::test_utils::get_test_clangd_path();
        let clangd_check = Command::new(&clangd_path).arg("--version").output();

        if clangd_check.is_err() {
            println!("Skipping clangd integration test: {clangd_path} binary not found");
            return;
        }

        use crate::test_utils::integration::TestProject;
        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        let project_root = &test_project.project_root;
        let build_dir = &test_project.build_dir;

        let config = ClangdConfigBuilder::new()
            .working_directory(project_root)
            .build_directory(build_dir)
            .clangd_path(&clangd_path)
            .build()
            .unwrap();

        let factory = ClangdSessionFactory::new();
        let session = factory.create_session(config).await.unwrap();

        assert_eq!(session.working_directory(), project_root);
        assert_eq!(session.build_directory(), build_dir);
        assert!(session.uptime().as_nanos() > 0);

        session.close().await.unwrap();
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_factory_with_real_test_project() {
        use crate::test_utils::integration::TestProject;
        use std::process::Command;

        // Check if clangd is available
        let clangd_path = crate::test_utils::get_test_clangd_path();
        let clangd_check = Command::new(&clangd_path).arg("--version").output();

        if clangd_check.is_err() {
            println!("Skipping clangd integration test: {clangd_path} binary not found");
            return;
        }

        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        let factory = ClangdSessionFactory::new();
        let session = factory
            .create_session_with_build_dir(
                test_project.project_root.clone(),
                test_project.build_dir.clone(),
            )
            .await
            .unwrap();

        assert_eq!(session.working_directory(), &test_project.project_root);
        assert_eq!(session.build_directory(), &test_project.build_dir);
        assert!(session.uptime().as_nanos() > 0);

        let file_path = test_project.project_root.join("include/Math.hpp");
        assert!(file_path.exists(), "Test file should exist");

        assert!(session.client().is_initialized());

        session.close().await.unwrap();
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_factory_multiple_sessions() {
        use crate::test_utils::integration::TestProject;
        use std::process::Command;

        // Check if clangd is available
        let clangd_path = crate::test_utils::get_test_clangd_path();
        let clangd_check = Command::new(&clangd_path).arg("--version").output();

        if clangd_check.is_err() {
            println!("Skipping clangd integration test: {clangd_path} binary not found");
            return;
        }

        // Create two separate test projects
        let test_project1 = TestProject::new().await.unwrap();
        test_project1.cmake_configure().await.unwrap();

        let test_project2 = TestProject::new().await.unwrap();
        test_project2.cmake_configure().await.unwrap();

        let factory = ClangdSessionFactory::new();

        let session1 = factory
            .create_session_with_build_dir(
                test_project1.project_root.clone(),
                test_project1.build_dir.clone(),
            )
            .await
            .unwrap();

        let session2 = factory
            .create_session_with_build_dir(
                test_project2.project_root.clone(),
                test_project2.build_dir.clone(),
            )
            .await
            .unwrap();

        assert_ne!(session1.working_directory(), session2.working_directory());
        assert_ne!(session1.build_directory(), session2.build_directory());

        assert!(session1.uptime().as_nanos() > 0);
        assert!(session2.uptime().as_nanos() > 0);

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }
}
