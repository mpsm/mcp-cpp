use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use serde_json::Value;

use crate::lsp::client::LspClient;
use crate::lsp::error::LspError;
use crate::lsp::types::IndexingState;

pub struct ClangdManager {
    current_build_dir: Arc<Mutex<Option<PathBuf>>>,
    lsp_client: Arc<Mutex<Option<LspClient>>>,
    is_initialized: Arc<Mutex<bool>>,
    indexing_state: Arc<Mutex<IndexingState>>,
}

impl ClangdManager {
    pub fn new() -> Self {
        Self {
            current_build_dir: Arc::new(Mutex::new(None)),
            lsp_client: Arc::new(Mutex::new(None)),
            is_initialized: Arc::new(Mutex::new(false)),
            indexing_state: Arc::new(Mutex::new(IndexingState::new())),
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

        // Check if build directory is different from current
        let current_build_dir = self.get_current_build_directory().await;
        let session_changed = match current_build_dir {
            Some(ref current) => current != &build_directory,
            None => true,
        };

        if session_changed {
            info!("Build directory changed, shutting down existing clangd session");
        }

        // Get clangd path from environment or use default
        let clangd_path = std::env::var("CLANGD_PATH").unwrap_or_else(|_| "clangd".to_string());
        
        // Check clangd version
        self.check_clangd_version(&clangd_path).await?;
        
        // Shutdown existing clangd if running or build directory changed
        if session_changed {
            self.shutdown_clangd().await?;
        }

        info!("Setting up clangd for build directory: {}", build_directory.display());

        // Start new clangd process
        let client = LspClient::start_clangd(&clangd_path, &build_directory, self.indexing_state.clone()).await?;
        
        // Update state
        {
            let mut current_dir = self.current_build_dir.lock().await;
            *current_dir = Some(build_directory.clone());
        }
        
        {
            let mut lsp_client = self.lsp_client.lock().await;
            *lsp_client = Some(client);
        }

        // Perform full LSP initialization sequence and trigger indexing
        {
            let mut is_initialized = self.is_initialized.lock().await;
            if !*is_initialized {
                info!("Performing LSP initialization sequence");
                // Add timeout to prevent hanging
                match tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    self.perform_lsp_initialization(&build_directory)
                ).await {
                    Ok(result) => {
                        result?;
                        *is_initialized = true;
                        info!("LSP initialization completed successfully");
                    }
                    Err(_) => {
                        return Err(LspError::ProcessError("LSP initialization timed out after 60 seconds".to_string()));
                    }
                }
            } else {
                info!("LSP session already initialized, skipping initialization");
            }
        }

        Ok(format!(
            "Clangd initialization completed for build directory: {}. Using clangd binary: {}. LSP session initialized and background indexing started. Monitor logs for indexing progress.",
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
                    // Handle "server already initialized" error more gracefully
                    if error.code == -32600 && error.message.contains("server already initialized") {
                        // Return a successful response for compatibility
                        return Ok(serde_json::json!({
                            "capabilities": {
                                "textDocumentSync": 1,
                                "definitionProvider": true,
                                "hoverProvider": true,
                                "completionProvider": {
                                    "triggerCharacters": [".", "->", "::"]
                                }
                            }
                        }));
                    }
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

    async fn perform_lsp_initialization(&self, build_directory: &std::path::Path) -> Result<(), LspError> {
        use serde_json::json;
        
        // Step 1: Send initialize request
        let init_params = json!({
            "processId": std::process::id(),
            "rootPath": build_directory.display().to_string(),
            "rootUri": format!("file://{}", build_directory.display()),
            "capabilities": {
                "workspace": {
                    "workDoneProgress": true,
                    "workspaceFolders": true,
                    "didChangeWatchedFiles": {
                        "dynamicRegistration": true
                    }
                },
                "window": {
                    "workDoneProgress": true
                },
                "textDocument": {
                    "definition": {"linkSupport": true},
                    "declaration": {"linkSupport": true},
                    "references": {"context": true},
                    "implementation": {"linkSupport": true},
                    "hover": {"contentFormat": ["markdown", "plaintext"]},
                    "documentSymbol": {
                        "hierarchicalDocumentSymbolSupport": true
                    },
                    "completion": {
                        "completionItem": {
                            "snippetSupport": true,
                            "documentationFormat": ["markdown", "plaintext"]
                        }
                    }
                }
            },
            "initializationOptions": {
                "clangdFileStatus": true,
                "fallbackFlags": ["-std=c++20"]
            }
        });

        info!("Sending LSP initialize request");
        let _init_response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.send_lsp_request("initialize".to_string(), Some(init_params))
        ).await
        .map_err(|_| LspError::ProcessError("Timeout during LSP initialize request".to_string()))?
        .map_err(|e| LspError::ProcessError(format!("LSP initialize request failed: {}", e)))?;
        info!("LSP initialize request completed");

        // Step 2: Send initialized notification
        info!("Sending initialized notification");
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.send_lsp_notification("initialized".to_string(), Some(json!({})))
        ).await
        .map_err(|_| LspError::ProcessError("Timeout during initialized notification".to_string()))?
        .map_err(|e| LspError::ProcessError(format!("Initialized notification failed: {}", e)))?;
        info!("LSP initialization sequence completed");

        // Step 3: Trigger indexing by opening first file from compile_commands.json
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            self.trigger_indexing_by_opening_file(build_directory)
        ).await
        .map_err(|_| LspError::ProcessError("Timeout during file opening for indexing trigger".to_string()))?
        .map_err(|e| LspError::ProcessError(format!("Failed to trigger indexing: {}", e)))?;

        Ok(())
    }

