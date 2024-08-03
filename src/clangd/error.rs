//! Error types for clangd session management
//!
//! Provides comprehensive error handling with proper context preservation
//! and structured error types for different failure scenarios.

use std::path::PathBuf;
use std::time::Duration;

use crate::lsp_v2::LspError;
use crate::lsp_v2::process::ProcessError;
use crate::project::ProjectError;

// ============================================================================
// Clangd Session Errors
// ============================================================================

/// Comprehensive error types for clangd session management
#[derive(Debug, thiserror::Error)]
pub enum ClangdSessionError {
    /// LSP client errors (initialization, requests, etc.)
    #[error("LSP error: {0}")]
    Lsp(#[from] LspError),

    /// Process management errors (start, stop, communication)
    #[error("Process error: {0}")]
    Process(#[from] ProcessError),

    /// Project detection and validation errors
    #[error("Project error: {0}")]
    Project(#[from] ProjectError),

    /// Configuration validation errors
    #[error("Configuration error: {0}")]
    Config(#[from] ClangdConfigError),

    /// Invalid working directory
    #[error("Invalid working directory: {path}")]
    InvalidWorkingDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Missing compile_commands.json in build directory
    #[error("Missing compile_commands.json in build directory: {build_dir}")]
    MissingCompileCommands { build_dir: PathBuf },

    /// Session already started
    #[error("Session already started")]
    AlreadyStarted,

    /// Session not started
    #[error("Session not started")]
    NotStarted,

    /// Invalid session state transition
    #[error("Invalid session state: current={current}, expected={expected}")]
    InvalidState { current: String, expected: String },

    /// Build directory detection failed
    #[error("Build directory detection failed for project: {project_root}")]
    BuildDirectoryDetection {
        project_root: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// No build directory found in project
    #[error("No build directory found in project: {project_root}")]
    NoBuildDirectoryFound { project_root: PathBuf },

    /// Multiple build directories found (ambiguous)
    #[error(
        "Multiple build directories found in project: {project_root}, directories: {build_dirs:?}"
    )]
    MultipleBuildDirectories {
        project_root: PathBuf,
        build_dirs: Vec<PathBuf>,
    },

    /// Clangd executable not found or invalid
    #[error("Clangd executable not found or invalid: {clangd_path}")]
    InvalidClangdExecutable { clangd_path: String },

    /// Session operation timeout
    #[error("Session operation timeout: {operation} took longer than {timeout:?}")]
    OperationTimeout {
        operation: String,
        timeout: Duration,
    },

    /// Session startup failed
    #[error("Session startup failed: {reason}")]
    StartupFailed { reason: String },

    /// Session shutdown failed
    #[error("Session shutdown failed: {reason}")]
    ShutdownFailed { reason: String },

    /// Unexpected session failure
    #[error("Unexpected session failure: {reason}")]
    UnexpectedFailure { reason: String },
}

// ============================================================================
// Clangd Configuration Errors
// ============================================================================

/// Configuration validation and building errors
#[derive(Debug, thiserror::Error)]
pub enum ClangdConfigError {
    /// Missing required configuration field
    #[error("Missing required field: {field}")]
    MissingField { field: String },

    /// Invalid path format or value
    #[error("Invalid path: {path} - {reason}")]
    InvalidPath { path: String, reason: String },

    /// Invalid timeout value
    #[error("Invalid timeout: {timeout:?} - {reason}")]
    InvalidTimeout { timeout: Duration, reason: String },

    /// Invalid clangd arguments
    #[error("Invalid clangd arguments: {args:?} - {reason}")]
    InvalidArguments { args: Vec<String>, reason: String },

    /// Invalid LSP configuration
    #[error("Invalid LSP configuration: {reason}")]
    InvalidLspConfig { reason: String },

    /// Invalid resource configuration
    #[error("Invalid resource configuration: {reason}")]
    InvalidResourceConfig { reason: String },

    /// Path validation error
    #[error("Path validation failed: {path}")]
    PathValidation {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Build directory validation error
    #[error("Build directory validation failed: {build_dir}")]
    BuildDirectoryValidation {
        build_dir: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Working directory validation error
    #[error("Working directory validation failed: {working_dir}")]
    WorkingDirectoryValidation {
        working_dir: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Clangd executable validation error
    #[error("Clangd executable validation failed: {clangd_path}")]
    ClangdExecutableValidation {
        clangd_path: String,
        #[source]
        source: std::io::Error,
    },
}

// ============================================================================
// Error Conversion and Context Helpers
// ============================================================================

impl ClangdSessionError {
    /// Create a startup failure error with context
    pub fn startup_failed(reason: impl Into<String>) -> Self {
        Self::StartupFailed {
            reason: reason.into(),
        }
    }

    /// Create a shutdown failure error with context
    pub fn shutdown_failed(reason: impl Into<String>) -> Self {
        Self::ShutdownFailed {
            reason: reason.into(),
        }
    }

    /// Create an unexpected failure error with context
    pub fn unexpected_failure(reason: impl Into<String>) -> Self {
        Self::UnexpectedFailure {
            reason: reason.into(),
        }
    }

    /// Create an invalid state error
    pub fn invalid_state(current: impl Into<String>, expected: impl Into<String>) -> Self {
        Self::InvalidState {
            current: current.into(),
            expected: expected.into(),
        }
    }

    /// Create an operation timeout error
    pub fn operation_timeout(operation: impl Into<String>, timeout: Duration) -> Self {
        Self::OperationTimeout {
            operation: operation.into(),
            timeout,
        }
    }
}

impl ClangdConfigError {
    /// Create a missing field error
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    /// Create an invalid path error
    pub fn invalid_path(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidPath {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create an invalid timeout error
    pub fn invalid_timeout(timeout: Duration, reason: impl Into<String>) -> Self {
        Self::InvalidTimeout {
            timeout,
            reason: reason.into(),
        }
    }

    /// Create an invalid arguments error
    pub fn invalid_arguments(args: Vec<String>, reason: impl Into<String>) -> Self {
        Self::InvalidArguments {
            args,
            reason: reason.into(),
        }
    }

    /// Create an invalid LSP config error
    pub fn invalid_lsp_config(reason: impl Into<String>) -> Self {
        Self::InvalidLspConfig {
            reason: reason.into(),
        }
    }

    /// Create an invalid resource config error
    pub fn invalid_resource_config(reason: impl Into<String>) -> Self {
        Self::InvalidResourceConfig {
            reason: reason.into(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation_helpers() {
        let startup_error = ClangdSessionError::startup_failed("test reason");
        assert!(matches!(
            startup_error,
            ClangdSessionError::StartupFailed { .. }
        ));

        let config_error = ClangdConfigError::missing_field("test_field");
        assert!(matches!(
            config_error,
            ClangdConfigError::MissingField { .. }
        ));
    }

    #[test]
    fn test_error_conversion() {
        let config_error = ClangdConfigError::missing_field("test");
        let session_error: ClangdSessionError = config_error.into();
        assert!(matches!(session_error, ClangdSessionError::Config(_)));
    }
}
