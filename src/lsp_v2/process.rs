//! Process management layer
//!
//! Handles external process lifecycle and stderr monitoring,
//! completely separate from transport concerns.

use crate::lsp_v2::transport::StdioTransport;
use async_trait::async_trait;
use std::io;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tracing::{error, info, trace};

// ============================================================================
// Stderr Monitoring Trait
// ============================================================================

/// Trait for monitoring stderr output from external processes
#[async_trait]
#[allow(dead_code)]
pub trait StderrMonitor: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Install a handler for stderr lines
    ///
    /// The handler will be called for each line received from stderr.
    /// Only one handler can be active at a time - installing a new handler
    /// will replace the previous one.
    fn on_stderr_line<F>(&mut self, handler: F)
    where
        F: Fn(String) + Send + Sync + 'static;

    /// Start monitoring stderr (if not already started)
    async fn start_monitoring(&mut self) -> Result<(), Self::Error>;

    /// Stop monitoring stderr
    async fn stop_monitoring(&mut self) -> Result<(), Self::Error>;

    /// Check if stderr monitoring is active
    fn is_monitoring(&self) -> bool;
}

// ============================================================================
// Process Management
// ============================================================================

/// Error types for process management
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum ProcessError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Process not started")]
    NotStarted,

    #[error("Process already started")]
    AlreadyStarted,

    #[error("Stdin not available")]
    StdinNotAvailable,

    #[error("Stdout not available")]
    StdoutNotAvailable,

    #[error("Stderr not available")]
    StderrNotAvailable,

    #[error("Process terminated unexpectedly")]
    ProcessTerminated,
}

/// Trait for managing external process lifecycle
#[async_trait]
#[allow(dead_code)]
pub trait ProcessManager: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start the external process
    async fn start(&mut self) -> Result<(), Self::Error>;

    /// Stop the external process (graceful shutdown)
    async fn stop(&mut self) -> Result<(), Self::Error>;

    /// Force kill the external process
    async fn kill(&mut self) -> Result<(), Self::Error>;

    /// Check if the process is currently running
    fn is_running(&self) -> bool;

    /// Get the process ID (if running)
    fn process_id(&self) -> Option<u32>;

    /// Create a stdio transport for communicating with the process
    /// This consumes the stdin/stdout from the process
    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error>;
}

/// Manages child processes spawned via Command
pub struct ChildProcessManager {
    /// Command to execute
    command: String,

    /// Command arguments
    args: Vec<String>,

    /// The spawned child process (if running)
    child: Option<Child>,

    /// Stderr handler
    stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,

    /// Stderr monitoring task handle
    stderr_task: Option<JoinHandle<()>>,

    /// Stderr monitoring status
    monitoring_stderr: bool,
}

#[allow(dead_code)]
impl ChildProcessManager {
    /// Create a new child process manager
    pub fn new(command: String, args: Vec<String>) -> Self {
        Self {
            command,
            args,
            child: None,
            stderr_handler: None,
            stderr_task: None,
            monitoring_stderr: false,
        }
    }

    /// Spawn the stderr monitoring task
    async fn spawn_stderr_monitor(&mut self) -> Result<(), ProcessError> {
        if self.monitoring_stderr {
            return Ok(());
        }

        let stderr = self
            .child
            .as_mut()
            .ok_or(ProcessError::NotStarted)?
            .stderr
            .take()
            .ok_or(ProcessError::StderrNotAvailable)?;

        if let Some(handler) = &self.stderr_handler {
            let handler = Arc::clone(handler);
            let task = tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();

                trace!("ChildProcessManager: Starting stderr monitoring");

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => {
                            // EOF reached
                            trace!("ChildProcessManager: stderr EOF reached");
                            break;
                        }
                        Ok(_) => {
                            let line_content = line.trim().to_string();
                            if !line_content.is_empty() {
                                trace!("ChildProcessManager: stderr line: {}", line_content);
                                handler(line_content);
                            }
                        }
                        Err(e) => {
                            error!("Failed to read from stderr: {}", e);
                            break;
                        }
                    }
                }

                trace!("ChildProcessManager: stderr monitoring finished");
            });

            self.stderr_task = Some(task);
            self.monitoring_stderr = true;
        }

        Ok(())
    }
}