    async fn trigger_indexing_by_opening_file(&self, build_directory: &std::path::Path) -> Result<(), LspError> {
        use serde_json::json;
        
        // Read compile_commands.json to find first source file
        let compile_commands_path = build_directory.join("compile_commands.json");
        let compile_commands_content = std::fs::read_to_string(&compile_commands_path)
            .map_err(|e| LspError::BuildDirectoryError(format!("Failed to read compile_commands.json: {}", e)))?;
        
        let compile_commands: Vec<serde_json::Value> = serde_json::from_str(&compile_commands_content)
            .map_err(|e| LspError::BuildDirectoryError(format!("Failed to parse compile_commands.json: {}", e)))?;
        
        if compile_commands.is_empty() {
            return Err(LspError::BuildDirectoryError("No entries found in compile_commands.json".to_string()));
        }
        
        // Find first source file
        let first_file = compile_commands.iter()
            .find_map(|entry| entry.get("file").and_then(|f| f.as_str()))
            .ok_or_else(|| LspError::BuildDirectoryError("No file entries found in compile_commands.json".to_string()))?;
        
        let file_path = std::path::Path::new(first_file);
        let file_uri = format!("file://{}", file_path.display());
        
        info!("Triggering indexing by opening file: {}", file_path.display());
        
        // Read file content
        let file_content = std::fs::read_to_string(file_path)
            .map_err(|e| LspError::BuildDirectoryError(format!("Failed to read file {}: {}", file_path.display(), e)))?;
        
        // Send textDocument/didOpen notification
        let did_open_params = json!({
            "textDocument": {
                "uri": file_uri,
                "languageId": "cpp",
                "version": 1,
                "text": file_content
            }
        });
        
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.send_lsp_notification("textDocument/didOpen".to_string(), Some(did_open_params))
        ).await
        .map_err(|_| LspError::ProcessError("Timeout during textDocument/didOpen notification".to_string()))?
        .map_err(|e| LspError::ProcessError(format!("Failed to send didOpen notification: {}", e)))?;
        info!("File opened, background indexing should now start");
        
