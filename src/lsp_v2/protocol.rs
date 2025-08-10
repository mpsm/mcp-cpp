//! JSON-RPC 2.0 protocol layer
//!
//! Implements JSON-RPC 2.0 protocol with request/response matching,
//! notification handling, and proper error management.

use crate::io::transport::Transport;
use crate::lsp_v2::framing::LspFraming;
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

/// Type alias for request handler to reduce complexity
type RequestHandler = Arc<dyn Fn(JsonRpcRequest) -> JsonRpcResponse + Send + Sync>;

// ============================================================================
// Message Classification
// ============================================================================

/// Properly classified JSON-RPC message according to specification
#[derive(Debug, Clone)]
enum JsonRpcMessage {
    /// Request from client to server (has method + non-null id)
    Request {
        method: String,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    },
    /// Notification (has method, id is null or missing)
    Notification {
        method: String,
        params: Option<serde_json::Value>,
    },
    /// Response to our request (no method, has non-null id)
    Response {
        id: serde_json::Value,
        result: Option<serde_json::Value>,
        error: Option<JsonRpcErrorObject>,
    },
    /// Invalid message that couldn't be classified
    Invalid(String),
}

impl JsonRpcMessage {
    /// Classify a JSON-RPC message according to JSON-RPC 2.0 specification
    fn classify(message: &str) -> Self {
        let parsed = match serde_json::from_str::<serde_json::Value>(message) {
            Ok(value) => value,
            Err(e) => return Self::Invalid(format!("JSON parse error: {e}")),
        };

        let method = parsed
            .get("method")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());
        let id = parsed.get("id").cloned();
        let params = parsed.get("params").cloned();

        match (method, id) {
            // Request: has method + non-null id
            (Some(method), Some(id)) if !id.is_null() => Self::Request { method, id, params },
            // Notification: has method, id is null or missing
            (Some(method), _) => Self::Notification { method, params },
            // Response: no method, has non-null id
            (None, Some(id)) if !id.is_null() => {
                let result = parsed.get("result").cloned();
                let error = parsed
                    .get("error")
                    .and_then(|e| serde_json::from_value::<JsonRpcErrorObject>(e.clone()).ok());
                Self::Response { id, result, error }
            }
            // Invalid: doesn't match any pattern
            _ => Self::Invalid("Missing required fields or invalid structure".to_string()),
        }
    }
}

// ============================================================================
// Client State Management
// ============================================================================

/// Unified client state to eliminate cloning and multiple mutexes
#[derive(Default)]
struct ClientState {
    /// Handler for incoming notifications
    notification_handler: Option<NotificationHandler>,
    /// Handler for incoming requests from server
    request_handler: Option<RequestHandler>,
    /// Pending requests waiting for responses
    pending_requests: HashMap<u64, mpsc::UnboundedSender<JsonRpcResponse>>,
}

/// JSON-RPC client with request/response correlation
#[allow(dead_code)]
pub struct JsonRpcClient<T: Transport> {
    /// Channel for sending outbound messages (requests and notifications)
    outbound_sender: mpsc::UnboundedSender<String>,

    /// Request ID counter
    request_id: AtomicU64,

    /// Unified client state (single mutex instead of multiple)
    state: Arc<Mutex<ClientState>>,

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

        // Unified state - single Arc instead of multiple
        let state = Arc::new(Mutex::new(ClientState::default()));

