//! LSP message framing layer
//!
//! Handles LSP-specific message framing using Content-Length headers
//! as specified in the Language Server Protocol specification.
//!
//! LSP message framing format:
//! Content-Length: <length>\r\n\r\n<content>

use crate::io::transport::Transport;
use async_trait::async_trait;
use std::collections::VecDeque;
use tracing::trace;

/// Error types for LSP framing
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum LspFramingError<T: std::error::Error + Send + Sync + 'static> {
    #[error("Transport error: {0}")]
    Transport(T),

    #[error("Invalid LSP message format: {0}")]
    InvalidFormat(String),

    #[error("Invalid content length: {0}")]
    InvalidContentLength(String),

    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Incomplete message: expected {expected} bytes, got {actual}")]
    IncompleteMessage { expected: usize, actual: usize },
}

/// Maximum message size to prevent memory exhaustion
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16MB

/// LSP message framing wrapper
///
/// Wraps any transport to handle LSP message framing with Content-Length headers.
/// This allows the underlying transport to work with raw message strings while
/// this wrapper handles the LSP protocol specifics.
pub struct LspFraming<T: Transport> {
    /// Underlying transport
    transport: T,

    /// Buffer for accumulating partial messages
    receive_buffer: String,

    /// Queue of complete messages ready to be returned
    message_queue: VecDeque<String>,
}

#[allow(dead_code)]
impl<T: Transport> LspFraming<T> {
    /// Create a new LSP framing wrapper around a transport
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            receive_buffer: String::new(),
            message_queue: VecDeque::new(),
        }
    }

    /// Get a reference to the underlying transport
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Get a mutable reference to the underlying transport
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Unwrap and return the underlying transport
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Parse LSP message from the receive buffer
    ///
    /// Returns Some(message) if a complete message was parsed,
    /// None if more data is needed.
    fn try_parse_message(&mut self) -> Result<Option<String>, LspFramingError<T::Error>> {
        // Look for the header separator (\r\n\r\n)
        if let Some(header_end) = self.receive_buffer.find("\r\n\r\n") {
            let header = &self.receive_buffer[..header_end];
            let content_start = header_end + 4;

            // Parse Content-Length header
            let content_length = self.parse_content_length(header)?;

            // Check if we have enough data for the complete message
            let available_content = self.receive_buffer.len() - content_start;
            if available_content >= content_length {
                // Extract the message content
                let message =
                    self.receive_buffer[content_start..content_start + content_length].to_string();

                // Remove the processed message from buffer
                self.receive_buffer.drain(..content_start + content_length);

                trace!(
                    "LspFraming: Parsed complete message ({} bytes)",
                    content_length
                );
                return Ok(Some(message));
            } else {
                trace!(
                    "LspFraming: Incomplete message - need {} more bytes",
                    content_length - available_content
                );
            }
        }

        Ok(None)
    }

    /// Parse Content-Length from LSP headers
    fn parse_content_length(&self, header: &str) -> Result<usize, LspFramingError<T::Error>> {
        for line in header.lines() {
            if let Some(length_str) = line.strip_prefix("Content-Length:") {
                let length_str = length_str.trim();
                let length = length_str
                    .parse::<usize>()
                    .map_err(|_| LspFramingError::InvalidContentLength(length_str.to_string()))?;

                if length > MAX_MESSAGE_SIZE {
                    return Err(LspFramingError::MessageTooLarge {
                        size: length,
                        max: MAX_MESSAGE_SIZE,
                    });
                }

                return Ok(length);
            }
        }

        Err(LspFramingError::InvalidFormat(
            "Missing Content-Length header".to_string(),
        ))
    }

    /// Process data from transport and try to extract complete messages
    async fn process_transport_data(&mut self) -> Result<(), LspFramingError<T::Error>> {
        // Read data from transport
        let new_data = self
            .transport
            .receive()
            .await
            .map_err(LspFramingError::Transport)?;

        // Add to receive buffer
        self.receive_buffer.push_str(&new_data);

        // Try to parse messages from buffer
        while let Some(message) = self.try_parse_message()? {
            self.message_queue.push_back(message);
        }

        Ok(())
    }
}

#[async_trait]
impl<T: Transport> Transport for LspFraming<T> {
    type Error = LspFramingError<T::Error>;

