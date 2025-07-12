use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::lsp::error::LspError;
use crate::lsp::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

pub struct LspClient {
    process: Child,
    pending_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    reader_task: Option<JoinHandle<()>>,
    shutdown_signal: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
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

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| LspError::ProcessError("Failed to get stdin".to_string()))?;

        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| LspError::ProcessError("Failed to get stdout".to_string()))?;

        let pending_requests: Arc<
            Mutex<HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>,
        > = Arc::new(Mutex::new(HashMap::new()));
        let pending_requests_clone = pending_requests.clone();

        // Create shutdown signal
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let shutdown_signal = Arc::new(Mutex::new(Some(shutdown_tx)));

        // Start reading responses
        let reader_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("LSP reader task received shutdown signal");
                        break;
                    }
                    result = reader.read_line(&mut line) => {
                        match result {
                            Ok(0) => {
                                info!("LSP client reached EOF, stopping reader task");
                                break; // EOF
                            }
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
                                line.clear();
                            }
                            Err(e) => {
                                warn!("Error reading from clangd: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
            info!("LSP reader task terminated");
        });

        let client = Self {
            process,
            pending_requests,
            stdin: Arc::new(Mutex::new(stdin)),
            reader_task: Some(reader_task),
            shutdown_signal,
        };

        // Note: Don't auto-initialize here, let the user control initialization
        info!("Clangd client created successfully (not yet initialized)");
        Ok(client)
    }

    pub async fn send_request(
        &self,
        method: String,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse, LspError> {
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
            Ok(Err(_)) => Err(LspError::JsonRpcError(
                "Response channel closed".to_string(),
            )),
            Err(_) => {
                // Remove from pending requests on timeout
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&request_id);
                Err(LspError::JsonRpcError("Request timeout".to_string()))
            }
        }
    }

    pub async fn send_notification(
        &self,
        method: String,
        params: Option<Value>,
    ) -> Result<(), LspError> {
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

        let mut shutdown_errors = Vec::new();

        // Step 1: Send graceful shutdown signal to reader task
        {
            let mut shutdown_signal = self.shutdown_signal.lock().await;
            if let Some(tx) = shutdown_signal.take() {
                if let Err(_) = tx.send(()) {
                    shutdown_errors.push("Failed to send shutdown signal to reader task");
                }
            }
        }

        // Step 2: Send LSP shutdown request (with timeout)
        let shutdown_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.send_request("shutdown".to_string(), None),
        )
        .await;

        match shutdown_result {
            Ok(Ok(_)) => {
                // Send exit notification
                let exit_result = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    self.send_notification("exit".to_string(), None),
                )
                .await;

                if let Err(_) = exit_result {
                    shutdown_errors.push("Timeout sending exit notification");
                }
            }
            Ok(Err(e)) => {
                shutdown_errors.push("LSP shutdown request failed");
                warn!("LSP shutdown request failed: {}", e);
            }
            Err(_) => {
                shutdown_errors.push("Timeout during LSP shutdown request");
            }
        }

        // Step 3: Wait for reader task to finish (with timeout)
        if let Some(reader_task) = self.reader_task.take() {
            match tokio::time::timeout(std::time::Duration::from_secs(3), reader_task).await {
                Ok(Ok(())) => {
                    info!("Reader task terminated cleanly");
                }
                Ok(Err(e)) => {
                    shutdown_errors.push("Reader task failed during shutdown");
                    warn!("Reader task failed: {}", e);
                }
                Err(_) => {
                    shutdown_errors.push("Timeout waiting for reader task");
                    warn!("Reader task did not terminate within timeout");
                }
            }
        }

        // Step 4: Force kill the process if still running
        match self.process.try_wait() {
            Ok(Some(_)) => {
                info!("Clangd process already terminated");
            }
            Ok(None) => {
                info!("Force killing clangd process");
                if let Err(e) = self.process.kill().await {
                    shutdown_errors.push("Failed to kill clangd process");
                    warn!("Failed to kill clangd process: {}", e);
                }
            }
            Err(e) => {
                shutdown_errors.push("Error checking process status");
                warn!("Error checking process status: {}", e);
            }
        }

        // Step 5: Clear pending requests and notify waiters
        {
            let mut pending = self.pending_requests.lock().await;
            let count = pending.len();
            if count > 0 {
                warn!("Cleaning up {} pending requests during shutdown", count);
                // Drop all pending requests - their receivers will get channel closed errors
                pending.clear();
            }
        }

        // Return error if any step failed
        if shutdown_errors.is_empty() {
            info!("LSP client shutdown completed successfully");
            Ok(())
        } else {
            let error_msg = format!(
                "Shutdown completed with errors: {}",
                shutdown_errors.join(", ")
            );
            warn!("{}", error_msg);
            Err(LspError::ProcessError(error_msg))
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        warn!("LspClient being dropped - attempting emergency cleanup");

        // Signal the reader task to stop
        if let Ok(mut shutdown_signal) = self.shutdown_signal.try_lock() {
            if let Some(tx) = shutdown_signal.take() {
                let _ = tx.send(());
            }
        }

        // Abort the reader task if it's still running
        if let Some(reader_task) = self.reader_task.take() {
            reader_task.abort();
        }

        // Force kill the process - this is synchronous for Drop
        if let Some(pid) = self.process.id() {
            warn!("Force killing clangd process {} during drop", pid);
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .output();
        }
    }
}
