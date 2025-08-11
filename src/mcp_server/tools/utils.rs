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
