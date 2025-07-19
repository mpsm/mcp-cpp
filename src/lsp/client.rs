use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{Level, debug, info, warn};

use crate::lsp::error::LspError;
use crate::lsp::types::{IndexingState, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::{log_lsp_message, log_timing};

/// Represents the result of parsing an LSP message from the stream
#[derive(Debug)]
enum LspParseResult {
    /// Successfully parsed a complete LSP response
    Response(JsonRpcResponse),
    /// Parsed a line that's not a complete message (e.g., header, empty line)
    Incomplete,
    /// Failed to parse - contains error description
    ParseError(String),
    /// Reached end of stream
    EndOfStream,
    /// Error reading from stream
    ReadError(String),
}

/// LSP message parser helper functions
struct LspMessageParser;

impl LspMessageParser {
    /// Parse a single LSP message from the reader
    async fn parse_message<R>(reader: &mut BufReader<R>) -> LspParseResult
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mut line = String::new();

        // Read header line
        match reader.read_line(&mut line).await {
            Ok(0) => LspParseResult::EndOfStream,
            Ok(_) => {
                let trimmed = line.trim();

                // Parse Content-Length header
                if let Some(content_length) = Self::parse_content_length(trimmed) {
                    return Self::read_message_body(reader, content_length).await;
                }

                // Skip empty lines or other headers
                LspParseResult::Incomplete
            }
            Err(e) => LspParseResult::ReadError(format!("Failed to read header: {e}")),
        }
    }

    /// Parse Content-Length header from a line
    fn parse_content_length(line: &str) -> Option<usize> {
        line.strip_prefix("Content-Length:")
            .and_then(|s| s.trim().parse::<usize>().ok())
    }

    /// Read the message body given the content length
    async fn read_message_body<R>(
        reader: &mut BufReader<R>,
        content_length: usize,
    ) -> LspParseResult
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mut line = String::new();

        // Read the empty line separator
        if let Err(e) = reader.read_line(&mut line).await {
            return LspParseResult::ReadError(format!("Failed to read separator: {e}"));
        }

        // Read the JSON content
        let mut content = vec![0u8; content_length];
        if let Err(e) = reader.read_exact(&mut content).await {
            return LspParseResult::ReadError(format!("Failed to read message body: {e}"));
        }

        // Convert to string and parse JSON
        match String::from_utf8(content) {
            Ok(json_str) => {
                debug!("Received from clangd: {}", json_str);
                // Parse the response and log it
                let result = Self::parse_json_response(&json_str);
                if let LspParseResult::Response(ref response) = result {
                    let response_id = match &response.id {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    log_lsp_message!(Level::DEBUG, "incoming", &response_id, response);
                }
                result
            }
            Err(e) => LspParseResult::ParseError(format!("Invalid UTF-8 in message: {e}")),
        }
    }

    /// Parse JSON string into LSP response
    fn parse_json_response(json_str: &str) -> LspParseResult {
        match serde_json::from_str::<JsonRpcResponse>(json_str) {
            Ok(response) => LspParseResult::Response(response),
            Err(e) => LspParseResult::ParseError(format!(
                "Invalid JSON response: {e} - Content: {json_str}"
            )),
        }
    }
}

