//! JSON-RPC 2.0 protocol layer
//!
//! Implements JSON-RPC 2.0 protocol with request/response matching,
//! notification handling, and proper error management.

use crate::lsp_v2::framing::LspFraming;
use crate::lsp_v2::transport::Transport;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error, trace};

// ============================================================================
// JSON-RPC Types
// ============================================================================

/// JSON-RPC 2.0 request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,

    /// Request identifier
    pub id: serde_json::Value,

    /// Method name
    pub method: String,

    /// Optional parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,

    /// Request identifier (matches the request)
    pub id: serde_json::Value,

    /// Result (present if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// Error (present if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorObject>,
}

/// JSON-RPC 2.0 notification message (no response expected)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,

    /// Method name
    pub method: String,

    /// Optional parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorObject {
    /// Error code
    pub code: i32,

    /// Error message
    pub message: String,

    /// Optional additional data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ============================================================================
// JSON-RPC Errors
// ============================================================================

/// JSON-RPC error codes as defined in the specification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(dead_code)]
pub enum JsonRpcErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
}

#[allow(dead_code)]
impl JsonRpcErrorCode {
    /// Check if the given code is in the server error range (-32099 to -32000)
    pub fn is_server_error(code: i32) -> bool {
        (-32099..=-32000).contains(&code)
    }
}