        // Transport handler task - only clone what we actually need
        let transport_for_task = Arc::clone(&transport_arc);
        let state_for_task = Arc::clone(&state);
        let outbound_sender_for_task = outbound_sender.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Outbound messages (prioritized)
                    Some(message) = outbound_receiver.recv() => {
                        let mut transport = transport_for_task.lock().await;
                        if let Err(e) = transport.send(&message).await {
                            error!("Failed to send message: {}", e);
                            break;
                        }
                        // Release lock immediately
                        drop(transport);
                    }
                    // Inbound messages
                    result = async {
                        let mut transport = transport_for_task.lock().await;
                        transport.receive().await
                    } => {
                        match result {
                            Ok(message) => {
                                Self::process_inbound_message(message, &state_for_task, &outbound_sender_for_task).await;
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
            state,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set notification handler
    pub async fn on_notification<F>(&self, handler: F)
    where
        F: Fn(JsonRpcNotification) + Send + Sync + 'static,
    {
        let mut state = self.state.lock().await;
        state.notification_handler = Some(Arc::new(handler));
    }

    /// Set request handler
    pub async fn on_request<F>(&self, handler: F)
    where
        F: Fn(JsonRpcRequest) -> JsonRpcResponse + Send + Sync + 'static,
    {
        let mut state = self.state.lock().await;
        state.request_handler = Some(Arc::new(handler));
    }

    /// Process an inbound message (response, request, or notification)
    async fn process_inbound_message(
        message: String,
        state: &Arc<Mutex<ClientState>>,
        outbound_sender: &mpsc::UnboundedSender<String>,
    ) {
        trace!("JsonRpcClient: Received {} bytes", message.len());

        // Classify message using proper enum-based approach
        let classified_message = JsonRpcMessage::classify(&message);

        match classified_message {
            JsonRpcMessage::Request { method, id, params } => {
                debug!("Received request: {} with id: {:?}", method, id);

                // Get request handler (single lock acquisition)
                let request_handler = {
                    let state = state.lock().await;
                    state.request_handler.clone()
                };

                if let Some(handler) = request_handler {
                    let request = JsonRpcRequest {
                        jsonrpc: "2.0".to_string(),
                        method,
                        id: id.clone(),
                        params,
                    };

                    let response = handler(request);
                    let response_json = match serde_json::to_string(&response) {
                        Ok(json) => json,
                        Err(e) => {
                            debug!("Failed to serialize response: {}", e);
                            return;
                        }
                    };

                    if outbound_sender.send(response_json).is_err() {
                        debug!("Failed to send response back to server");
                    }
                } else {
                    debug!("No request handler registered for method: {}", method);
                }
            }

            JsonRpcMessage::Notification { method, params } => {
                debug!("Received notification: {}", method);

                // Get notification handler (single lock acquisition)
                let notification_handler = {
                    let state = state.lock().await;
                    state.notification_handler.clone()
                };

                if let Some(handler) = notification_handler {
                    let notification = JsonRpcNotification {
                        jsonrpc: "2.0".to_string(),
                        method,
                        params,
                    };
                    handler(notification);
                }
            }

            JsonRpcMessage::Response { id, result, error } => {
                // Try to match by ID (could be any JSON type, but we track as u64)
                if let Some(id_u64) = id.as_u64() {
                    let mut state = state.lock().await;
                    if let Some(sender) = state.pending_requests.remove(&id_u64) {
                        let response = JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result,
                            error,
                        };
                        if sender.send(response).is_err() {
                            debug!("Response receiver dropped for request {}", id_u64);
                        }
                    } else {
                        debug!("Received response for unknown request {}", id_u64);
                    }
                } else {
                    debug!("Response has non-numeric ID, cannot match: {:?}", id);
                }
            }

            JsonRpcMessage::Invalid(reason) => {
                debug!("Received invalid JSON-RPC message: {}", reason);
            }
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
            let mut state = self.state.lock().await;
            state.pending_requests.insert(id, response_sender);
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
                let mut state = self.state.lock().await;
                state.pending_requests.remove(&id);
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
            let mut state = self.state.lock().await;
            state.pending_requests.insert(id, response_sender);
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
                let mut state = self.state.lock().await;
                state.pending_requests.remove(&id);
                return Err(JsonRpcError::RequestCancelled);
            }
            Err(_) => {
                // Timeout - clean up pending request
                let mut state = self.state.lock().await;
                state.pending_requests.remove(&id);
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
        let mut state = self.state.lock().await;
        for (id, sender) in state.pending_requests.drain() {
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
