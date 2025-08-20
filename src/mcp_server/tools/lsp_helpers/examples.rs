//! Usage examples and reference analysis functionality for C++ symbols
//!
//! This module provides LSP-based usage example and reference analysis capabilities
//! that work with clangd to find real usage patterns of symbols throughout the
//! codebase, with configurable limits and file content extraction.

use tracing::debug;

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::io::file_manager::RealFileBufferManager;
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::{FileLineWithContents, FileLocation};

// ============================================================================
// Public API
// ============================================================================

/// Get usage examples for a symbol
pub async fn get_examples(
    session: &mut ClangdSession,
    file_buffer_manager: &mut RealFileBufferManager,
    symbol_location: &FileLocation,
    max_examples: Option<u32>,
) -> Result<Vec<FileLineWithContents>, AnalyzerError> {
    let uri = symbol_location.get_uri();
    let lsp_position: lsp_types::Position = symbol_location.range.start.into();

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    // Get references to the symbol (exclude declaration)
    let references = session
        .client_mut()
        .text_document_references(uri, lsp_position, false)
        .await
        .map_err(AnalyzerError::from)?;

    // Convert references to FileLocation
    let reference_locations: Vec<FileLocation> =
        references.iter().map(FileLocation::from).collect();

    // Apply max_examples limit if specified
    let locations_to_process = match max_examples {
        Some(max) => reference_locations.into_iter().take(max as usize).collect(),
        None => reference_locations,
    };

    // Extract code snippets for each reference (full line, trimmed)
    let examples = locations_to_process
        .iter()
        .filter_map(|loc| {
            let file_line = loc.to_file_line();
            match FileLineWithContents::new_from_file_line(&file_line, file_buffer_manager) {
                Ok(line_with_contents) => Some(line_with_contents),
                Err(err) => {
                    debug!("Failed to get contents for reference: {}", err);
                    None
                }
            }
        })
        .collect();

    Ok(examples)
}
