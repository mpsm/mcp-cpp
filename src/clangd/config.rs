//! Configuration system for clangd sessions
//!
//! Provides ClangdConfig for session configuration with builder pattern,
//! validation, and support for different LSP and resource settings.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::clangd::error::ClangdConfigError;

// ============================================================================
// Configuration Constants
// ============================================================================

/// Default timeout for LSP initialization (30 seconds)
///
/// This allows sufficient time for clangd to start, parse compile_commands.json,
/// and complete initial indexing of small to medium projects.
pub const DEFAULT_INITIALIZATION_TIMEOUT_SECS: u64 = 30;

/// Default timeout for individual LSP requests (10 seconds)
///
/// Most LSP requests should complete quickly, but symbol queries in large
/// codebases may take several seconds.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 10;

/// Maximum allowed initialization timeout (5 minutes)
///
/// Prevents configuration of unreasonably long timeouts that could hang
/// the application. Large projects should complete indexing within this limit.
pub const MAX_INITIALIZATION_TIMEOUT_SECS: u64 = 300;

/// Memory limit conversion factor (MB to result count)
///
/// Rough heuristic to convert memory limit in MB to clangd's --limit-results
/// parameter. This is an approximation based on typical symbol sizes.
pub const MEMORY_TO_RESULTS_FACTOR: u64 = 1000;

/// Default workspace symbol limit for clangd
///
/// Clangd's default workspace symbol limit is 100 results. We increase this to 1000
/// to provide better search coverage for large C++ codebases while maintaining
/// reasonable performance. This is applied via the --limit-results clangd argument.
pub const DEFAULT_WORKSPACE_SYMBOL_LIMIT: u32 = 1000;

// ============================================================================
// Core Configuration Types
// ============================================================================

/// Complete clangd session configuration
#[derive(Clone)]
pub struct ClangdConfig {
    /// Working directory for clangd process
    pub working_directory: PathBuf,

    /// Path to clangd executable
    pub clangd_path: String,

    /// Build directory with compile_commands.json
    pub build_directory: PathBuf,

    /// Additional clangd command-line arguments
    pub extra_args: Vec<String>,

    /// LSP initialization options
    pub lsp_config: LspConfig,

    /// Resource management settings
    pub resource_config: ResourceConfig,

    /// Optional stderr handler for process monitoring
    pub stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

impl std::fmt::Debug for ClangdConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClangdConfig")
            .field("working_directory", &self.working_directory)
            .field("clangd_path", &self.clangd_path)
            .field("build_directory", &self.build_directory)
            .field("extra_args", &self.extra_args)
            .field("lsp_config", &self.lsp_config)
            .field("resource_config", &self.resource_config)
            .field(
                "stderr_handler",
                &self.stderr_handler.as_ref().map(|_| "Fn(String)"),
            )
            .finish()
    }
}

/// LSP client configuration
#[derive(Debug, Clone)]
pub struct LspConfig {
    /// Root URI for LSP initialization
    pub root_uri: Option<String>,

    /// Timeout for LSP initialization
    pub initialization_timeout: Duration,

    /// Timeout for individual LSP requests
    pub request_timeout: Duration,

    /// Enable verbose LSP tracing
    pub verbose_tracing: bool,

    /// Client name for LSP identification
    pub client_name: String,

    /// Client version for LSP identification
    pub client_version: String,
}

/// Resource management configuration
#[derive(Debug, Clone)]
pub struct ResourceConfig {
    /// Stderr log file path (optional)
    pub stderr_log_path: Option<PathBuf>,

    /// Maximum memory usage hint for clangd
    pub max_memory_mb: Option<u64>,

    /// Process priority setting
    pub process_priority: ProcessPriority,

    /// Enable background indexing
    pub background_indexing: bool,

    /// Limit on number of concurrent clangd processes
    pub max_concurrent_processes: Option<u32>,
}

/// Process priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessPriority {
    /// Normal process priority
    Normal,
    /// Low process priority (background)
    Low,
    /// High process priority (interactive)
    High,
}

// ============================================================================
// Configuration Builder
// ============================================================================

