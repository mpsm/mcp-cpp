//! Transport layer - Pure I/O abstraction for message exchange
//!
//! This module provides the core transport abstraction that handles
//! bidirectional message exchange without knowledge of message format
//! or process management.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tracing::{error, trace};

// ============================================================================
// Constants
// ============================================================================

/// Size of the read buffer for stdout reading operations
const READ_BUFFER_SIZE: usize = 4096;

/// Default capacity for UTF-8 accumulation buffer
const UTF8_ACCUMULATION_BUFFER_CAPACITY: usize = 8192;

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
#[derive(Debug)]
pub struct StdioTransport {
    /// Channel for sending messages to stdin
    stdin_sender: Option<mpsc::UnboundedSender<String>>,

    /// Channel for receiving messages from stdout
    stdout_receiver: Option<mpsc::UnboundedReceiver<String>>,

    /// Connection status
    connected: bool,
}

/// Internal state for the stdout reader task that handles byte accumulation
struct StdoutReaderState {
    /// Buffer for accumulating raw bytes before UTF-8 conversion
    byte_buffer: Vec<u8>,

    /// Buffer capacity to avoid frequent reallocations  
    buffer_capacity: usize,
}

impl StdoutReaderState {
    /// Create new reader state with default capacity
    fn new() -> Self {
        Self {
            byte_buffer: Vec::with_capacity(UTF8_ACCUMULATION_BUFFER_CAPACITY),
            buffer_capacity: UTF8_ACCUMULATION_BUFFER_CAPACITY,
        }
    }

    /// Add new bytes to the accumulation buffer
    fn add_bytes(&mut self, bytes: &[u8]) {
        self.byte_buffer.extend_from_slice(bytes);
    }

    /// Find the longest valid UTF-8 prefix in the buffer
    /// Returns valid_bytes that can be safely converted to String
    fn extract_valid_utf8(&mut self) -> Option<Vec<u8>> {
        if self.byte_buffer.is_empty() {
            return None;
        }

        // Use standard library UTF-8 validation
        match std::str::from_utf8(&self.byte_buffer) {
            Ok(_) => {
                // All bytes are valid UTF-8 - extract everything
                Some(self.byte_buffer.drain(..).collect())
            }
            Err(e) => {
                let valid_end = e.valid_up_to();
                if valid_end == 0 {
                    // No complete UTF-8 characters available yet
                    // Wait for more data to complete the sequence
                    None
                } else {
                    // Extract valid prefix, keep remainder for later
                    Some(self.byte_buffer.drain(..valid_end).collect())
                }
            }
        }
    }

    /// Check if buffer is getting too large and needs capacity management
    fn should_compact(&self) -> bool {
        // If buffer is using more than 2x its intended capacity, compact it
        self.byte_buffer.capacity() > self.buffer_capacity * 2
    }

    /// Compact the buffer to reduce memory usage
    fn compact(&mut self) {
        if self.should_compact() {
            self.byte_buffer.shrink_to(self.buffer_capacity);
        }
    }
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
            trace!(
                "StdioTransport: Writing message (length: {})",
                message.len()
            );

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

    /// Background task that reads messages from stdout with byte-safe UTF-8 handling
    async fn stdout_reader_task(stdout: ChildStdout, sender: mpsc::UnboundedSender<String>) {
        let mut reader = BufReader::new(stdout);
        let mut state = StdoutReaderState::new();
        let mut read_buffer = Box::new([0u8; READ_BUFFER_SIZE]);

        loop {
            match reader.read(read_buffer.as_mut()).await {
                Ok(0) => {
                    // EOF reached
                    Self::handle_eof(&mut state, &sender);
                    break;
                }
                Ok(n) => {
                    // Successfully read bytes from stdout
                    state.add_bytes(&read_buffer[..n]);

                    // Extract and send all complete UTF-8 sequences
                    while let Some(valid_bytes) = state.extract_valid_utf8() {
                        match String::from_utf8(valid_bytes) {
                            Ok(data) => {
                                if sender.send(data).is_err() {
                                    trace!(
                                        "StdioTransport: stdout receiver dropped, stopping reader"
                                    );
                                    return;
                                }
                            }
                            Err(e) => {
                                error!(
                                    "StdioTransport: Failed to convert validated UTF-8 bytes: {}",
                                    e
                                );
                                // This should never happen since we validated the bytes
                                break;
                            }
                        }
                    }

                    // Periodic buffer management to prevent unbounded growth
                    state.compact();
                }
                Err(e) => {
                    error!("Failed to read from stdout: {}", e);
                    break;
                }
            }
        }

        trace!("StdioTransport: stdout reader task finished");
    }