#[async_trait]
impl ProcessManager for ChildProcessManager {
    type Error = ProcessError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.child.is_some() {
            return Err(ProcessError::AlreadyStarted);
        }

        info!("Starting process: {} {:?}", self.command, self.args);

        let child = Command::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        info!("Process started with PID: {:?}", child.id());

        self.child = Some(child);

        // Start stderr monitoring if handler is installed
        if self.stderr_handler.is_some() {
            self.spawn_stderr_monitor().await?;
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Self::Error> {
        if let Some(mut child) = self.child.take() {
            info!("Stopping process with PID: {:?}", child.id());

            // Stop stderr monitoring
            if let Some(task) = self.stderr_task.take() {
                task.abort();
                self.monitoring_stderr = false;
            }

            // Try graceful shutdown first
            if let Some(stdin) = child.stdin.as_mut() {
                // For LSP servers, we should send shutdown request first
                // But at this level, we just close stdin
                let _ = stdin;
            }

            // Wait for process to exit with timeout
            match tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await {
                Ok(Ok(status)) => {
                    info!("Process exited with status: {}", status);
                }
                Ok(Err(e)) => {
                    error!("Error waiting for process: {}", e);
                    return Err(ProcessError::Io(e));
                }
                Err(_) => {
                    // Timeout - force kill
                    info!("Process did not exit gracefully, killing");
                    child.kill().await?;
                    child.wait().await?;
                }
            }
        }

        Ok(())
    }

    async fn kill(&mut self) -> Result<(), Self::Error> {
        if let Some(mut child) = self.child.take() {
            info!("Killing process with PID: {:?}", child.id());

            // Stop stderr monitoring
            if let Some(task) = self.stderr_task.take() {
                task.abort();
                self.monitoring_stderr = false;
            }

            child.kill().await?;
            child.wait().await?;

            info!("Process killed");
        }

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.child.is_some()
    }

    fn process_id(&self) -> Option<u32> {
        self.child.as_ref().and_then(|child| child.id())
    }

    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error> {
        let child = self.child.as_mut().ok_or(ProcessError::NotStarted)?;

        let stdin = child.stdin.take().ok_or(ProcessError::StdinNotAvailable)?;

        let stdout = child
            .stdout
            .take()
            .ok_or(ProcessError::StdoutNotAvailable)?;

        // Create stdio transport
        Ok(StdioTransport::new(stdin, stdout))
    }
}

#[async_trait]
impl StderrMonitor for ChildProcessManager {
    type Error = ProcessError;

    fn on_stderr_line<F>(&mut self, handler: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.stderr_handler = Some(Arc::new(handler));
    }

    async fn start_monitoring(&mut self) -> Result<(), Self::Error> {
        self.spawn_stderr_monitor().await
    }

    async fn stop_monitoring(&mut self) -> Result<(), Self::Error> {
        if let Some(task) = self.stderr_task.take() {
            task.abort();
            self.monitoring_stderr = false;
        }
        Ok(())
    }

    fn is_monitoring(&self) -> bool {
        self.monitoring_stderr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_child_process_manager_lifecycle() {
        let mut manager = ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()]);

        assert!(!manager.is_running());
        assert!(manager.process_id().is_none());

        // Start process
        manager.start().await.unwrap();

        assert!(manager.is_running());
        assert!(manager.process_id().is_some());

        // Stop process
        manager.stop().await.unwrap();

        assert!(!manager.is_running());
    }

    #[tokio::test]
    async fn test_stderr_monitoring() {
        let mut manager = ChildProcessManager::new(
            "sh".to_string(),
            vec![
                "-c".to_string(),
                "echo 'error message' >&2; sleep 1".to_string(),
            ],
        );

        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_lines_clone = Arc::clone(&stderr_lines);

        manager.on_stderr_line(move |line| {
            if let Ok(mut lines) = stderr_lines_clone.lock() {
                lines.push(line);
            }
        });

        manager.start().await.unwrap();

        // Wait a bit for stderr to be captured
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        manager.stop().await.unwrap();

        let lines = stderr_lines.lock().unwrap();
        assert!(!lines.is_empty());
        assert_eq!(lines[0], "error message");
    }
}
