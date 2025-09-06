//! Usage examples and reference analysis functionality for C++ symbols
//!
//! This module provides LSP-based usage example and reference analysis capabilities
//! that work with clangd to find real usage patterns of symbols throughout the
//! codebase, with configurable limits.

use crate::clangd::session::ClangdSessionTrait;
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::project::component_session::ComponentSession;
use crate::symbol::FileLocation;

// ============================================================================
// Public API
// ============================================================================

/// Get usage examples for a symbol (returns locations only)
pub async fn get_examples(
    component_session: &ComponentSession,
    symbol_location: &FileLocation,
    max_examples: Option<u32>,
) -> Result<Vec<FileLocation>, AnalyzerError> {
    let uri = symbol_location.get_uri();
    let lsp_position: lsp_types::Position = symbol_location.range.start.into();

    // Ensure file is ready first
    component_session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    // Get LSP session and make the request
    let mut session = component_session.lsp_session().await;
    let references = session
        .client_mut()
        .text_document_references(uri, lsp_position, false)
        .await
        .map_err(AnalyzerError::from)?;

    // Convert references to FileLocation
    let reference_locations: Vec<FileLocation> =
        references.iter().map(FileLocation::from).collect();

    // Apply max_examples limit if specified
    let example_locations = match max_examples {
        Some(max) => reference_locations.into_iter().take(max as usize).collect(),
        None => reference_locations,
    };

    Ok(example_locations)
}
