//! Common utilities for MCP tools

use tracing::{info, warn};

/// Default timeout for waiting for clangd indexing to complete
pub const INDEXING_WAIT_TIMEOUT_SECS: u64 = 30;

/// Wait for clangd indexing to complete with timeout
///
/// This helper function waits for clangd to finish indexing the codebase,
/// with a configurable timeout. It logs the result and continues execution
/// regardless of whether indexing completes successfully, fails, or times out.
///
/// # Arguments
/// * `index_monitor` - The index monitor from a ClangdSession
/// * `timeout_secs` - Optional timeout in seconds (defaults to INDEXING_WAIT_TIMEOUT_SECS)
pub async fn wait_for_indexing(
    index_monitor: &crate::clangd::index::IndexMonitor,
    timeout_secs: Option<u64>,
) {
    let timeout =
        std::time::Duration::from_secs(timeout_secs.unwrap_or(INDEXING_WAIT_TIMEOUT_SECS));

    info!("Waiting for clangd indexing to complete...");
    match tokio::time::timeout(timeout, index_monitor.wait_for_indexing_completion()).await {
        Ok(Ok(())) => info!("Indexing completed successfully"),
        Ok(Err(e)) => {
            warn!("Indexing wait failed (continuing anyway): {}", e);
        }
        Err(_) => {
            warn!(
                "Indexing wait timed out after {} seconds (continuing anyway)",
                timeout.as_secs()
            );
        }
    }
}

/// Helper function to serialize JSON content and handle errors gracefully
pub fn serialize_result(content: &serde_json::Value) -> String {
    serde_json::to_string_pretty(content)
        .unwrap_or_else(|e| format!("Error serializing result: {e}"))
}

/// Converts LSP numeric symbol kinds to string representations for MCP client compatibility.
///
/// This function takes LSP SymbolKind numeric values (1-26) and converts them to lowercase
/// string representations (e.g., 5 -> "class", 12 -> "function") for better readability
/// in MCP client responses.
///
/// # Arguments
/// * `symbols` - Vector of JSON symbol objects with numeric "kind" fields
///
/// # Returns
/// * Vector of JSON symbol objects with string "kind" fields
pub fn convert_symbol_kinds(symbols: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    symbols
        .into_iter()
        .map(|mut symbol| {
            if let Some(kind_num) = symbol.get("kind").and_then(|k| k.as_u64()) {
                // Convert numeric kind to strongly-typed SymbolKind enum via serde
                if let Ok(kind_enum) = serde_json::from_value::<lsp_types::SymbolKind>(
                    serde_json::Value::Number(serde_json::Number::from(kind_num)),
                ) {
                    let kind_str = format!("{:?}", kind_enum).to_lowercase();
                    symbol["kind"] = serde_json::Value::String(kind_str);
                }
            }
            symbol
        })
        .collect()
}
