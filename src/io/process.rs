//! Process management layer
//!
//! Handles external process lifecycle and stderr monitoring,
//! completely separate from transport concerns.

use crate::io::transport::{StdioTransport, Transport};
use async_trait::async_trait;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
// warn! is used in Windows-specific code blocks
use tracing::{error, info, trace, warn};

// ============================================================================
// Process State Management
// ============================================================================

/// How to stop a process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub enum StopMode {
    /// Try graceful shutdown first (SIGTERM), then force kill if needed
    Graceful,
    /// Force kill immediately (SIGKILL)
    #[allow(dead_code)]
    Force,
}

/// Process lifecycle states
#[derive(Debug, Clone, PartialEq, Eq)]

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

pub struct ProcessExitEvent {}

// ============================================================================
// Process Restart Handler Trait
// ============================================================================

/// Trait for handling process exit events
#[async_trait]

pub trait ProcessExitHandler: Send + Sync {
    /// Called when process exits unexpectedly
    async fn on_process_exit(&self, event: ProcessExitEvent);
}

// ============================================================================
// Stderr Monitoring Trait
// ============================================================================

/// Trait for monitoring stderr output from external processes
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
}

/// Trait for managing external process lifecycle
#[async_trait]

pub trait ProcessManager: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start the external process
    async fn start(&mut self) -> Result<(), Self::Error>;

    /// Stop the external process
    async fn stop(&mut self, mode: StopMode) -> Result<(), Self::Error>;

    /// Check if the process is currently running
    fn is_running(&self) -> bool;

    /// Create a stdio transport for communicating with the process
    /// This consumes the stdin/stdout from the process
    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error>;

    /// Synchronous force kill for Drop trait implementations
    ///
    /// This is a simplified version of stop() that skips async transport cleanup
    /// and directly kills the process. Intended for use in Drop implementations.
    fn kill_sync(&mut self);
}

/// Manages child processes spawned via Command
pub struct ChildProcessManager {
    /// Command to execute
    command: String,

    /// Command arguments
    args: Vec<String>,

    /// Working directory for the process (optional)
    working_directory: Option<PathBuf>,

    /// Thread-safe process state
    state: Arc<Mutex<ProcessState>>,

    /// Stdio transport (created when process starts)
    stdio_transport: Option<StdioTransport>,

    /// Stderr handler
    stderr_handler: Option<Box<dyn Fn(String) + Send + Sync>>,

    /// Stderr monitoring task handle
    stderr_task: Option<JoinHandle<()>>,

    /// Process wait task handle (waits for child to exit)
    wait_task: Option<JoinHandle<()>>,

    /// Process exit event handler
    exit_handler: Option<Arc<dyn ProcessExitHandler>>,
}

