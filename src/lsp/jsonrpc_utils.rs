//! JSON-RPC 2.0 protocol utilities
//!
//! Provides constants, helper functions, and utilities for working with
//! JSON-RPC 2.0 protocol as per https://www.jsonrpc.org/specification

use crate::lsp::protocol::{JsonRpcErrorObject, JsonRpcResponse};
use serde_json::Value;

// ============================================================================
// JSON-RPC 2.0 Constants
// ============================================================================

/// JSON-RPC 2.0 version identifier
pub const JSONRPC_VERSION: &str = "2.0";

/// JSON-RPC 2.0 Error Codes (as per JSON-RPC specification)
/// https://www.jsonrpc.org/specification#error_object
pub mod error_codes {
    /// Method not found - The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;
}

// ============================================================================
// JSON-RPC Response Builders
// ============================================================================

/// Create a successful JSON-RPC response
pub fn success_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

/// Create a JSON-RPC error response
pub fn error_response(
    id: Value,
    code: i32,
    message: String,
    data: Option<Value>,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result: None,
        error: Some(JsonRpcErrorObject {
            code,
            message,
            data,
        }),
    }
}

/// Create a "method not found" error response
pub fn method_not_found_response(id: Value, method: &str) -> JsonRpcResponse {
    error_response(
        id,
        error_codes::METHOD_NOT_FOUND,
        format!("Method not found: {}", method),
        None,
    )
}

/// Create a null success response (for requests that return void)
pub fn null_success_response(id: Value) -> JsonRpcResponse {
    success_response(id, Value::Null)
}