        Ok(())
    }

    pub async fn get_current_build_directory(&self) -> Option<PathBuf> {
        let current_dir = self.current_build_dir.lock().await;
        current_dir.clone()
    }

    pub async fn get_indexing_state(&self) -> IndexingState {
        let state = self.indexing_state.lock().await;
        state.clone()
    }

    #[allow(dead_code)]
    pub fn get_indexing_state_ref(&self) -> Arc<Mutex<IndexingState>> {
        self.indexing_state.clone()
    }

    async fn check_clangd_version(&self, clangd_path: &str) -> Result<(), LspError> {
        use std::process::Command;
        
        info!("Checking clangd version for {}", clangd_path);
        
        let output = Command::new(clangd_path)
            .arg("--version")
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    LspError::ClangdNotFound
                } else {
                    LspError::ProcessError(format!("Failed to check clangd version: {}", e))
                }
            })?;

        if !output.status.success() {
            return Err(LspError::ProcessError(format!(
                "Failed to get clangd version: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        info!("Clangd version output: {}", version_output);
        
        // Parse version number from output like "clangd version 20.0.0"
        let version_number = self.parse_clangd_version(&version_output)?;
        
        if version_number < 20 {
            return Err(LspError::ProcessError(format!(
                "Clangd version {} is too old. Minimum required version is 20. Current version: {}",
                version_number,
                version_output.trim()
            )));
        }

        info!("Clangd version {} meets minimum requirement (20+)", version_number);
        Ok(())
    }

    fn parse_clangd_version(&self, version_output: &str) -> Result<u32, LspError> {
        // Look for pattern like "clangd version X.Y.Z" or "version X.Y.Z"
        let version_regex = regex::Regex::new(r"(?i)version\s+(\d+)\.?")
            .map_err(|e| LspError::ProcessError(format!("Failed to create version regex: {}", e)))?;
        
        if let Some(captures) = version_regex.captures(version_output) {
            if let Some(version_match) = captures.get(1) {
                let version_str = version_match.as_str();
                return version_str.parse::<u32>()
                    .map_err(|e| LspError::ProcessError(format!("Failed to parse version number '{}': {}", version_str, e)));
            }
        }
        
        Err(LspError::ProcessError(format!(
            "Could not parse clangd version from output: {}",
            version_output.trim()
        )))
    }

    async fn shutdown_clangd(&self) -> Result<(), LspError> {
        let mut client_guard = self.lsp_client.lock().await;
        
        if let Some(mut client) = client_guard.take() {
            info!("Shutting down existing clangd process");
            if let Err(e) = client.shutdown().await {
                warn!("Error during clangd shutdown: {}", e);
            }
        }
        
        // Reset initialization flag
        {
            let mut is_initialized = self.is_initialized.lock().await;
            *is_initialized = false;
        }

        // Reset indexing state
        {
            let mut indexing_state = self.indexing_state.lock().await;
            *indexing_state = IndexingState::new();
        }
        
        Ok(())
    }

}

impl Default for ClangdManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::IndexingStatus;

    #[test]
    fn test_manager_new() {
        let manager = ClangdManager::new();
        // Basic construction test - can't easily test internal state without getters
        assert!(matches!(manager, ClangdManager { .. }));
    }

    #[tokio::test]
    async fn test_get_indexing_state_default() {
        let manager = ClangdManager::new();
        let state = manager.get_indexing_state().await;
        
        assert_eq!(state.status, IndexingStatus::NotStarted);
        assert_eq!(state.files_processed, 0);
        assert_eq!(state.total_files, None);
        assert_eq!(state.percentage, None);
        assert_eq!(state.message, None);
        assert_eq!(state.estimated_completion_seconds, None);
        assert!(!state.is_indexing());
    }

    #[tokio::test]
    async fn test_get_current_build_directory_default() {
        let manager = ClangdManager::new();
        let build_dir = manager.get_current_build_directory().await;
        assert_eq!(build_dir, None);
    }

    #[test]
    fn test_parse_clangd_version_valid() {
        let manager = ClangdManager::new();
        
        // Test typical clangd version output
        let version_output = "clangd version 20.0.0";
        let version = manager.parse_clangd_version(version_output).unwrap();
        assert_eq!(version, 20);
        
        // Test version with more details
        let version_output = "clangd version 22.0.1 (https://github.com/llvm/llvm-project.git)";
        let version = manager.parse_clangd_version(version_output).unwrap();
        assert_eq!(version, 22);
        
        // Test version with different format
        let version_output = "version 18.1.8";
        let version = manager.parse_clangd_version(version_output).unwrap();
        assert_eq!(version, 18);
    }

    #[test]
    fn test_parse_clangd_version_invalid() {
        let manager = ClangdManager::new();
        
        // Test invalid version output
        let version_output = "clangd: command not found";
        assert!(manager.parse_clangd_version(version_output).is_err());
        
        // Test empty output
        let version_output = "";
        assert!(manager.parse_clangd_version(version_output).is_err());
        
        // Test output without version
        let version_output = "clangd help text";
        assert!(manager.parse_clangd_version(version_output).is_err());
    }

    #[test]
    fn test_parse_clangd_version_edge_cases() {
        let manager = ClangdManager::new();
        
        // Test version with no decimal point
        let version_output = "clangd version 20";
        let version = manager.parse_clangd_version(version_output).unwrap();
        assert_eq!(version, 20);
        
        // Test version with case insensitive
        let version_output = "CLANGD VERSION 19.0.0";
        let version = manager.parse_clangd_version(version_output).unwrap();
        assert_eq!(version, 19);
    }

    #[tokio::test]
    async fn test_indexing_state_update_through_reference() {
        let manager = ClangdManager::new();
        let state_ref = manager.get_indexing_state_ref();
        
        // Update state through reference
        {
            let mut state = state_ref.lock().await;
            state.start_indexing(Some("Test indexing".to_string()));
        }
        
        // Verify state was updated
        let state = manager.get_indexing_state().await;
        assert_eq!(state.status, IndexingStatus::InProgress);
        assert_eq!(state.message, Some("Test indexing".to_string()));
        assert!(state.is_indexing());
    }
}