pub struct LspClient {
    process: Child,
    pending_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    reader_task: Option<JoinHandle<()>>,
    stderr_task: Option<JoinHandle<()>>,
    shutdown_signal: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl LspClient {
    pub async fn start_clangd(
        clangd_path: &str,
        project_root: &std::path::Path,
        build_directory: &std::path::Path,
        indexing_state: Arc<Mutex<IndexingState>>,
    ) -> Result<Self, LspError> {
        info!(
            "Starting clangd process at: {} with project root: {}, build directory: {}",
            clangd_path,
            project_root.display(),
            build_directory.display()
        );

        let mut command = Command::new(clangd_path);
        command
            .arg("--background-index")
            .arg("--clang-tidy")
            .arg("--completion-style=detailed")
            .arg("--log=verbose")
            .arg("--query-driver=**")
            .arg(format!(
                "--compile-commands-dir={}",
                build_directory.display()
            ))
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut process = command.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LspError::ClangdNotFound
            } else {
                LspError::ProcessError(format!("Failed to start clangd: {e}"))
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

        let stderr = process
            .stderr
            .take()
            .ok_or_else(|| LspError::ProcessError("Failed to get stderr".to_string()))?;

        let pending_requests: Arc<
            Mutex<HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>,
        > = Arc::new(Mutex::new(HashMap::new()));
        let pending_requests_clone = pending_requests.clone();
        let stdin_clone = Arc::new(Mutex::new(stdin));
        let stdin_for_task = stdin_clone.clone();

        // Create shutdown signal
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let shutdown_signal = Arc::new(Mutex::new(Some(shutdown_tx)));

        // Start reading responses
        let indexing_state_clone = indexing_state.clone();
        let reader_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("LSP reader task received shutdown signal");
                        break;
                    }
                    result = LspMessageParser::parse_message(&mut reader) => {
                        match result {
                            LspParseResult::Response(response) => {
                                if !response.id.is_null() && response.method.is_none() {
                                    // Handle responses with IDs (regular request responses)
                                    let mut pending = pending_requests_clone.lock().await;

                                    // Extract the actual string value from the JSON Value
                                    let response_id = match &response.id {
                                        serde_json::Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    };

                                    if let Some(sender) = pending.remove(&response_id) {
                                        if sender.send(response).is_err() {
                                            warn!("Failed to send response to waiting request");
                                        }
                                    } else {
                                        warn!("Received response for unknown request ID: {}", response_id);
                                    }
                                } else if !response.id.is_null() && response.method.is_some() {
                                    // Handle requests from clangd (need to respond)
                                    if let Some(method) = &response.method {
                                        match method.as_str() {
                                            "window/workDoneProgress/create" => {
                                                // Accept the progress token
                                                info!("Accepting progress token request from clangd");
                                                // Send success response
                                                let response_message = serde_json::json!({
                                                    "jsonrpc": "2.0",
                                                    "id": response.id,
                                                    "result": null
                                                });
                                                // Send response to clangd
                                                if let Err(e) = Self::send_raw_message(&stdin_for_task, &response_message.to_string()).await {
                                                    warn!("Failed to send progress token response: {}", e);
                                                }
                                            }
                                            _ => {
                                                debug!("Unhandled request from clangd: {}", method);
                                            }
                                        }
                                    }
                                } else {
                                    // Handle notifications (empty ID and method present)
                                    Self::handle_notification(&response, &indexing_state_clone).await;
                                }
                            }
                            LspParseResult::Incomplete => {
                                // Continue reading - this is normal (headers, empty lines, etc.)
                                continue;
                            }
                            LspParseResult::ParseError(error) => {
                                warn!("LSP parse error: {}", error);
                                // Continue reading - we might recover from parse errors
                                continue;
                            }
                            LspParseResult::EndOfStream => {
                                info!("LSP client reached EOF, stopping reader task");
                                break;
                            }
                            LspParseResult::ReadError(error) => {
                                warn!("LSP read error: {}", error);
                                break;
                            }
                        }
                    }
                }
            }
            info!("LSP reader task terminated");
        });

        // Start stderr logging task to capture clangd logs
        let build_dir_for_log = build_directory.to_path_buf();
        let stderr_task = tokio::spawn(async move {
            let mut stderr_reader = BufReader::new(stderr);

            // Create or open the clangd log file in the build directory instead of current directory
            let log_file_path = build_dir_for_log.join("mcp-cpp-clangd.log");
            let mut log_file = match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path)
                .await
            {
                Ok(file) => {
                    info!(
                        "Clangd logs will be written to: {}",
                        log_file_path.display()
                    );
                    Some(file)
                }
                Err(e) => {
                    warn!(
                        "Failed to open clangd log file {}: {}. Logs will only go to stderr.",
                        log_file_path.display(),
                        e
                    );
                    None
                }
            };

            // Add a timestamp header to the log file
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
            if let Some(ref mut file) = log_file {
                if let Err(e) = file
                    .write_all(
                        format!("\n=== CLANGD SESSION STARTED: {timestamp} ===\n").as_bytes(),
                    )
                    .await
                {
                    warn!("Failed to write header to clangd log file: {}", e);
                }
            }

            let mut line = String::new();
            loop {
                line.clear();
                match stderr_reader.read_line(&mut line).await {
                    Ok(0) => {
                        info!("Clangd stderr stream ended");
                        break;
                    }
                    Ok(_) => {
                        // Write the line to the log file with timestamp
                        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S.%3f");
                        let log_entry = format!("[{timestamp}] {line}");

                        if let Some(ref mut file) = log_file {
                            if let Err(e) = file.write_all(log_entry.as_bytes()).await {
                                warn!("Failed to write to clangd log file: {}", e);
                            } else {
                                // Flush immediately for real-time logging
                                let _ = file.flush().await;
                            }
                        }

                        // Also log important messages to our tracing system
                        let line_trimmed = line.trim();
                        if !line_trimmed.is_empty() {
                            if line_trimmed.contains("error")
                                || line_trimmed.contains("Error")
                                || line_trimmed.contains("failed")
                                || line_trimmed.contains("Failed")
                            {
                                warn!("clangd stderr: {}", line_trimmed);
                            } else if line_trimmed.contains("Indexed")
                                || line_trimmed.contains("backgroundIndexProgress")
                                || line_trimmed.contains("compilation database")
                            {
                                info!("clangd stderr: {}", line_trimmed);
                            } else {
                                debug!("clangd stderr: {}", line_trimmed);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error reading clangd stderr: {}", e);
                        break;
                    }
                }
            }

            // Add session end marker
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
            if let Some(ref mut file) = log_file {
                if let Err(e) = file
                    .write_all(format!("=== CLANGD SESSION ENDED: {timestamp} ===\n\n").as_bytes())
                    .await
                {
                    warn!("Failed to write footer to clangd log file: {}", e);
                }
            }

            info!("Clangd stderr logging task terminated");
        });

        let client = Self {
            process,
            pending_requests,
            stdin: stdin_clone.clone(),
            reader_task: Some(reader_task),
            stderr_task: Some(stderr_task),
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
        let start = Instant::now();
        let request = JsonRpcRequest::new(method.clone(), params);
        let request_id = request.id.clone();

        log_lsp_message!(Level::DEBUG, "outgoing", &method, &request);

        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(request_id.clone(), tx);
        }

        self.send_message(&request).await?;

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => {
                log_timing!(
                    Level::DEBUG,
                    &format!("lsp_request_{method}"),
                    start.elapsed()
                );
                Ok(response)
            }
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
            method: method.clone(),
            params,
        };

        log_lsp_message!(Level::DEBUG, "outgoing", &method, &notification);

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
                if tx.send(()).is_err() {
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

                if exit_result.is_err() {
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

        // Step 3a: Wait for stderr task to finish (with timeout)
        if let Some(stderr_task) = self.stderr_task.take() {
            match tokio::time::timeout(std::time::Duration::from_secs(3), stderr_task).await {
                Ok(Ok(())) => {
                    info!("Stderr logging task terminated cleanly");
                }
                Ok(Err(e)) => {
                    shutdown_errors.push("Stderr task failed during shutdown");
                    warn!("Stderr task failed: {}", e);
                }
                Err(_) => {
                    shutdown_errors.push("Timeout waiting for stderr task");
                    warn!("Stderr task did not terminate within timeout");
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

    async fn send_raw_message(
        stdin: &Arc<Mutex<tokio::process::ChildStdin>>,
        message: &str,
    ) -> Result<(), LspError> {
        let content = format!("Content-Length: {}\r\n\r\n{}", message.len(), message);

        let mut stdin_guard = stdin.lock().await;
        stdin_guard
            .write_all(content.as_bytes())
            .await
            .map_err(|e| LspError::ProcessError(format!("Failed to write to stdin: {e}")))?;
        stdin_guard
            .flush()
            .await
            .map_err(|e| LspError::ProcessError(format!("Failed to flush stdin: {e}")))?;

        Ok(())
    }

    async fn handle_notification(
        response: &JsonRpcResponse,
        indexing_state: &Arc<Mutex<IndexingState>>,
    ) {
        // Handle notifications from clangd
        if let Some(method) = &response.method {
            match method.as_str() {
                "window/workDoneProgress/create" => {
                    // clangd requesting progress token
                    if let Some(params) = &response.params {
                        if let Some(token) = params.get("token") {
                            info!("Progress token requested: {}", token);
                            // We should send a response accepting the token, but we can't do that here
                            // since this is a notification handler. For now, just log it.
                        }
                    }
                }
                "$/progress" => {
                    // Progress updates
                    info!("🔔 LspClient::handle_notification() - Received $/progress notification");
                    if let Some(params) = &response.params {
                        info!(
                            "🔍 LspClient::handle_notification() - Progress params: {:?}",
                            params
                        );
                        if let (Some(token), Some(value)) =
                            (params.get("token"), params.get("value"))
                        {
                            if let Some(token_str) = token.as_str() {
                                info!(
                                    "🏷️  LspClient::handle_notification() - Progress token: {:?}",
                                    token_str
                                );
                                if token_str == "backgroundIndexProgress" {
                                    info!(
                                        "🎯 LspClient::handle_notification() - Matched backgroundIndexProgress token"
                                    );
                                    if let Some(kind) = value.get("kind").and_then(|k| k.as_str()) {
                                        info!(
                                            "📝 LspClient::handle_notification() - Progress kind: {:?}",
                                            kind
                                        );
                                        match kind {
                                            "begin" => {
                                                let title = value
                                                    .get("title")
                                                    .and_then(|t| t.as_str())
                                                    .map(|s| s.to_string());

                                                info!(
                                                    "🔍 LspClient::handle_notification() - Indexing started: {}",
                                                    title
                                                        .as_deref()
                                                        .unwrap_or("Background indexing")
                                                );

                                                // Update indexing state
                                                let mut state = indexing_state.lock().await;
                                                info!(
                                                    "🔄 LspClient::handle_notification() - Acquired indexing state lock, calling start_indexing()"
                                                );
                                                state.start_indexing(title);
                                            }
                                            "report" => {
                                                let message = value
                                                    .get("message")
                                                    .and_then(|m| m.as_str())
                                                    .map(|s| s.to_string());
                                                let percentage = value
                                                    .get("percentage")
                                                    .and_then(|p| p.as_u64())
                                                    .map(|p| p as u8);

                                                info!(
                                                    "📊 LspClient::handle_notification() - Indexing progress: {} ({}%)",
                                                    message.as_deref().unwrap_or("Processing"),
                                                    percentage.unwrap_or(0)
                                                );

                                                // Update indexing state
                                                let mut state = indexing_state.lock().await;
                                                info!(
                                                    "🔄 LspClient::handle_notification() - Acquired indexing state lock, calling update_progress()"
                                                );
                                                state.update_progress(message, percentage);
                                            }
                                            "end" => {
                                                info!(
                                                    "✅ LspClient::handle_notification() - Indexing completed!"
                                                );

                                                // Update indexing state
                                                let mut state = indexing_state.lock().await;
                                                info!(
                                                    "🔄 LspClient::handle_notification() - Acquired indexing state lock, calling complete_indexing()"
                                                );
                                                state.complete_indexing();
                                            }
                                            _ => {
                                                info!(
                                                    "❓ LspClient::handle_notification() - Unknown progress kind: {}",
                                                    kind
                                                );
                                            }
                                        }
                                    } else {
                                        info!(
                                            "⚠️  LspClient::handle_notification() - No 'kind' field in progress value"
                                        );
                                    }
                                } else {
                                    info!(
                                        "🚫 LspClient::handle_notification() - Token '{}' does not match 'backgroundIndexProgress'",
                                        token_str
                                    );
                                }
                            } else {
                                info!(
                                    "⚠️  LspClient::handle_notification() - Token is not a string"
                                );
                            }
                        } else {
                            info!(
                                "⚠️  LspClient::handle_notification() - Missing token or value in progress params"
                            );
                        }
                    } else {
                        info!(
                            "⚠️  LspClient::handle_notification() - No params in $/progress notification"
                        );
                    }
                }
                "textDocument/clangd.fileStatus" => {
                    // File status updates
                    if let Some(params) = &response.params {
                        if let (Some(uri), Some(state)) = (params.get("uri"), params.get("state")) {
                            if let (Some(uri_str), Some(state_str)) = (uri.as_str(), state.as_str())
                            {
                                let file_path = uri_str.replace("file://", "");
                                let filename = std::path::Path::new(&file_path)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown");
                                info!("📄 File status: {} - {}", filename, state_str);
                            }
                        }
                    }
                }
                "textDocument/publishDiagnostics" => {
                    // Diagnostic messages (errors, warnings, etc.)
                    if let Some(params) = &response.params {
                        if let (Some(uri), Some(diagnostics)) =
                            (params.get("uri"), params.get("diagnostics"))
                        {
                            if let (Some(uri_str), Some(diagnostics_array)) =
                                (uri.as_str(), diagnostics.as_array())
                            {
                                let file_path = uri_str.replace("file://", "");
                                let filename = std::path::Path::new(&file_path)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown");
                                if !diagnostics_array.is_empty() {
                                    info!(
                                        "⚠️  Diagnostics for {}: {} issues",
                                        filename,
                                        diagnostics_array.len()
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {
                    debug!("Unhandled notification: {}", method);
                }
            }
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

        // Abort the stderr task if it's still running
        if let Some(stderr_task) = self.stderr_task.take() {
            stderr_task.abort();
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[test]
    fn test_parse_content_length() {
        assert_eq!(
            LspMessageParser::parse_content_length("Content-Length: 123"),
            Some(123)
        );
        assert_eq!(
            LspMessageParser::parse_content_length("Content-Length:456"),
            Some(456)
        );
        assert_eq!(
            LspMessageParser::parse_content_length("Content-Length: invalid"),
            None
        );
        assert_eq!(
            LspMessageParser::parse_content_length("Other-Header: 123"),
            None
        );
        assert_eq!(LspMessageParser::parse_content_length(""), None);
    }

    #[test]
    fn test_parse_json_response() {
        let valid_json = r#"{"jsonrpc":"2.0","id":"1","result":{"success":true}}"#;
        match LspMessageParser::parse_json_response(valid_json) {
            LspParseResult::Response(response) => {
                assert_eq!(response.id, "1");
                assert_eq!(response.jsonrpc, "2.0");
                assert!(response.result.is_some());
            }
            _ => panic!("Expected Response variant"),
        }

        let invalid_json = "not json";
        match LspMessageParser::parse_json_response(invalid_json) {
            LspParseResult::ParseError(error) => {
                assert!(error.contains("Invalid JSON response"));
            }
            _ => panic!("Expected ParseError variant"),
        }
    }

    #[tokio::test]
    async fn test_parse_message_end_of_stream() {
        let empty_data: &[u8] = b"";
        let mut reader = BufReader::new(empty_data);

        match LspMessageParser::parse_message(&mut reader).await {
            LspParseResult::EndOfStream => {
                // Expected
            }
            other => panic!("Expected EndOfStream, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_message_content_length_header() {
        let data = b"Content-Length: 123\r\n";
        let mut reader = BufReader::new(data.as_slice());

        // This will try to read the message body and fail due to insufficient data,
        // but we're testing that the header parsing works
        match LspMessageParser::parse_message(&mut reader).await {
            LspParseResult::ReadError(_) => {
                // Expected - we don't have enough data for the full message
            }
            other => panic!("Expected ReadError due to incomplete message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_message_other_header() {
        let data = b"Other-Header: value\r\n";
        let mut reader = BufReader::new(data.as_slice());

        match LspMessageParser::parse_message(&mut reader).await {
            LspParseResult::Incomplete => {
                // Expected - non-Content-Length headers are skipped
            }
            other => panic!("Expected Incomplete, got {other:?}"),
        }
    }
}