/// JSON-RPC error type
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum JsonRpcError {
    #[error("JSON-RPC parse error: {0}")]
    ParseError(String),

    #[error("JSON-RPC invalid request: {0}")]
    InvalidRequest(String),

    #[error("JSON-RPC method not found: {0}")]
    MethodNotFound(String),

    #[error("JSON-RPC invalid params: {0}")]
    InvalidParams(String),

    #[error("JSON-RPC internal error: {0}")]
    InternalError(String),

    #[error("JSON-RPC server error ({code}): {message}")]
    Server {
        code: i32,
        message: String,
        data: Option<serde_json::Value>,
    },

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Serialization error: {0}")]
    Serialization(serde_json::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(serde_json::Error),

    #[error("Request timeout")]
    Timeout,

    #[error("Request was cancelled")]
    RequestCancelled,

    #[error("Missing result in response")]
    MissingResult,
}

// ============================================================================
// JSON-RPC Client
// ============================================================================

/// Type alias for notification handler to reduce complexity
type NotificationHandler = Arc<dyn Fn(JsonRpcNotification) + Send + Sync>;

/// JSON-RPC client with request/response correlation
#[allow(dead_code)]
pub struct JsonRpcClient<T: Transport> {
    /// Channel for sending outbound messages (requests and notifications)
    outbound_sender: mpsc::UnboundedSender<String>,

    /// Request ID counter
    request_id: AtomicU64,

    /// Pending requests waiting for responses
    pending_requests: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<JsonRpcResponse>>>>,

    /// Notification handler (shared with transport task)
    notification_handler: Arc<Mutex<Option<NotificationHandler>>>,

    /// Type parameter marker
    _phantom: std::marker::PhantomData<T>,
}

#[allow(dead_code)]
impl<T: Transport + 'static> JsonRpcClient<T> {
    /// Create a new JSON-RPC client
    pub fn new(transport: T) -> Self {
        let framed_transport = LspFraming::new(transport);
        let transport_arc = Arc::new(Mutex::new(framed_transport));
        let (outbound_sender, mut outbound_receiver) = mpsc::unbounded_channel::<String>();
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));

        // Notification handler shared with transport task
        let notification_handler = Arc::new(Mutex::new(None::<NotificationHandler>));
        let handler_clone = Arc::clone(&notification_handler);

        // Transport handler task for bidirectional communication
        let transport_clone = Arc::clone(&transport_arc);
        let pending_clone = Arc::clone(&pending_requests);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Outbound messages (prioritized)
                    Some(message) = outbound_receiver.recv() => {
                        let mut transport = transport_clone.lock().await;
                        if let Err(e) = transport.send(&message).await {
                            error!("Failed to send message: {}", e);
                            break;
                        }
                        // Release lock immediately
                        drop(transport);
                    }
                    // Inbound messages
                    result = async {
                        let mut transport = transport_clone.lock().await;
                        transport.receive().await
                    } => {
                        match result {
                            Ok(message) => {
                                let handler = handler_clone.lock().await.clone();
                                Self::process_inbound_message(message, &pending_clone, &handler).await;
                            }
                            Err(e) => {
                                error!("Failed to receive message: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
            trace!("Transport handler task finished");
        });

        Self {
            outbound_sender,
            request_id: AtomicU64::new(1),
            pending_requests,
            notification_handler,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set notification handler
    pub async fn on_notification<F>(&self, handler: F)
    where
        F: Fn(JsonRpcNotification) + Send + Sync + 'static,
    {
        *self.notification_handler.lock().await = Some(Arc::new(handler));
    }

    /// Process an inbound message (response or notification)
    async fn process_inbound_message(
        message: String,
        pending_requests: &Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<JsonRpcResponse>>>>,
        notification_handler: &Option<NotificationHandler>,
    ) {
        trace!("JsonRpcClient: Received message: {}", message);

        // Try to parse as response first
        if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&message) {
            if let Some(id) = response.id.as_u64() {
                let mut pending = pending_requests.lock().await;
                if let Some(sender) = pending.remove(&id) {
                    if sender.send(response).is_err() {
                        debug!("Response receiver dropped for request {}", id);
                    }
                } else {
                    debug!("Received response for unknown request {}", id);
                }
            }
        }
        // Try to parse as notification
        else if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(&message) {
            debug!("Received notification: {:?}", notification);
            if let Some(handler) = notification_handler {
                handler(notification);
            }
        }
        // Unknown message format
        else {
            debug!("Received unparseable message: {}", message);
        }
    }

    /// Send a JSON-RPC request with default timeout (30 seconds)
    pub async fn request<P, R>(
        &mut self,
        method: &str,
        params: Option<P>,
    ) -> Result<R, JsonRpcError>
    where
        P: serde::Serialize,
        R: for<'de> serde::Deserialize<'de>,
    {
        self.request_with_timeout(method, params, std::time::Duration::from_secs(30))
            .await
    }

    /// Send a JSON-RPC request without timeout (blocking until response)
    pub async fn request_blocking<P, R>(
        &mut self,
        method: &str,
        params: Option<P>,
    ) -> Result<R, JsonRpcError>
    where
        P: serde::Serialize,
        R: for<'de> serde::Deserialize<'de>,
    {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (response_sender, mut response_receiver) = mpsc::unbounded_channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, response_sender);
        }

        // Create request
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::Value::Number(serde_json::Number::from(id)),
            method: method.to_string(),
            params: params
                .map(|p| serde_json::to_value(p).map_err(JsonRpcError::Serialization))
                .transpose()?,
        };

        // Send request
        let request_json = serde_json::to_string(&request).map_err(JsonRpcError::Serialization)?;
        debug!("JsonRpcClient: Sending blocking request: {}", request_json);

        // Send through the channel
        self.outbound_sender
            .send(request_json)
            .map_err(|_| JsonRpcError::Transport("Outbound channel closed".to_string()))?;

        // Wait for response without timeout
        let response = match response_receiver.recv().await {
            Some(response) => response,
            None => {
                // Channel closed - clean up pending request
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                return Err(JsonRpcError::RequestCancelled);
            }
        };

        // Handle response
        if let Some(error) = response.error {
            return Err(JsonRpcError::Server {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }

        match response.result {
            Some(Value::Null) => {
                // Handle null results (e.g., LSP shutdown) by trying to deserialize null as R
                serde_json::from_value(Value::Null).map_err(JsonRpcError::Deserialization)
            }
            Some(result) => serde_json::from_value(result).map_err(JsonRpcError::Deserialization),
            None => Err(JsonRpcError::MissingResult),
        }
    }

    /// Send a JSON-RPC request with custom timeout
    pub async fn request_with_timeout<P, R>(
        &mut self,
        method: &str,
        params: Option<P>,
        timeout: std::time::Duration,
    ) -> Result<R, JsonRpcError>
    where
        P: serde::Serialize,
        R: for<'de> serde::Deserialize<'de>,
    {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (response_sender, mut response_receiver) = mpsc::unbounded_channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, response_sender);
        }

        // Create request
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::Number(serde_json::Number::from(id)),
            method: method.to_string(),
            params: params
                .map(|p| serde_json::to_value(p).map_err(JsonRpcError::Serialization))
                .transpose()?,
        };

        // Send request
        let request_json = serde_json::to_string(&request).map_err(JsonRpcError::Serialization)?;
        debug!("JsonRpcClient: Sending request: {}", request_json);

        // Send through the channel
        self.outbound_sender
            .send(request_json)
            .map_err(|_| JsonRpcError::Transport("Outbound channel closed".to_string()))?;

        // Wait for response with timeout
        let response_result = tokio::time::timeout(timeout, response_receiver.recv()).await;

        // Handle timeout - clean up pending request
        let response = match response_result {
            Ok(Some(response)) => response,
            Ok(None) => {
                // Channel closed - clean up pending request
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                return Err(JsonRpcError::RequestCancelled);
            }
            Err(_) => {
                // Timeout - clean up pending request
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                return Err(JsonRpcError::Timeout);
            }
        };

        // Handle response
        if let Some(error) = response.error {
            return Err(JsonRpcError::Server {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }

        match response.result {
            Some(Value::Null) => {
                // Handle null results (e.g., LSP shutdown) by trying to deserialize null as R
                serde_json::from_value(Value::Null).map_err(JsonRpcError::Deserialization)
            }
            Some(result) => serde_json::from_value(result).map_err(JsonRpcError::Deserialization),
            None => Err(JsonRpcError::MissingResult),
        }
    }

    /// Send a JSON-RPC notification
    pub async fn notify<P>(&mut self, method: &str, params: Option<P>) -> Result<(), JsonRpcError>
    where
        P: serde::Serialize,
    {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: params
                .map(|p| serde_json::to_value(p).map_err(JsonRpcError::Serialization))
                .transpose()?,
        };

        let notification_json =
            serde_json::to_string(&notification).map_err(JsonRpcError::Serialization)?;
        debug!("JsonRpcClient: Sending notification: {}", notification_json);

        // Send via channel
        self.outbound_sender
            .send(notification_json)
            .map_err(|_| JsonRpcError::Transport("Outbound channel closed".to_string()))?;

        Ok(())
    }

    /// Check if transport is connected
    pub async fn is_connected(&self) -> bool {
        // Check if our channel is still open
        !self.outbound_sender.is_closed()
    }

    /// Clean up all pending requests (e.g., during restart)
    pub async fn cleanup_pending_requests(&mut self) {
        let mut pending = self.pending_requests.lock().await;
        for (id, sender) in pending.drain() {
            debug!("JsonRpcClient: Cleaning up pending request ID {}", id);
            // Try to send a cancellation, but don't wait if the receiver is gone
            let _ = sender.send(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: Value::Number(serde_json::Number::from(id)),
                result: None,
                error: Some(JsonRpcErrorObject {
                    code: JsonRpcErrorCode::InternalError as i32,
                    message: "Request cancelled due to connection restart".to_string(),
                    data: None,
                }),
            });
        }
    }

    /// Close the connection
    pub async fn close(&mut self) -> Result<(), JsonRpcError> {
        // Clean up all pending requests first
        self.cleanup_pending_requests().await;

        // The transport handler will exit when the channel is closed
        // (which happens when this struct is dropped)
        Ok(())
    }
}
