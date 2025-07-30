//! Process management layer
//!
//! Handles external process lifecycle and stderr monitoring,
//! completely separate from transport concerns.

use crate::lsp_v2::transport::{StdioTransport, Transport};
use async_trait::async_trait;
use std::io;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
#[allow(unused_imports)] // warn! is used in Windows-specific code blocks
use tracing::{error, info, trace, warn};

// ============================================================================
// Process State Management
// ============================================================================

/// How to stop a process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum StopMode {
    /// Try graceful shutdown first (SIGTERM), then force kill if needed
    Graceful,
    /// Force kill immediately (SIGKILL)
    Force,
}

/// Process lifecycle states
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProcessState {
    /// Process has not been started yet
    NotStarted,
    /// Process is currently running
    Running { pid: u32 },
    /// Process has been stopped (either gracefully or forcefully)
    Stopped,
}

impl ProcessState {
    /// Get the process ID if the process is running
    pub fn pid(&self) -> Option<u32> {
        match self {
            ProcessState::Running { pid } => Some(*pid),
            _ => None,
        }
    }

    /// Check if the process is currently running
    pub fn is_running(&self) -> bool {
        matches!(self, ProcessState::Running { .. })
    }
}

// ============================================================================
// Process Exit Events
// ============================================================================

/// Event fired when process exits unexpectedly
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProcessExitEvent {
    /// Process ID that exited
    pub pid: Option<u32>,
    /// Exit code if available
    pub exit_code: Option<i32>,
    /// Timestamp when exit was detected
    pub timestamp: std::time::SystemTime,
}

// ============================================================================
// Process Restart Handler Trait
// ============================================================================

/// Trait for handling process exit events
#[async_trait]
#[allow(dead_code)]
pub trait ProcessExitHandler: Send + Sync {
    /// Called when process exits unexpectedly
    async fn on_process_exit(&self, event: ProcessExitEvent);
}

// ============================================================================
// Stderr Monitoring Trait
// ============================================================================

/// Trait for monitoring stderr output from external processes
#[allow(dead_code)]
pub trait StderrMonitor: Send + Sync {
    /// Install a handler for stderr lines
    ///
    /// The handler will be called for each line received from stderr.
    /// Only one handler can be active at a time - installing a new handler
    /// will replace the previous one.
    ///
    /// Note: Monitoring starts automatically when the process starts if a handler is installed.
    fn on_stderr_line<F>(&mut self, handler: F)
    where
        F: Fn(String) + Send + Sync + 'static;
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

    /// Stop the external process
    async fn stop(&mut self, mode: StopMode) -> Result<(), Self::Error>;

    /// Check if the process is currently running
    fn is_running(&self) -> bool;

    /// Get the process ID (if running)
    fn process_id(&self) -> Option<u32>;

    /// Create a stdio transport for communicating with the process
    /// This consumes the stdin/stdout from the process
    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error>;

    /// Install a process exit event handler
    fn on_process_exit<H>(&mut self, handler: H)
    where
        H: ProcessExitHandler + 'static;
}

/// Manages child processes spawned via Command
pub struct ChildProcessManager {
    /// Command to execute
    command: String,

    /// Command arguments
    args: Vec<String>,

    /// Thread-safe process state
    state: Arc<Mutex<ProcessState>>,

    /// Stdio transport (created when process starts)
    stdio_transport: Option<StdioTransport>,

    /// Stderr handler
    stderr_handler: Option<Arc<dyn Fn(String) + Send + Sync>>,

    /// Stderr monitoring task handle
    stderr_task: Option<JoinHandle<()>>,

    /// Process wait task handle (waits for child to exit)
    wait_task: Option<JoinHandle<()>>,

    /// Process exit event handler
    exit_handler: Option<Arc<dyn ProcessExitHandler>>,
}

#[allow(dead_code)]
impl ChildProcessManager {
    /// Create a new child process manager
    pub fn new(command: String, args: Vec<String>) -> Self {
        Self {
            command,
            args,
            state: Arc::new(Mutex::new(ProcessState::NotStarted)),
            stdio_transport: None,
            stderr_handler: None,
            stderr_task: None,
            wait_task: None,
            exit_handler: None,
        }
    }

    /// Get current process state (thread-safe)
    pub fn get_state(&self) -> ProcessState {
        self.state.lock().unwrap().clone()
    }

    /// Install a process exit event handler
    pub fn on_process_exit<H>(&mut self, handler: H)
    where
        H: ProcessExitHandler + 'static,
    {
        self.exit_handler = Some(Arc::new(handler));
    }

