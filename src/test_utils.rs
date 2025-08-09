//! Test utilities and global setup
//!
//! Provides centralized test logging configuration and other test helpers.

/// Test logging utilities
#[cfg(all(test, feature = "test-logging"))]
pub mod logging {
    use std::sync::Once;
    use tracing_subscriber::{EnvFilter, fmt};

    static INIT: Once = Once::new();

    /// Initialize test logging globally - safe to call multiple times
    ///
    /// This function sets up a test-friendly logger that:
    /// - Only initializes once per test run (using Once)
    /// - Respects RUST_LOG environment variable with sensible defaults
    /// - Uses test writer to avoid interfering with test output
    /// - Gracefully handles multiple initialization attempts
    ///
    /// # Usage
    ///
    /// For manual initialization in specific tests:
    /// ```rust
    /// #[tokio::test]
    /// async fn my_test() {
    ///     crate::test_utils::logging::init();
    ///     // ... test code ...
    /// }
    /// ```
    ///
    /// For automatic initialization in a test module:
    /// ```rust
    /// #[cfg(test)]
    /// mod tests {
    ///     use super::*;
    ///     
    ///     // Auto-initialize logging for all tests in this module
    ///     #[cfg(feature = "test-logging")]
    ///     #[ctor::ctor]
    ///     fn init_test_logging() {
    ///         crate::test_utils::logging::init();
    ///     }
    ///     
    ///     #[tokio::test]
    ///     async fn my_test() {
    ///         // No manual init needed - logging already set up!
    ///         // ... test code ...
    ///     }
    /// }
    /// ```
    ///
    /// # Environment Variables
    ///
    /// - `RUST_LOG`: Controls log level (default: "debug,tokio=info,hyper=info")
    ///
    /// # Examples
    ///
    /// ```bash
    /// # Run tests with default logging
    /// cargo test --features test-logging
    ///
    /// # Run tests with trace-level logging
    /// RUST_LOG=trace cargo test --features test-logging
    ///
    /// # Run tests with specific module logging
    /// RUST_LOG=mcp_cpp_server::clangd=trace cargo test --features test-logging
    /// ```
    pub fn init() {
        INIT.call_once(|| {
            let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Default filter: debug for our crate, info for noisy dependencies
                EnvFilter::new("debug,tokio=info,hyper=info,h2=info,tower=info")
            });

            fmt()
                .with_env_filter(env_filter)
                .with_test_writer() // Ensures logs don't interfere with test output
                .with_target(true) // Include module paths in logs
                .with_thread_ids(true) // Include thread IDs for async debugging
                .compact() // Use compact format for test readability
                .try_init()
                .ok(); // Ignore errors if already initialized by another test
        });
    }
}

/// Global test logging setup
///
/// This provides a convenient way to set up logging for all tests in the project.
/// Add this to any test module where you want automatic logging initialization.
#[cfg(all(test, feature = "test-logging"))]
#[macro_export]
macro_rules! setup_test_logging {
    () => {
        #[ctor::ctor]
        fn init_test_logging() {
            $crate::test_utils::logging::init();
        }
    };
}

/// Get clangd path for integration tests
///
/// Checks the CLANGD_PATH environment variable and falls back to "clangd" if not set.
/// This allows tests to work both in CI (where CLANGD_PATH=/usr/bin/clangd-20) and
/// local development (where clangd is in PATH).
///
/// # Examples
///
/// ```rust
/// use crate::test_utils::get_test_clangd_path;
///
/// let clangd_path = get_test_clangd_path();
/// // Returns "/usr/bin/clangd-20" if CLANGD_PATH is set, otherwise "clangd"
/// ```
#[cfg(all(test, feature = "clangd-integration-tests"))]
pub fn get_test_clangd_path() -> String {
    std::env::var("CLANGD_PATH").unwrap_or_else(|_| "clangd".to_string())
}

/// Integration test helpers for working with test/test-project
#[cfg(test)]
pub mod integration {
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;
    use walkdir::WalkDir;

    /// Test workspace that can contain multiple test projects
    #[cfg(feature = "project-integration-tests")]
    pub struct TestWorkspace {
        _temp_dir: TempDir, // Underscore prefix keeps it alive until drop
        pub root: PathBuf,
    }

    #[cfg(feature = "project-integration-tests")]
    impl TestWorkspace {
        /// Create a new test workspace
        pub fn new() -> Result<Self, std::io::Error> {
            let temp_dir = TempDir::new()?;
            let root = temp_dir.path().to_path_buf();

            Ok(TestWorkspace {
                _temp_dir: temp_dir,
                root,
            })
        }

        /// Create a CMake test project within this workspace
        pub async fn create_cmake_project(
            &self,
            name: &str,
        ) -> Result<TestProject, std::io::Error> {
            let project_root = self.root.join(name);
            // Ensure parent directories exist if name contains path separators
            if let Some(parent) = project_root.parent() {
                fs::create_dir_all(parent)?;
            }
            TestProject::create_at(
                &project_root,
                "test/test-project",
                "build-debug",
                ProjectType::CMake,
            )
            .await
        }

        /// Create a Meson test project within this workspace
        #[cfg(feature = "project-integration-tests")]
        pub async fn create_meson_project(
            &self,
            name: &str,
        ) -> Result<TestProject, std::io::Error> {
            let project_root = self.root.join(name);
            // Ensure parent directories exist if name contains path separators
            if let Some(parent) = project_root.parent() {
                fs::create_dir_all(parent)?;
            }
            #[cfg(feature = "project-integration-tests")]
            {
                TestProject::create_at(
                    &project_root,
                    "test/test-meson-project",
                    "builddir",
                    ProjectType::Meson,
                )
                .await
            }
        }

