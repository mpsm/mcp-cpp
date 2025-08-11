use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum ProjectError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path does not exist: {path}")]
    PathNotFound { path: String },

    #[error("Compilation database not found: {path}")]
    CompilationDatabaseNotFound { path: String },

    #[error("Invalid build directory: {reason}")]
    InvalidBuildDirectory { reason: String },

    #[error("Failed to parse build configuration: {reason}")]
    ParseError { reason: String },

    #[error("Build directory is not readable: {path}")]
    BuildDirectoryNotReadable { path: String },

    #[error("Source root directory not found: {path}")]
    SourceRootNotFound { path: String },

    #[error("Compilation database is not readable: {error}")]
    CompilationDatabaseNotReadable { error: String },

    #[error("Compilation database is invalid: {error}")]
    CompilationDatabaseInvalid { error: String },

    #[error("Compilation database is empty")]
    CompilationDatabaseEmpty,

    #[error("Session creation failed: {0}")]
    SessionCreation(String),
}
