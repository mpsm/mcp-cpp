use rust_mcp_sdk::schema::schema_utils::CallToolError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LspError {
    #[error("Clangd not setup. Run setup_clangd tool first.")]
    NotSetup,

    #[error("Clangd process error: {0}")]
    ProcessError(String),

    #[error("JSON-RPC error: {0}")]
    JsonRpcError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Build directory error: {0}")]
    BuildDirectoryError(String),

    #[error("Clangd binary not found. Check CLANGD_PATH environment variable.")]
    ClangdNotFound,
}

impl From<LspError> for CallToolError {
    fn from(error: LspError) -> Self {
        match error {
            LspError::NotSetup => CallToolError::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "{}. Workflow: 1. Optional: cpp_project_status, 2. Required: setup_clangd, 3. Use: lsp_request. See lsp://workflow resource for details.",
                    error
                ),
            )),
            _ => CallToolError::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                error.to_string(),
            )),
        }
    }
}
