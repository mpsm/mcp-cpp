use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::lsp::error::LspError;
use crate::lsp::types::{JsonRpcRequest, JsonRpcResponse, JsonRpcNotification, InitializeParams, ClientCapabilities, TextDocumentClientCapabilities};

pub struct LspClient {
    process: Child,
    pending_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
}

impl LspClient {
    pub async fn start_clangd(
        clangd_path: &str,
        build_directory: &std::path::Path,
    ) -> Result<Self, LspError> {
        info!("Starting clangd process at: {}", clangd_path);
        
        let mut command = Command::new(clangd_path);
        command
            .arg("--background-index=false")
            .current_dir(build_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut process = command.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LspError::ClangdNotFound
            } else {
                LspError::ProcessError(format!("Failed to start clangd: {}", e))
            }
        })?;

        let stdin = process.stdin.take().ok_or_else(|| {
            LspError::ProcessError("Failed to get stdin".to_string())
        })?;

        let stdout = process.stdout.take().ok_or_else(|| {
            LspError::ProcessError("Failed to get stdout".to_string())
        })?;

        let pending_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_requests_clone = pending_requests.clone();

        // Start reading responses
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        
                        // Look for Content-Length header
                        if let Some(content_length_str) = trimmed.strip_prefix("Content-Length:") {
                            if let Ok(content_length) = content_length_str.trim().parse::<usize>() {
                                // Read the empty line
                                line.clear();
                                if reader.read_line(&mut line).await.is_ok() {
                                    // Read the JSON content
                                    let mut content = vec![0u8; content_length];
                                    if let Ok(_) = reader.read_exact(&mut content).await {
                                        if let Ok(json_str) = String::from_utf8(content) {
                                            debug!("Received from clangd: {}", json_str);
                                            
                                            match serde_json::from_str::<JsonRpcResponse>(&json_str) {
                                                Ok(response) => {
                                                    // Only handle responses with non-empty IDs (not notifications)
                                                    if !response.id.is_empty() {
                                                        let mut pending = pending_requests_clone.lock().await;
                                                        if let Some(sender) = pending.remove(&response.id) {
                                                            if sender.send(response).is_err() {
                                                                warn!("Failed to send response to waiting request");
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    warn!("Failed to parse LSP response: {} - JSON: {}", e, json_str);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error reading from clangd: {}", e);
                        break;
                    }
                }
            }
        });

        let client = Self {
            process,
            pending_requests,
            stdin: Arc::new(Mutex::new(stdin)),
        };

        // Note: Don't auto-initialize here, let the user control initialization
        info!("Clangd client created successfully (not yet initialized)");
        Ok(client)
    }

    pub async fn initialize(&mut self, build_directory: &std::path::Path) -> Result<(), LspError> {
        let root_uri = format!("file://{}", build_directory.display());
        
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(root_uri),
            capabilities: ClientCapabilities {
                text_document: Some(TextDocumentClientCapabilities {
                    completion: Some(Value::Object(serde_json::Map::new())),
                    hover: Some(Value::Object(serde_json::Map::new())),
                    definition: Some(Value::Object(serde_json::Map::new())),
                    references: Some(Value::Object(serde_json::Map::new())),
                    document_symbol: Some(Value::Object(serde_json::Map::new())),
                }),
            },
        };

        let response = self.send_request("initialize".to_string(), Some(serde_json::to_value(params)?)).await?;
        
        if response.error.is_some() {
            return Err(LspError::JsonRpcError(format!("Initialize failed: {:?}", response.error)));
        }

        // Send initialized notification
        self.send_notification("initialized".to_string(), None).await?;
        
        Ok(())
    }

    pub async fn send_request(&self, method: String, params: Option<Value>) -> Result<JsonRpcResponse, LspError> {
        let request = JsonRpcRequest::new(method, params);
        let request_id = request.id.clone();
        
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(request_id.clone(), tx);
        }

        self.send_message(&request).await?;
        
        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(LspError::JsonRpcError("Response channel closed".to_string())),
            Err(_) => {
                // Remove from pending requests on timeout
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&request_id);
                Err(LspError::JsonRpcError("Request timeout".to_string()))
            }
        }
    }

    pub async fn send_notification(&self, method: String, params: Option<Value>) -> Result<(), LspError> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        };
        
        self.send_message(&notification).await
    }

    async fn send_message<T: serde::Serialize>(&self, message: &T) -> Result<(), LspError> {
        let json = serde_json::to_string(message)?;
        let content = format!("Content-Length: {}\r\n\r\n{}", json.len(), json);
        
        debug!("Sending to clangd: {}", json);
        
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(content.as_bytes()).await?;
        stdin.flush().await?;
        
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), LspError> {
        info!("Shutting down clangd client");
        
        // Send shutdown request
        if self.send_request("shutdown".to_string(), None).await.is_ok() {
            // Send exit notification
            let _ = self.send_notification("exit".to_string(), None).await;
        }
        
        // Kill the process if it's still running
        if let Err(e) = self.process.kill().await {
            warn!("Failed to kill clangd process: {}", e);
        }
        
        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Best effort cleanup
        let _ = std::process::Command::new("kill")
            .arg("-9")
            .arg(self.process.id().unwrap_or(0).to_string())
            .output();
    }
}