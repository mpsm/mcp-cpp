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
#[cfg(any(test, feature = "clangd-integration-tests"))]
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

    /// Test project with automatic cleanup
    pub struct TestProject {
        _temp_dir: TempDir, // Underscore prefix keeps it alive until drop
        pub project_root: PathBuf,
        pub build_dir: PathBuf,
    }

    impl TestProject {
        /// Create a new test project by copying test/test-project
        pub async fn new() -> Result<Self, std::io::Error> {
            // Create temp directory (auto-cleanup on drop)
            let temp_dir = TempDir::new()?;
            let project_root = temp_dir.path().to_path_buf();

            // Copy test/test-project to temp location
            copy_dir_recursively("test/test-project", &project_root)?;

            // Create build directory
            let build_dir = project_root.join("build-debug");
            fs::create_dir(&build_dir)?;

            Ok(TestProject {
                _temp_dir: temp_dir,
                project_root,
                build_dir,
            })
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
                return Err(std::io::Error::other(format!(
                    "cmake failed: {}",
                    String::from_utf8_lossy(&output.stderr)
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