/// Builder for ClangdConfig with validation and defaults
pub struct ClangdConfigBuilder {
    working_directory: Option<PathBuf>,
    clangd_path: Option<String>,
    build_directory: Option<PathBuf>,
    extra_args: Vec<String>,
    lsp_config: LspConfigBuilder,
    resource_config: ResourceConfigBuilder,
    stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

/// Builder for LspConfig
#[derive(Debug, Default)]
pub struct LspConfigBuilder {
    root_uri: Option<String>,
    initialization_timeout: Option<Duration>,
    request_timeout: Option<Duration>,
    verbose_tracing: Option<bool>,
    client_name: Option<String>,
    client_version: Option<String>,
}

/// Builder for ResourceConfig
#[derive(Debug, Default)]
pub struct ResourceConfigBuilder {
    stderr_log_path: Option<PathBuf>,
    max_memory_mb: Option<u64>,
    process_priority: Option<ProcessPriority>,
    background_indexing: Option<bool>,
    max_concurrent_processes: Option<u32>,
}

// ============================================================================
// Default Implementations
// ============================================================================

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            root_uri: None,
            initialization_timeout: Duration::from_secs(DEFAULT_INITIALIZATION_TIMEOUT_SECS),
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            verbose_tracing: false,
            client_name: "mcp-cpp-clangd-client".to_string(),
            client_version: "0.1.0".to_string(),
        }
    }
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            stderr_log_path: None,
            max_memory_mb: None,
            process_priority: ProcessPriority::Normal,
            background_indexing: true,
            max_concurrent_processes: None,
        }
    }
}

// ============================================================================
// Builder Implementation
// ============================================================================

impl ClangdConfigBuilder {
    /// Create a new configuration builder
    pub fn new() -> Self {
        Self {
            working_directory: None,
            clangd_path: None,
            build_directory: None,
            extra_args: Vec::new(),
            lsp_config: LspConfigBuilder::default(),
            resource_config: ResourceConfigBuilder::default(),
            stderr_handler: None,
        }
    }