    /// Handle EOF condition by processing any remaining valid UTF-8 bytes
    fn handle_eof(state: &mut StdoutReaderState, sender: &mpsc::UnboundedSender<String>) {
        trace!("StdioTransport: stdout reader reached EOF");

        // Try to extract any remaining valid UTF-8
        if let Some(final_bytes) = state.extract_valid_utf8() {
            // Convert remaining valid bytes to string
            match String::from_utf8(final_bytes) {
                Ok(final_string) => {
                    if !final_string.is_empty() && sender.send(final_string).is_err() {
                        trace!("StdioTransport: stdout receiver dropped during EOF processing");
                    }
                }
                Err(e) => {
                    error!("StdioTransport: Invalid UTF-8 in final bytes: {}", e);
                }
            }
        }

        // Check for incomplete UTF-8 sequences left in buffer
        if !state.byte_buffer.is_empty() {
            error!(
                "StdioTransport: {} incomplete bytes remaining at EOF: {:?}",
                state.byte_buffer.len(),
                state.byte_buffer
            );
        }
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
#[allow(dead_code)]
pub enum MockTransportError {
    #[error("Transport is disconnected")]
    Disconnected,
    #[error("No more responses available")]
    NoMoreResponses,
}

/// Mock transport for testing - allows controlling sent/received messages
#[allow(dead_code)]
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
        let mut transport = Self::new();
        transport.add_responses(responses);
        transport
    }

    /// Add multiple responses to the response queue
    fn add_responses(&mut self, responses: Vec<String>) {
        let mut response_queue = self.responses.lock().unwrap();
        response_queue.extend(responses);
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

    #[tokio::test]
    async fn test_stdout_reader_state_accumulation() {
        let mut state = StdoutReaderState::new();

        // Add partial UTF-8 sequence
        let partial_utf8 = &[0xE4, 0xB8]; // First 2 bytes of "世"
        state.add_bytes(partial_utf8);

        // Should not extract anything yet
        assert!(state.extract_valid_utf8().is_none());

        // Complete the sequence
        let completion = &[0x96]; // Final byte of "世"  
        state.add_bytes(completion);

        // Now should extract the complete character
        let extracted = state
            .extract_valid_utf8()
            .expect("Should extract complete UTF-8");
        let result = String::from_utf8(extracted).expect("Should be valid UTF-8");
        assert_eq!(result, "世");

        // Buffer should be empty now
        assert!(state.extract_valid_utf8().is_none());
        assert!(state.byte_buffer.is_empty());
    }

    #[tokio::test]
    async fn test_stdout_reader_mixed_boundaries() {
        let mut state = StdoutReaderState::new();

        // Simulate data that crosses UTF-8 boundaries
        let data1 = "Hello ".as_bytes(); // Complete ASCII
        let data2 = &[0xE4, 0xB8]; // Partial "世"
        let data3 = &[0x96, 0xE7, 0x95]; // Complete "世" + partial "界"  
        let data4 = &[0x8C, 0x20, 0xF0, 0x9F]; // Complete "界" + " " + partial 🌍
        let data5 = &[0x8C, 0x8D]; // Complete 🌍

        // Add data incrementally
        state.add_bytes(data1);
        let result1 = state.extract_valid_utf8().expect("Should extract 'Hello '");
        assert_eq!(String::from_utf8(result1).unwrap(), "Hello ");

        state.add_bytes(data2);
        assert!(state.extract_valid_utf8().is_none()); // Incomplete

        state.add_bytes(data3);
        let result2 = state.extract_valid_utf8().expect("Should extract '世'");
        assert_eq!(String::from_utf8(result2).unwrap(), "世");

        state.add_bytes(data4);
        let result3 = state.extract_valid_utf8().expect("Should extract '界 '");
        assert_eq!(String::from_utf8(result3).unwrap(), "界 ");

        state.add_bytes(data5);
        let result4 = state.extract_valid_utf8().expect("Should extract '🌍'");
        assert_eq!(String::from_utf8(result4).unwrap(), "🌍");

        assert!(state.byte_buffer.is_empty());
    }
}