    async fn send(&mut self, message: &str) -> Result<(), Self::Error> {
        // Frame the message with Content-Length header
        let framed_message = format!("Content-Length: {}\r\n\r\n{}", message.len(), message);

        trace!(
            "LspFraming: Sending framed message ({} bytes content)",
            message.len()
        );

        self.transport
            .send(&framed_message)
            .await
            .map_err(LspFramingError::Transport)
    }

    async fn receive(&mut self) -> Result<String, Self::Error> {
        // Return queued message if available
        if let Some(message) = self.message_queue.pop_front() {
            return Ok(message);
        }

        // Otherwise, read from transport until we have a complete message
        loop {
            self.process_transport_data().await?;

            if let Some(message) = self.message_queue.pop_front() {
                return Ok(message);
            }

            // If transport is disconnected and no messages in queue, we're done
            if !self.transport.is_connected() {
                return Err(LspFramingError::Transport(
                    // This is a bit hacky but we need to create a transport error
                    // In practice, this would be handled by the specific transport implementation
                    self.transport.receive().await.unwrap_err(),
                ));
            }
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.transport
            .close()
            .await
            .map_err(LspFramingError::Transport)
    }

    fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::transport::MockTransport;

    #[tokio::test]
    async fn test_lsp_framing_send() {
        let mock_transport = MockTransport::new();
        let mut framing = LspFraming::new(mock_transport);

        let message = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        framing.send(message).await.unwrap();

        let sent = framing.transport().sent_messages();
        assert_eq!(sent.len(), 1);

        let expected = format!("Content-Length: {}\r\n\r\n{}", message.len(), message);
        assert_eq!(sent[0], expected);
    }

    #[tokio::test]
    async fn test_lsp_framing_receive() {
        let message = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let framed_message = format!("Content-Length: {}\r\n\r\n{}", message.len(), message);

        let mock_transport = MockTransport::with_responses(vec![framed_message]);
        let mut framing = LspFraming::new(mock_transport);

        let received = framing.receive().await.unwrap();
        assert_eq!(received, message);
    }

    #[tokio::test]
    async fn test_lsp_framing_partial_message() {
        let message = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        let partial_content = &message[..10]; // Only part of the message
        let remaining_content = &message[10..];

        let mock_transport = MockTransport::with_responses(vec![
            format!("{}{}", header, partial_content),
            remaining_content.to_string(),
        ]);
        let mut framing = LspFraming::new(mock_transport);

        let received = framing.receive().await.unwrap();
        assert_eq!(received, message);
    }

    #[tokio::test]
    async fn test_lsp_framing_multiple_messages() {
        let message1 = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let message2 = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#;

        let combined = format!(
            "Content-Length: {}\r\n\r\n{}Content-Length: {}\r\n\r\n{}",
            message1.len(),
            message1,
            message2.len(),
            message2
        );

        let mock_transport = MockTransport::with_responses(vec![combined]);
        let mut framing = LspFraming::new(mock_transport);

        let received1 = framing.receive().await.unwrap();
        let received2 = framing.receive().await.unwrap();

        assert_eq!(received1, message1);
        assert_eq!(received2, message2);
    }

    #[tokio::test]
    async fn test_lsp_framing_invalid_content_length() {
        let invalid_message = "Content-Length: invalid\r\n\r\n{}";

        let mock_transport = MockTransport::with_responses(vec![invalid_message.to_string()]);
        let mut framing = LspFraming::new(mock_transport);

        let result = framing.receive().await;
        assert!(result.is_err());

        match result.unwrap_err() {
            LspFramingError::InvalidContentLength(_) => {}
            other => panic!("Expected InvalidContentLength error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_lsp_framing_message_too_large() {
        let large_size = MAX_MESSAGE_SIZE + 1;
        let invalid_message = format!("Content-Length: {large_size}\r\n\r\n");

        let mock_transport = MockTransport::with_responses(vec![invalid_message]);
        let mut framing = LspFraming::new(mock_transport);

        let result = framing.receive().await;
        assert!(result.is_err());

        match result.unwrap_err() {
            LspFramingError::MessageTooLarge { size, max } => {
                assert_eq!(size, large_size);
                assert_eq!(max, MAX_MESSAGE_SIZE);
            }
            other => panic!("Expected MessageTooLarge error, got: {other:?}"),
        }
    }
}