    /// Set the working directory for the clangd process
    pub fn working_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(path.into());
        self
    }

    /// Set the path to the clangd executable
    pub fn clangd_path(mut self, path: impl Into<String>) -> Self {
        self.clangd_path = Some(path.into());
        self
    }

    /// Set the build directory containing compile_commands.json
    pub fn build_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.build_directory = Some(path.into());
        self
    }

    /// Add an extra command-line argument for clangd
    pub fn add_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Add multiple extra command-line arguments for clangd
    pub fn add_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.extra_args
            .extend(args.into_iter().map(|arg| arg.into()));
        self
    }

    /// Set the LSP root URI
    pub fn root_uri(mut self, uri: impl Into<String>) -> Self {
        self.lsp_config.root_uri = Some(uri.into());
        self
    }

    /// Set the LSP initialization timeout
    pub fn initialization_timeout(mut self, timeout: Duration) -> Self {
        self.lsp_config.initialization_timeout = Some(timeout);
        self
    }

    /// Set the LSP request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.lsp_config.request_timeout = Some(timeout);
        self
    }

    /// Enable verbose LSP tracing
    pub fn verbose_tracing(mut self, enabled: bool) -> Self {
        self.lsp_config.verbose_tracing = Some(enabled);
        self
    }

    /// Set the LSP client name
    pub fn client_name(mut self, name: impl Into<String>) -> Self {
        self.lsp_config.client_name = Some(name.into());
        self
    }

    /// Set the LSP client version
    pub fn client_version(mut self, version: impl Into<String>) -> Self {
        self.lsp_config.client_version = Some(version.into());
        self
    }

    /// Set the stderr handler for process monitoring
    pub fn stderr_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.stderr_handler = Some(Arc::new(handler));
        self
    }

    /// Set the stderr log file path
    pub fn stderr_log(mut self, path: impl Into<PathBuf>) -> Self {
        self.resource_config.stderr_log_path = Some(path.into());
        self
    }

    /// Set the maximum memory usage hint
    pub fn max_memory_mb(mut self, memory_mb: u64) -> Self {
        self.resource_config.max_memory_mb = Some(memory_mb);
        self
    }

    /// Set the process priority
    pub fn process_priority(mut self, priority: ProcessPriority) -> Self {
        self.resource_config.process_priority = Some(priority);
        self
    }

    /// Enable or disable background indexing
    pub fn background_indexing(mut self, enabled: bool) -> Self {
        self.resource_config.background_indexing = Some(enabled);
        self
    }

    /// Set the maximum number of concurrent clangd processes
    pub fn max_concurrent_processes(mut self, count: u32) -> Self {
        self.resource_config.max_concurrent_processes = Some(count);
        self
    }

    /// Build the configuration with validation
    pub fn build(self) -> Result<ClangdConfig, ClangdConfigError> {
        // Validate required fields
        let working_directory = self
            .working_directory
            .ok_or_else(|| ClangdConfigError::missing_field("working_directory"))?;

        let build_directory = self
            .build_directory
            .ok_or_else(|| ClangdConfigError::missing_field("build_directory"))?;

        // Use default clangd path if not specified
        let clangd_path = self.clangd_path.unwrap_or_else(|| "clangd".to_string());

        // Build LSP config
        let lsp_config = self.lsp_config.build();

        // Build resource config
        let resource_config = self.resource_config.build();

        // Validate paths
        Self::validate_working_directory(&working_directory)?;
        Self::validate_build_directory(&build_directory)?;
        Self::validate_clangd_path(&clangd_path)?;

        // Validate timeouts
        Self::validate_timeouts(&lsp_config)?;

        // Validate arguments
        Self::validate_arguments(&self.extra_args)?;

        Ok(ClangdConfig {
            working_directory,
            clangd_path,
            build_directory,
            extra_args: self.extra_args,
            lsp_config,
            resource_config,
            stderr_handler: self.stderr_handler,
        })
    }

    /// Validate working directory exists and is readable
    fn validate_working_directory(path: &Path) -> Result<(), ClangdConfigError> {
        if !path.exists() {
            return Err(ClangdConfigError::WorkingDirectoryValidation {
                working_dir: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Working directory does not exist",
                ),
            });
        }

        if !path.is_dir() {
            return Err(ClangdConfigError::WorkingDirectoryValidation {
                working_dir: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Working directory path is not a directory",
                ),
            });
        }

        Ok(())
    }

    /// Validate build directory exists and contains compile_commands.json
    fn validate_build_directory(path: &Path) -> Result<(), ClangdConfigError> {
        if !path.exists() {
            return Err(ClangdConfigError::BuildDirectoryValidation {
                build_dir: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Build directory does not exist",
                ),
            });
        }

        if !path.is_dir() {
            return Err(ClangdConfigError::BuildDirectoryValidation {
                build_dir: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Build directory path is not a directory",
                ),
            });
        }

        let compile_commands = path.join("compile_commands.json");
        if !compile_commands.exists() {
            return Err(ClangdConfigError::BuildDirectoryValidation {
                build_dir: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "compile_commands.json not found in build directory",
                ),
            });
        }

        Ok(())
    }

    /// Validate clangd executable path
    fn validate_clangd_path(clangd_path: &str) -> Result<(), ClangdConfigError> {
        // Basic validation - check if it's not empty and doesn't contain invalid characters
        if clangd_path.is_empty() {
            return Err(ClangdConfigError::invalid_path(
                clangd_path,
                "Clangd path cannot be empty",
            ));
        }

        // Check for obviously invalid characters
        if clangd_path.contains('\0') {
            return Err(ClangdConfigError::invalid_path(
                clangd_path,
                "Clangd path contains null character",
            ));
        }

        // Note: We don't check if the executable exists here because:
        // 1. It might be in PATH and require resolution
        // 2. It might be installed after configuration but before session start
        // 3. The session will validate it during startup

        Ok(())
    }

    /// Validate timeout values
    fn validate_timeouts(lsp_config: &LspConfig) -> Result<(), ClangdConfigError> {
        if lsp_config.initialization_timeout.is_zero() {
            return Err(ClangdConfigError::invalid_timeout(
                lsp_config.initialization_timeout,
                "Initialization timeout must be greater than zero",
            ));
        }

        if lsp_config.request_timeout.is_zero() {
            return Err(ClangdConfigError::invalid_timeout(
                lsp_config.request_timeout,
                "Request timeout must be greater than zero",
            ));
        }

        if lsp_config.initialization_timeout > Duration::from_secs(MAX_INITIALIZATION_TIMEOUT_SECS)
        {
            return Err(ClangdConfigError::invalid_timeout(
                lsp_config.initialization_timeout,
                "Initialization timeout too long (max 5 minutes)",
            ));
        }

        Ok(())
    }

    /// Validate command-line arguments
    fn validate_arguments(args: &[String]) -> Result<(), ClangdConfigError> {
        for arg in args {
            if arg.contains('\0') {
                return Err(ClangdConfigError::invalid_arguments(
                    args.to_vec(),
                    "Arguments cannot contain null characters",
                ));
            }
        }

        Ok(())
    }
}

impl LspConfigBuilder {
    /// Build the LSP configuration
    fn build(self) -> LspConfig {
        let default = LspConfig::default();

        LspConfig {
            root_uri: self.root_uri,
            initialization_timeout: self
                .initialization_timeout
                .unwrap_or(default.initialization_timeout),
            request_timeout: self.request_timeout.unwrap_or(default.request_timeout),
            verbose_tracing: self.verbose_tracing.unwrap_or(default.verbose_tracing),
            client_name: self.client_name.unwrap_or(default.client_name),
            client_version: self.client_version.unwrap_or(default.client_version),
        }
    }
}

