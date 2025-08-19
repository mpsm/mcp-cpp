//! Definition and declaration analysis functionality for C++ symbols
//!
//! This module provides LSP-based definition and declaration analysis capabilities
//! that work with clangd to find where symbols are defined and declared, supporting
//! multiple locations and various LSP response formats.

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::io::file_manager::RealFileBufferManager;
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::{FileLineWithContents, FileLocation};

// ============================================================================
// Public API
// ============================================================================

/// Get declarations for a symbol with file contents
pub async fn get_declarations(
    session: &mut ClangdSession,
    file_buffer_manager: &mut RealFileBufferManager,
    symbol_location: &FileLocation,
) -> Result<Vec<FileLineWithContents>, AnalyzerError> {
    let declarations_locations = get_symbol_declarations(symbol_location, session).await?;

    let declarations = declarations_locations
        .iter()
        .map(|loc| {
            let file_line = loc.to_file_line();
            FileLineWithContents::new_from_file_line(&file_line, file_buffer_manager)
                .map_err(AnalyzerError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(declarations)
}

/// Get definitions for a symbol with file contents
pub async fn get_definitions(
    session: &mut ClangdSession,
    file_buffer_manager: &mut RealFileBufferManager,
    symbol_location: &FileLocation,
) -> Result<Vec<FileLineWithContents>, AnalyzerError> {
    let definitions_locations = get_symbol_definitions(symbol_location, session).await?;

    let definitions = definitions_locations
        .iter()
        .map(|loc| {
            let file_line = loc.to_file_line();
            FileLineWithContents::new_from_file_line(&file_line, file_buffer_manager)
                .map_err(AnalyzerError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(definitions)
}

// ============================================================================
// LSP Operation Helpers
// ============================================================================

/// Get the declaration locations of a symbol
pub async fn get_symbol_declarations(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
) -> Result<Vec<FileLocation>, AnalyzerError> {
    let file_path = symbol_location
        .file_path
        .to_str()
        .ok_or_else(|| AnalyzerError::NoData("Invalid file path in symbol location".to_string()))?;
    let uri = format!("file://{}", file_path);
    let position = symbol_location.range.start;

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let declaration = session
        .client_mut()
        .text_document_declaration(uri.to_string(), position.into())
        .await
        .map_err(AnalyzerError::from)?;

    goto_defdecl_response_to_file_locations(declaration)
}

/// Get the definition locations of a symbol
pub async fn get_symbol_definitions(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
) -> Result<Vec<FileLocation>, AnalyzerError> {
    let file_path = symbol_location
        .file_path
        .to_str()
        .ok_or_else(|| AnalyzerError::NoData("Invalid file path in symbol location".to_string()))?;
    let uri = format!("file://{}", file_path);
    let position = symbol_location.range.start;

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let definition = session
        .client_mut()
        .text_document_definition(uri.to_string(), position.into())
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
    match response {
        GotoDefinitionResponse::Scalar(loc) => Ok(vec![FileLocation::from(&loc)]),
        GotoDefinitionResponse::Array(locs) => Ok(locs.iter().map(FileLocation::from).collect()),
        GotoDefinitionResponse::Link(links) => Ok(links.iter().map(FileLocation::from).collect()),
    }
}
