//! Definition and declaration analysis functionality for C++ symbols
//!
//! This module provides LSP-based definition and declaration analysis capabilities
//! that work with clangd to find where symbols are defined and declared, supporting
//! multiple locations and various LSP response formats.

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::FileLocation;
use tracing::trace;

// ============================================================================
// Public API
// ============================================================================

/// Get the declaration locations of a symbol
pub async fn get_declarations(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
) -> Result<Vec<FileLocation>, AnalyzerError> {
    let uri = symbol_location.get_uri();
    let lsp_position: lsp_types::Position = symbol_location.range.start.into();

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let declaration = session
        .client_mut()
        .text_document_declaration(uri, lsp_position)
        .await
        .map_err(AnalyzerError::from)?;

    goto_defdecl_response_to_file_locations(declaration)
}

/// Get the definition locations of a symbol
pub async fn get_definitions(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
) -> Result<Vec<FileLocation>, AnalyzerError> {
    let uri = symbol_location.get_uri();
    let lsp_position: lsp_types::Position = symbol_location.range.start.into();

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let definition = session
        .client_mut()
        .text_document_definition(uri, lsp_position)
        .await
        .map_err(AnalyzerError::from)?;

    goto_defdecl_response_to_file_locations(definition)
}

// ============================================================================
// Response Processing Utilities
// ============================================================================

/// Convert LSP GotoDefinitionResponse to FileLocation vector
pub fn goto_defdecl_response_to_file_locations(
    response: lsp_types::GotoDefinitionResponse,
) -> Result<Vec<FileLocation>, AnalyzerError> {
    use lsp_types::GotoDefinitionResponse;
    trace!("Parsing GotoDefinitionReponse: {:?}", response);
    match response {
        GotoDefinitionResponse::Scalar(loc) => Ok(vec![FileLocation::from(&loc)]),
        GotoDefinitionResponse::Array(locs) => Ok(locs.iter().map(FileLocation::from).collect()),
        GotoDefinitionResponse::Link(links) => Ok(links.iter().map(FileLocation::from).collect()),
    }
}