        /// Get the workspace root path
        pub fn path(&self) -> &Path {
            &self.root
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum ProjectType {
        CMake,
        #[cfg(feature = "project-integration-tests")]
        Meson,
    }

    /// Test project with automatic cleanup
    pub struct TestProject {
        _temp_dir: Option<TempDir>, // Some if owned, None if part of workspace
        pub project_root: PathBuf,
        pub build_dir: PathBuf,
        #[cfg_attr(not(feature = "project-integration-tests"), allow(dead_code))]
        pub project_type: ProjectType,
    }

    impl TestProject {
        /// Create a new standalone CMake test project
        pub async fn new() -> Result<Self, std::io::Error> {
            // Create temp directory (auto-cleanup on drop)
            let temp_dir = TempDir::new()?;
            let project_root = temp_dir.path().to_path_buf();
            let build_dir =
                Self::init_project(&project_root, "test/test-project", "build-debug").await?;

            Ok(TestProject {
                _temp_dir: Some(temp_dir),
                project_root,
                build_dir,
                project_type: ProjectType::CMake,
            })
        }

        /// Create a test project at a specific path (used by TestWorkspace)
        #[cfg_attr(not(feature = "project-integration-tests"), allow(dead_code))]
        pub(super) async fn create_at(
            project_root: &Path,
            template_path: &str,
            build_dir_name: &str,
            project_type: ProjectType,
        ) -> Result<Self, std::io::Error> {
            let build_dir = Self::init_project(project_root, template_path, build_dir_name).await?;

            Ok(TestProject {
                _temp_dir: None, // Not owned, part of workspace
                project_root: project_root.to_path_buf(),
                build_dir,
                project_type,
            })
        }

        /// Initialize a test project from template
        async fn init_project(
            project_root: &Path,
            template_path: &str,
            build_dir_name: &str,
        ) -> Result<PathBuf, std::io::Error> {
            // Ensure the project root exists
            fs::create_dir_all(project_root)?;

            // Copy template contents to the specified location
            copy_dir_recursively(template_path, project_root)?;

            // Remove any existing build* directories that were copied
            for entry in fs::read_dir(project_root)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir()
                    && let Some(name) = path.file_name()
                {
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("build") {
                        fs::remove_dir_all(&path)?;
                    }
                }
            }

            // Create build directory
            let build_dir = project_root.join(build_dir_name);
            fs::create_dir(&build_dir)?;

            Ok(build_dir)
        }

        /// Configure the project using the appropriate build system
        #[cfg_attr(not(feature = "project-integration-tests"), allow(dead_code))]
        pub async fn configure(&self) -> Result<(), std::io::Error> {
            match self.project_type {
                ProjectType::CMake => self.cmake_configure().await,
                #[cfg(feature = "project-integration-tests")]
                ProjectType::Meson => self.meson_configure().await,
            }
        }

        /// Configure with cmake to generate compile_commands.json
        pub async fn cmake_configure(&self) -> Result<(), std::io::Error> {
            use tokio::process::Command;

            let output = Command::new("cmake")
                .arg("-S")
                .arg(&self.project_root)
                .arg("-B")
                .arg(&self.build_dir)
                .arg("-DCMAKE_BUILD_TYPE=Debug")
                .arg("-DCMAKE_EXPORT_COMPILE_COMMANDS=ON")
                .output()
                .await?;

            if !output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(std::io::Error::other(format!(
                    "cmake failed:\nstdout: {}\nstderr: {}",
                    stdout, stderr
                )));
            }

            Ok(())
        }

        /// Configure with meson to generate compile_commands.json
        #[cfg(feature = "project-integration-tests")]
        pub async fn meson_configure(&self) -> Result<(), std::io::Error> {
            use tokio::process::Command;

            let output = Command::new("meson")
                .arg("setup")
                .arg(&self.build_dir)
                .arg(&self.project_root)
                .output()
                .await?;

            if !output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(std::io::Error::other(format!(
                    "meson failed:\nstdout: {}\nstderr: {}",
                    stdout, stderr
                )));
            }

            Ok(())
        }
    }

    fn copy_dir_recursively(src: &str, dst: &Path) -> Result<(), std::io::Error> {
        let src_path = Path::new(src);
        if !src_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Source directory {} does not exist", src_path.display()),
            ));
        }

        for entry in WalkDir::new(src) {
            let entry = entry?;
            let src_path = entry.path();
            let rel_path = src_path.strip_prefix(src).unwrap();
            let dst_path = dst.join(rel_path);

            if entry.file_type().is_dir() {
                fs::create_dir_all(&dst_path)?;
            } else {
                fs::copy(src_path, &dst_path)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_test_project_creation() {
            let project = TestProject::new().await.unwrap();

            assert!(project.project_root.exists());
            assert!(project.build_dir.exists());
            assert!(project.project_root.join("CMakeLists.txt").exists());
            assert!(project.project_root.join("src").exists());
            assert!(project.project_root.join("include").exists());
        }

        #[tokio::test]
        async fn test_test_project_cmake_configure() {
            let project = TestProject::new().await.unwrap();
            project.cmake_configure().await.unwrap();

            assert!(project.build_dir.join("compile_commands.json").exists());
            assert!(project.build_dir.join("CMakeCache.txt").exists());
        }

        #[test]
        fn test_copy_dir_recursively_missing_source() {
            use tempfile::TempDir;
            let temp_dir = TempDir::new().unwrap();
            let result = copy_dir_recursively("nonexistent/path", temp_dir.path());
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
        }
    }
}
