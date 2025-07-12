use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use serde_json::Value;

use crate::lsp::client::LspClient;
use crate::lsp::error::LspError;

pub struct ClangdManager {
    current_build_dir: Arc<Mutex<Option<PathBuf>>>,
    lsp_client: Arc<Mutex<Option<LspClient>>>,
}

impl ClangdManager {
    pub fn new() -> Self {
        Self {
            current_build_dir: Arc::new(Mutex::new(None)),
            lsp_client: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn setup_clangd(&self, build_directory: PathBuf) -> Result<String, LspError> {
        // Validate build directory has compile_commands.json
        let compile_commands_path = build_directory.join("compile_commands.json");
        if !compile_commands_path.exists() {
            return Err(LspError::BuildDirectoryError(format!(
                "No compile_commands.json found in {}. Run CMake to generate it.",
                build_directory.display()
            )));
        }

        // Get clangd path from environment or use default
        let clangd_path = std::env::var("CLANGD_PATH").unwrap_or_else(|_| "clangd".to_string());
        
        // Shutdown existing clangd if running
        self.shutdown_clangd().await?;

        info!("Setting up clangd for build directory: {}", build_directory.display());

        // Start new clangd process
        let client = LspClient::start_clangd(&clangd_path, &build_directory).await?;
        
        // Update state
        {
            let mut current_dir = self.current_build_dir.lock().await;
            *current_dir = Some(build_directory.clone());
        }
        
        {
            let mut lsp_client = self.lsp_client.lock().await;
            *lsp_client = Some(client);
        }

        Ok(format!(
            "Clangd setup successful for build directory: {}. Using clangd binary: {}",
            build_directory.display(),
            clangd_path
        ))
    }

    pub async fn send_lsp_request(&self, method: String, params: Option<Value>) -> Result<Value, LspError> {
        let client_guard = self.lsp_client.lock().await;
        
        match client_guard.as_ref() {
            Some(client) => {
                let response = client.send_request(method, params).await?;
                
                if let Some(error) = response.error {
                    return Err(LspError::JsonRpcError(format!(
                        "LSP error {}: {}",
                        error.code,
                        error.message
                    )));
                }
                
                Ok(response.result.unwrap_or(Value::Null))
            }
            None => Err(LspError::NotSetup),
        }
    }

    pub async fn send_lsp_notification(&self, method: String, params: Option<Value>) -> Result<(), LspError> {
        let client_guard = self.lsp_client.lock().await;
        
        match client_guard.as_ref() {
            Some(client) => {
                client.send_notification(method, params).await
            }
            None => Err(LspError::NotSetup),
        }
    }


    async fn shutdown_clangd(&self) -> Result<(), LspError> {
        let mut client_guard = self.lsp_client.lock().await;
        
        if let Some(mut client) = client_guard.take() {
            info!("Shutting down existing clangd process");
            if let Err(e) = client.shutdown().await {
                warn!("Error during clangd shutdown: {}", e);
            }
        }
        
        Ok(())
    }

}

impl Default for ClangdManager {
    fn default() -> Self {
        Self::new()
    }
}