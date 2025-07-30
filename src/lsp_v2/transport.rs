//! Transport layer - Pure I/O abstraction for message exchange
//!
//! This module provides the core transport abstraction that handles
//! bidirectional message exchange without knowledge of message format
//! or process management.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tracing::{error, trace};

/// Core transport trait for bidirectional message exchange
#[async_trait]
pub trait Transport: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Send a message (raw string)
    async fn send(&mut self, message: &str) -> Result<(), Self::Error>;

    /// Receive a message (raw string)  
    async fn receive(&mut self) -> Result<String, Self::Error>;

    /// Close the transport
    async fn close(&mut self) -> Result<(), Self::Error>;

    /// Check if transport is still active
    fn is_connected(&self) -> bool;
}

// ============================================================================
// Stdio Transport Implementation
// ============================================================================

/// Error types for stdio transport
#[derive(Debug, thiserror::Error)]
pub enum StdioTransportError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Transport is disconnected")]
    Disconnected,

    #[error("Channel error: {0}")]
    Channel(String),
}

/// Transport implementation using stdin/stdout streams
pub struct StdioTransport {
    /// Channel for sending messages to stdin
    stdin_sender: Option<mpsc::UnboundedSender<String>>,

    /// Channel for receiving messages from stdout
    stdout_receiver: Option<mpsc::UnboundedReceiver<String>>,

    /// Connection status
    connected: bool,
}

impl StdioTransport {
    /// Create a new StdioTransport from child process streams
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        let (stdin_sender, stdin_receiver) = mpsc::unbounded_channel();
        let (stdout_sender, stdout_receiver) = mpsc::unbounded_channel();

        // Spawn background task for stdin writing
        tokio::spawn(Self::stdin_writer_task(stdin, stdin_receiver));

        // Spawn background task for stdout reading
        tokio::spawn(Self::stdout_reader_task(stdout, stdout_sender));

        Self {
            stdin_sender: Some(stdin_sender),
            stdout_receiver: Some(stdout_receiver),
            connected: true,
        }
    }

    /// Background task that writes messages to stdin
    async fn stdin_writer_task(
        mut stdin: ChildStdin,
        mut receiver: mpsc::UnboundedReceiver<String>,
    ) {
        while let Some(message) = receiver.recv().await {
            trace!("StdioTransport: Writing to stdin: {}", message);

            if let Err(e) = stdin.write_all(message.as_bytes()).await {
                error!("Failed to write to stdin: {}", e);
                break;
            }

            if let Err(e) = stdin.flush().await {
                error!("Failed to flush stdin: {}", e);
                break;
            }
        }

        trace!("StdioTransport: stdin writer task finished");
    }

    /// Background task that reads messages from stdout
    async fn stdout_reader_task(stdout: ChildStdout, sender: mpsc::UnboundedSender<String>) {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF reached
                    trace!("StdioTransport: stdout reader reached EOF");
                    break;
                }
                Ok(_) => {
                    trace!("StdioTransport: Read from stdout: {}", line.trim());

                    if sender.send(line.clone()).is_err() {
                        trace!("StdioTransport: stdout receiver dropped, stopping reader");
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to read from stdout: {}", e);
                    break;
                }
            }
        }

        trace!("StdioTransport: stdout reader task finished");
    }
}

#[async_trait]
impl Transport for StdioTransport {
    type Error = StdioTransportError;

    async fn send(&mut self, message: &str) -> Result<(), Self::Error> {
        if !self.connected {
            return Err(StdioTransportError::Disconnected);
        }

        let sender = self
            .stdin_sender
            .as_ref()
            .ok_or(StdioTransportError::Disconnected)?;

        sender
            .send(message.to_string())
            .map_err(|e| StdioTransportError::Channel(e.to_string()))?;

        Ok(())
    }

