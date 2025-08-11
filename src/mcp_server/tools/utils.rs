//! Common utilities for MCP tools

use lsp_types::{Location, Position, Range, Uri};
use std::str::FromStr;
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

/// Converts a single LSP numeric symbol kind to string representation.
///
/// # Arguments
/// * `kind_num` - Numeric LSP SymbolKind value (1-26)
///
/// # Returns
/// * String representation of the symbol kind (e.g., "class", "function")
#[allow(dead_code)]
pub fn convert_symbol_kind(kind_num: u64) -> String {
    if let Ok(kind_enum) = serde_json::from_value::<lsp_types::SymbolKind>(
        serde_json::Value::Number(serde_json::Number::from(kind_num)),
    ) {
        format!("{:?}", kind_enum).to_lowercase()
    } else {
        "unknown".to_string()
    }
}

// ============================================================================
// LSP Location Utilities
// ============================================================================

/// Creates a zero position (line 0, character 0) for default cases
pub fn zero_position() -> Position {
    Position::new(0, 0)
}

/// Creates a Position from line and character coordinates
///
/// This is a convenience wrapper around Position::new() for better readability
///
/// # Arguments
/// * `line` - Zero-based line number
/// * `character` - Zero-based character offset
///
/// # Returns
/// * Position instance
pub fn position(line: u32, character: u32) -> Position {
    Position::new(line, character)
}

/// Creates a Range from start and end positions
///
/// This is a convenience wrapper around Range::new() for better readability
///
/// # Arguments
/// * `start` - Start position
/// * `end` - End position
///
/// # Returns
/// * Range instance
#[allow(dead_code)]
pub fn range(start: Position, end: Position) -> Range {
    Range::new(start, end)
}

/// Creates a Range from line and character coordinates
///
/// # Arguments
/// * `start_line` - Zero-based start line number
/// * `start_char` - Zero-based start character offset
/// * `end_line` - Zero-based end line number  
/// * `end_char` - Zero-based end character offset
///
/// # Returns
/// * Range instance
#[allow(dead_code)]
pub fn range_from_coords(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Range {
    Range::new(
        Position::new(start_line, start_char),
        Position::new(end_line, end_char),
    )
}

/// Creates a zero-width range at a specific position (useful for cursor positions)
///
/// # Arguments
/// * `line` - Zero-based line number
/// * `character` - Zero-based character offset
///
/// # Returns
/// * Range instance where start and end are the same position
pub fn point_range(line: u32, character: u32) -> Range {
    let pos = Position::new(line, character);
    Range::new(pos, pos)
}

/// Creates a default zero range (0,0 to 0,0) for default cases
pub fn zero_range() -> Range {
    let zero_pos = zero_position();
    Range::new(zero_pos, zero_pos)
}

/// Creates a Location from URI and range
///
/// This is a convenience wrapper around Location::new() for better readability
///
/// # Arguments
/// * `uri` - File URI
/// * `range` - Range within the file
///
/// # Returns  
/// * Location instance
#[allow(dead_code)]
pub fn location(uri: Uri, range: Range) -> Location {
    Location::new(uri, range)
}

/// Creates a Location from URI string and coordinate values
///
/// # Arguments
/// * `uri_str` - File URI as string
/// * `start_line` - Zero-based start line number
/// * `start_char` - Zero-based start character offset
/// * `end_line` - Zero-based end line number
/// * `end_char` - Zero-based end character offset
///
/// # Returns
/// * Result containing Location instance or Uri parse error
#[allow(dead_code)]
pub fn location_from_coords(
    uri_str: &str,
    start_line: u32,
    start_char: u32,
    end_line: u32,
    end_char: u32,
) -> Result<Location, <Uri as FromStr>::Err> {
    let uri = Uri::from_str(uri_str)?;
    let range = range_from_coords(start_line, start_char, end_line, end_char);
    Ok(Location::new(uri, range))
}
