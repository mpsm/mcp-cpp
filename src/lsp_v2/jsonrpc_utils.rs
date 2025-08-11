//! JSON-RPC 2.0 protocol utilities
//!
//! Provides constants, helper functions, and utilities for working with
//! JSON-RPC 2.0 protocol as per https://www.jsonrpc.org/specification

use crate::lsp_v2::protocol::{JsonRpcErrorObject, JsonRpcResponse};
use serde_json::Value;

// ============================================================================
// JSON-RPC 2.0 Constants
// ============================================================================

/// JSON-RPC 2.0 version identifier
pub const JSONRPC_VERSION: &str = "2.0";

/// JSON-RPC 2.0 Error Codes (as per JSON-RPC specification)
/// https://www.jsonrpc.org/specification#error_object
#[allow(dead_code)]
pub mod error_codes {
    /// Parse error - Invalid JSON was received by the server.
    pub const PARSE_ERROR: i32 = -32700;

    /// Invalid Request - The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i32 = -32600;

    /// Method not found - The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;

    /// Invalid params - Invalid method parameter(s).
    pub const INVALID_PARAMS: i32 = -32602;

    /// Internal error - Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;

    /// Server error range start - Reserved for implementation-defined server-errors.
    pub const SERVER_ERROR_START: i32 = -32099;

    /// Server error range end - Reserved for implementation-defined server-errors.
    pub const SERVER_ERROR_END: i32 = -32000;
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