impl ResourceConfigBuilder {
    /// Build the resource configuration
    fn build(self) -> ResourceConfig {
        let default = ResourceConfig::default();

        ResourceConfig {
            stderr_log_path: self.stderr_log_path,
            max_memory_mb: self.max_memory_mb,
            process_priority: self.process_priority.unwrap_or(default.process_priority),
            background_indexing: self
                .background_indexing
                .unwrap_or(default.background_indexing),
            max_concurrent_processes: self.max_concurrent_processes,
        }
    }
}

impl Default for ClangdConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Utility Methods
// ============================================================================

impl ClangdConfig {
    /// Get the full command-line arguments for clangd
    pub fn get_clangd_args(&self) -> Vec<String> {
        let mut args = vec![
            "--compile-commands-dir".to_string(),
            self.build_directory.to_string_lossy().to_string(),
        ];

        // Add background indexing control
        if !self.resource_config.background_indexing {
            args.push("--background-index=false".to_string());
        }

        // Add memory limit if specified
        if let Some(memory_mb) = self.resource_config.max_memory_mb {
            args.push(format!(
                "--limit-results={}",
                memory_mb * MEMORY_TO_RESULTS_FACTOR
            ));
        }

        // Add extra arguments
        args.extend(self.extra_args.clone());

        args
    }

    /// Get the root URI for LSP initialization
    pub fn get_root_uri(&self) -> Option<String> {
        self.lsp_config.root_uri.clone().or_else(|| {
            // Auto-generate from working directory if not specified
            Some(format!(
                "file://{}",
                self.working_directory.to_string_lossy()
            ))
        })
    }

    /// Check if verbose tracing is enabled
    pub fn is_verbose_tracing(&self) -> bool {
        self.lsp_config.verbose_tracing
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_builder_full() {
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        std::fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .clangd_path("/usr/bin/clangd")
            .build_directory(&build_dir)
            .add_arg("--log=verbose")
            .add_arg("--pretty")
            .root_uri("file:///test/project")
            .initialization_timeout(Duration::from_secs(60))
            .verbose_tracing(true)
            .max_memory_mb(2048)
            .process_priority(ProcessPriority::High)
            .build()
            .unwrap();

        assert_eq!(config.clangd_path, "/usr/bin/clangd");
        assert_eq!(config.extra_args, vec!["--log=verbose", "--pretty"]);
        assert_eq!(
            config.lsp_config.root_uri,
            Some("file:///test/project".to_string())
        );
        assert_eq!(
            config.lsp_config.initialization_timeout,
            Duration::from_secs(60)
        );
        assert!(config.lsp_config.verbose_tracing);
        assert_eq!(config.resource_config.max_memory_mb, Some(2048));
        assert_eq!(
            config.resource_config.process_priority,
            ProcessPriority::High
        );
    }

    #[test]
    fn test_config_validation_missing_fields() {
        let result = ClangdConfigBuilder::new().build();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("working_directory")
        );
    }

    #[test]
    fn test_config_validation_invalid_timeout() {
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        std::fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let result = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .initialization_timeout(Duration::from_secs(0))
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));
    }

    #[test]
    fn test_clangd_args_generation() {
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        std::fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .add_arg("--log=verbose")
            .max_memory_mb(1024)
            .background_indexing(false)
            .build()
            .unwrap();

        let args = config.get_clangd_args();
        assert!(args.contains(&"--compile-commands-dir".to_string()));
        assert!(args.contains(&build_dir.to_string_lossy().to_string()));
        assert!(args.contains(&"--background-index=false".to_string()));
        assert!(args.contains(&"--log=verbose".to_string()));
        assert!(args.iter().any(|arg| arg.starts_with("--limit-results=")));
    }

    #[test]
    fn test_root_uri_auto_generation() {
        let temp_dir = tempdir().unwrap();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir(&build_dir).unwrap();
        std::fs::write(build_dir.join("compile_commands.json"), "[]").unwrap();

        let config = ClangdConfigBuilder::new()
            .working_directory(temp_dir.path())
            .build_directory(&build_dir)
            .build()
            .unwrap();

        let root_uri = config.get_root_uri().unwrap();
        assert!(root_uri.starts_with("file://"));
        assert!(root_uri.contains(&temp_dir.path().to_string_lossy().to_string()));
    }
}