impl ChildProcessManager {
    /// Create a new child process manager
    ///
    /// # Arguments
    /// * `command` - The command to execute
    /// * `args` - Command line arguments
    /// * `working_dir` - Optional working directory for the process
    pub fn new(command: String, args: Vec<String>, working_dir: Option<PathBuf>) -> Self {
        Self {
            command,
            args,
            working_directory: working_dir,
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
        // Intentional .unwrap() - poisoned mutex indicates serious bug, panic is appropriate
        self.state.lock().unwrap().clone()
    }

    /// Spawn the stderr monitoring task with a provided stderr pipe
    ///
    /// Always drains stderr to prevent child process from blocking.
    /// If a handler is installed, lines are forwarded to it.
    async fn spawn_stderr_monitor_with_pipe(
        &mut self,
        stderr: tokio::process::ChildStderr,
    ) -> Result<(), ProcessError> {
        // Only start if we don't already have a task running
        if self.stderr_task.is_some() {
            return Ok(());
        }

        // Move handler into task (take ownership, no cloning needed)
        let handler = self.stderr_handler.take();

        let task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            trace!(
                "ChildProcessManager: Starting stderr monitoring (handler: {})",
                if handler.is_some() {
                    "installed"
                } else {
                    "draining only"
                }
            );

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
                            if let Some(ref handler) = handler {
                                // Handler installed - forward the line (direct Box call, no Arc deref)
                                trace!("ChildProcessManager: stderr line: {}", line_content);
                                handler(line_content);
                            } else {
                                // No handler - just drain (optionally trace at debug level)
                                trace!("ChildProcessManager: stderr drained: {}", line_content);
                            }
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
                        let event = ProcessExitEvent {};

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
                        let event = ProcessExitEvent {};

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

        let mut command_builder = Command::new(&self.command);
        command_builder
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set working directory if specified
        if let Some(working_dir) = &self.working_directory {
            command_builder.current_dir(working_dir);
        }

        let mut child = command_builder.spawn()?;

        let pid = child.id();
        info!("Process started with PID: {:?}", pid);

        // Update state to Running with PID
        if let Some(pid) = pid {
            // Intentional .unwrap() - poisoned mutex indicates serious bug, panic is appropriate
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

        // Always start stderr monitoring to prevent clangd from blocking
        // Handler is optional - if not installed, lines are just drained
        self.spawn_stderr_monitor_with_pipe(stderr).await?;

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
        // Intentional .unwrap() - poisoned mutex indicates serious bug, panic is appropriate
        *self.state.lock().unwrap() = ProcessState::Stopped;

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.get_state().is_running()
    }

    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error> {
        self.stdio_transport.take().ok_or(ProcessError::NotStarted)
    }

    fn kill_sync(&mut self) {
        let pid = match self.get_state().pid() {
            Some(pid) => pid,
            None => return, // Already stopped
        };

        info!("Synchronously force killing process with PID: {}", pid);

        // Skip transport closure (async) - just kill the process directly
        #[cfg(unix)]
        {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
                info!("Sent SIGKILL to process {}", pid);
            }
        }

        #[cfg(not(unix))]
        {
            warn!("Windows sync process kill not implemented - process may remain");
        }

        // Stop stderr monitoring task
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }

        // Update state
        // Intentional .unwrap() - poisoned mutex indicates serious bug, panic is appropriate
        *self.state.lock().unwrap() = ProcessState::Stopped;

        // Note: Transport cleanup will happen when Drop is called on transport
        // Wait task will detect process exit and clean up naturally
    }
}

impl StderrMonitor for ChildProcessManager {
    fn on_stderr_line<F>(&mut self, handler: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.stderr_handler = Some(Box::new(handler));
    }
}

// ============================================================================
// Mock Process Manager (for testing)
// ============================================================================

/// Mock process manager for testing
#[cfg(test)]
#[allow(dead_code)]
pub struct MockProcessManager {
    running: bool,
    process_id: Option<u32>,
}

#[cfg(test)]
#[allow(dead_code)]
impl MockProcessManager {
    /// Create a new mock process manager
    pub fn new() -> Self {
        Self {
            running: false,
            process_id: None,
        }
    }
}

#[cfg(test)]
impl Default for MockProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[async_trait]
impl ProcessManager for MockProcessManager {
    type Error = ProcessError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.running {
            return Err(ProcessError::AlreadyStarted);
        }
        self.running = true;
        self.process_id = Some(12345); // Mock PID
        Ok(())
    }

    async fn stop(&mut self, _mode: StopMode) -> Result<(), Self::Error> {
        self.running = false;
        self.process_id = None;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn create_stdio_transport(&mut self) -> Result<StdioTransport, Self::Error> {
        // For testing purposes, we can't create a real StdioTransport
        // In practice, tests would use MockTransport directly
        Err(ProcessError::NotStarted)
    }

    fn kill_sync(&mut self) {
        // Mock implementation - simply reset state like stop()
        self.running = false;
        self.process_id = None;
    }
}

#[cfg(test)]
impl StderrMonitor for MockProcessManager {
    fn on_stderr_line<F>(&mut self, _callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        // Mock implementation - no-op since we don't have real stderr in tests
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_child_process_manager_lifecycle() {
        let mut manager =
            ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()], None);

        assert!(!manager.is_running());

        // Start process
        manager.start().await.unwrap();

        assert!(manager.is_running());

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
            None,
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
        let mut manager =
            ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()], None);

        // Initial state should be NotStarted
        assert_eq!(manager.get_state(), ProcessState::NotStarted);
        assert!(!manager.is_running());

        // Start process - should transition to Running
        manager.start().await.unwrap();
        let running_state = manager.get_state();
        assert!(matches!(running_state, ProcessState::Running { .. }));
        assert!(manager.is_running());

        // Stop process - should transition to Stopped
        manager.stop(StopMode::Graceful).await.unwrap();
        assert_eq!(manager.get_state(), ProcessState::Stopped);
        assert!(!manager.is_running());
    }

    #[tokio::test]
    async fn test_invalid_operations() {
        let mut manager =
            ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()], None);

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
        let mut manager =
            ChildProcessManager::new("echo".to_string(), vec!["hello".to_string()], None);

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