    /// Spawn the stderr monitoring task with a provided stderr pipe
    async fn spawn_stderr_monitor_with_pipe(
        &mut self,
        stderr: tokio::process::ChildStderr,
    ) -> Result<(), ProcessError> {
        // Only start if we don't already have a task running
        if self.stderr_task.is_some() {
            return Ok(());
        }

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
        }

        Ok(())
    }

    /// Spawn the wait task that monitors child process exit
    async fn spawn_wait_task(&mut self, mut child: Child) -> Result<(), ProcessError> {
        let current_pid = self.get_state().pid();
        let exit_handler = self.exit_handler.clone();
        let state = Arc::clone(&self.state);

        let task = tokio::spawn(async move {
            trace!(
                "ChildProcessManager: Starting wait task for PID {:?}",
                current_pid
            );

            // Wait for the child process to exit
            match child.wait().await {
                Ok(exit_status) => {
                    info!(
                        "Process PID {:?} exited with status: {}",
                        current_pid, exit_status
                    );

                    // Transition state to Stopped
                    if let Ok(mut process_state) = state.lock() {
                        *process_state = ProcessState::Stopped;
                    }

                    // Fire exit event if handler is present
                    if let Some(handler) = &exit_handler {
                        let event = ProcessExitEvent {
                            pid: current_pid,
                            exit_code: exit_status.code(),
                            timestamp: std::time::SystemTime::now(),
                        };

                        handler.on_process_exit(event).await;
                    }
                }
                Err(e) => {
                    error!("Error waiting for child process: {}", e);

                    // Transition state to Stopped even on error
                    if let Ok(mut process_state) = state.lock() {
                        *process_state = ProcessState::Stopped;
                    }

                    // Fire exit event for error case too
                    if let Some(handler) = &exit_handler {
                        let event = ProcessExitEvent {
                            pid: current_pid,
                            exit_code: None,
                            timestamp: std::time::SystemTime::now(),
                        };

                        handler.on_process_exit(event).await;
                    }
                }
            }

            trace!(
                "ChildProcessManager: Wait task finished for PID {:?}",
                current_pid
            );
        });

        self.wait_task = Some(task);
        Ok(())
    }

    /// Check if process is actually running
    pub fn is_process_alive(&self) -> bool {
        let state = self.get_state();
        if let Some(pid) = state.pid() {
            #[cfg(unix)]
            {
                // Use kill(pid, 0) to check if process exists without sending signal
                unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
            }
            #[cfg(not(unix))]
            {
                // On Windows, this is more complex - for now just check if we have a PID
                // TODO: Implement proper Windows process checking
                true
            }
        } else {
            false
        }
    }
}

#[async_trait]
impl ProcessManager for ChildProcessManager {
    type Error = ProcessError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        // Simple check - don't start if already running
        if self.is_running() {
            return Err(ProcessError::AlreadyStarted);
        }

        info!("Starting process: {} {:?}", self.command, self.args);

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = child.id();
        info!("Process started with PID: {:?}", pid);

        // Update state to Running with PID
        if let Some(pid) = pid {
            *self.state.lock().unwrap() = ProcessState::Running { pid };
        } else {
            return Err(ProcessError::Io(std::io::Error::other(
                "Failed to get process ID",
            )));
        }

        // Extract stdio streams immediately before moving child to wait task
        let stdin = child.stdin.take().ok_or(ProcessError::StdinNotAvailable)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(ProcessError::StdoutNotAvailable)?;
        let stderr = child
            .stderr
            .take()
            .ok_or(ProcessError::StderrNotAvailable)?;

        // Create and store stdio transport
        self.stdio_transport = Some(StdioTransport::new(stdin, stdout));

        // Start stderr monitoring if handler is installed
        if self.stderr_handler.is_some() {
            self.spawn_stderr_monitor_with_pipe(stderr).await?;
        }

        // Start wait task with the child process (this consumes the child)
        self.spawn_wait_task(child).await?;