    async fn receive(&mut self) -> Result<String, Self::Error> {
        if !self.connected {
            return Err(StdioTransportError::Disconnected);
        }

        let receiver = self
            .stdout_receiver
            .as_mut()
            .ok_or(StdioTransportError::Disconnected)?;

        receiver
            .recv()
            .await
            .ok_or(StdioTransportError::Disconnected)
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        self.stdin_sender.take();
        self.stdout_receiver.take();
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

// ============================================================================
// Mock Transport Implementation
// ============================================================================

/// Error type for mock transport
#[derive(Debug, thiserror::Error)]
pub enum MockTransportError {
    #[error("Transport is disconnected")]
    Disconnected,
    #[error("No more responses available")]
    NoMoreResponses,
}

/// Mock transport for testing - allows controlling sent/received messages
pub struct MockTransport {
    /// Messages that were sent via this transport
    sent_messages: Arc<Mutex<Vec<String>>>,

    /// Predefined responses to return when receive() is called
    responses: Arc<Mutex<VecDeque<String>>>,

    /// Connection status
    connected: bool,
}

#[allow(dead_code)]
impl MockTransport {
    /// Create a new mock transport
    pub fn new() -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::new())),
            connected: true,
        }
    }

    /// Create a mock transport with predefined responses
    pub fn with_responses(responses: Vec<String>) -> Self {
        let transport = Self::new();
        {
            let mut response_queue = transport.responses.lock().unwrap();
            response_queue.extend(responses);
        }
        transport
    }

    /// Add a response that will be returned by the next receive() call
    pub fn add_response(&mut self, response: String) {
        let mut responses = self.responses.lock().unwrap();
        responses.push_back(response);
    }

    /// Get all messages that were sent via this transport
    pub fn sent_messages(&self) -> Vec<String> {
        self.sent_messages.lock().unwrap().clone()
    }

    /// Clear all sent messages
    pub fn clear_sent_messages(&mut self) {
        self.sent_messages.lock().unwrap().clear();
    }

    /// Check if there are more responses available
    pub fn has_responses(&self) -> bool {
        !self.responses.lock().unwrap().is_empty()
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for MockTransport {
    type Error = MockTransportError;

    async fn send(&mut self, message: &str) -> Result<(), Self::Error> {
        if !self.connected {
            return Err(MockTransportError::Disconnected);
        }

        self.sent_messages.lock().unwrap().push(message.to_string());
        Ok(())
    }

    async fn receive(&mut self) -> Result<String, Self::Error> {
        if !self.connected {
            return Err(MockTransportError::Disconnected);
        }

        let mut responses = self.responses.lock().unwrap();
        responses
            .pop_front()
            .ok_or(MockTransportError::NoMoreResponses)
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use tokio::process::Command;

    #[tokio::test]
    async fn test_stdio_transport_echo() {
        // Spawn echo process for testing
        let mut child = Command::new("echo")
            .arg("hello world")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to spawn echo command");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        let mut transport = StdioTransport::new(stdin, stdout);

        // Read the output from echo
        let output = transport.receive().await.unwrap();
        assert_eq!(output.trim(), "hello world");

        assert!(transport.is_connected());

        // Clean up
        transport.close().await.unwrap();
        let _ = child.wait().await;
    }

    #[tokio::test]
    async fn test_mock_transport_send_receive() {
        let mut transport =
            MockTransport::with_responses(vec!["response1".to_string(), "response2".to_string()]);

        // Test sending
        transport.send("message1").await.unwrap();
        transport.send("message2").await.unwrap();

        // Test receiving
        let response1 = transport.receive().await.unwrap();
        assert_eq!(response1, "response1");

        let response2 = transport.receive().await.unwrap();
        assert_eq!(response2, "response2");

        // Test sent messages were recorded
        let sent = transport.sent_messages();
        assert_eq!(sent, vec!["message1", "message2"]);

        // Test no more responses
        assert!(transport.receive().await.is_err());
    }

    #[tokio::test]
    async fn test_mock_transport_disconnect() {
        let mut transport = MockTransport::new();

        assert!(transport.is_connected());

        transport.close().await.unwrap();

        assert!(!transport.is_connected());
        assert!(transport.send("test").await.is_err());
        assert!(transport.receive().await.is_err());
    }
}
