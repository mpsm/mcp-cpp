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