        Ok(())
    }

    async fn stop(&mut self, mode: StopMode) -> Result<(), Self::Error> {
        // Simply check if we have a running process
        let pid = match self.get_state().pid() {
            Some(pid) => pid,
            None => return Err(ProcessError::NotStarted),
        };

        match mode {
            StopMode::Graceful => info!("Gracefully stopping process with PID: {}", pid),
            StopMode::Force => info!("Force killing process with PID: {}", pid),
        }

        // Close stdio transport first (may trigger graceful shutdown)
        if let Some(mut transport) = self.stdio_transport.take() {
            let _ = transport.close().await; // Ignore errors during shutdown
        }

        // Send termination signal to process
        #[cfg(unix)]
        {
            unsafe {
                match mode {
                    StopMode::Graceful => {
                        // Send SIGTERM for graceful shutdown
                        if libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 {
                            info!("Sent SIGTERM to process {}", pid);
                        }
                        // Don't wait here - let the wait task detect exit naturally
                        // If process doesn't exit within reasonable time, orchestrator can call stop(Force)
                    }
                    StopMode::Force => {
                        // Force kill immediately
                        libc::kill(pid as libc::pid_t, libc::SIGKILL);
                        info!("Sent SIGKILL to process {}", pid);
                    }
                }
            }
        }
        #[cfg(not(unix))]
        {
            // On Windows, this is more complex - would need different approach
            warn!("Windows process termination not fully implemented");
        }

        // Stop stderr monitoring task
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }

        // Update state immediately for API consistency
        // The wait task will also update state when it detects the actual exit
        *self.state.lock().unwrap() = ProcessState::Stopped;

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.get_state().is_running()
    }

    fn process_id(&self) -> Option<u32> {
        self.get_state().pid()
    }

    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error> {
        self.stdio_transport.take().ok_or(ProcessError::NotStarted)
    }

    fn on_process_exit<H>(&mut self, handler: H)
    where
        H: ProcessExitHandler + 'static,
    {
        self.exit_handler = Some(Arc::new(handler));
    }
}

impl StderrMonitor for ChildProcessManager {
    fn on_stderr_line<F>(&mut self, handler: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.stderr_handler = Some(Arc::new(handler));
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
        manager.stop(StopMode::Graceful).await.unwrap();

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

        manager.stop(StopMode::Graceful).await.unwrap();

        let lines = stderr_lines.lock().unwrap();
        assert!(!lines.is_empty());
        assert_eq!(lines[0], "error message");
    }

    #[tokio::test]
    async fn test_process_state_transitions() {
        let mut manager = ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()]);

        // Initial state should be NotStarted
        assert_eq!(manager.get_state(), ProcessState::NotStarted);
        assert!(!manager.is_running());
        assert!(manager.process_id().is_none());

        // Start process - should transition to Running
        manager.start().await.unwrap();
        let running_state = manager.get_state();
        assert!(matches!(running_state, ProcessState::Running { .. }));
        assert!(manager.is_running());
        assert!(manager.process_id().is_some());

        // Stop process - should transition to Stopped
        manager.stop(StopMode::Graceful).await.unwrap();
        assert_eq!(manager.get_state(), ProcessState::Stopped);
        assert!(!manager.is_running());
        assert!(manager.process_id().is_none());
    }

    #[tokio::test]
    async fn test_invalid_operations() {
        let mut manager = ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()]);

        // Cannot stop when not started
        let result = manager.stop(StopMode::Graceful).await;
        assert!(matches!(result, Err(ProcessError::NotStarted)));

        // Start process
        manager.start().await.unwrap();

        // Cannot start when already running
        let result = manager.start().await;
        assert!(matches!(result, Err(ProcessError::AlreadyStarted)));

        // Stop process
        manager.stop(StopMode::Graceful).await.unwrap();

        // Can stop again - just returns NotStarted error (simple behavior)
        let result = manager.stop(StopMode::Graceful).await;
        assert!(matches!(result, Err(ProcessError::NotStarted)));
    }

    #[tokio::test]
    async fn test_create_transport_simple() {
        let mut manager = ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()]);

        // Cannot create transport when not started
        let result = manager.create_stdio_transport();
        assert!(matches!(result, Err(ProcessError::NotStarted)));

        // Start process
        manager.start().await.unwrap();

        // Should be able to create transport when running (consumes it)
        let _transport = manager.create_stdio_transport().unwrap();

        // Transport is consumed, so second call fails
        let result = manager.create_stdio_transport();
        assert!(matches!(result, Err(ProcessError::NotStarted)));
    }

    #[test]
    fn test_process_state_methods() {
        let not_started = ProcessState::NotStarted;
        assert!(!not_started.is_running());
        assert!(not_started.pid().is_none());

        let running = ProcessState::Running { pid: 12345 };
        assert!(running.is_running());
        assert_eq!(running.pid(), Some(12345));

        let stopped = ProcessState::Stopped;
        assert!(!stopped.is_running());
        assert!(stopped.pid().is_none());
    }
}
