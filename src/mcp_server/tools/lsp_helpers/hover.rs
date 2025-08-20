//! Hover documentation analysis functionality for C++ symbols
//!
//! This module provides LSP-based hover documentation analysis capabilities
//! that work with clangd to extract rich documentation, type information,
//! and signatures for symbols, supporting both markdown and plain text formats.

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::FileLocation;

// ============================================================================
// Public API
// ============================================================================

/// Get hover information for a symbol
pub async fn get_hover_info(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
) -> Result<String, AnalyzerError> {
    let uri = symbol_location.get_uri();
    let lsp_position: lsp_types::Position = symbol_location.range.start.into();

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let client = session.client_mut();
    let hover_info = client
        .text_document_hover(uri, lsp_position)
        .await
        .map_err(AnalyzerError::from)?;

    let markup = match hover_info {
        Some(lsp_types::Hover {
            contents: lsp_types::HoverContents::Markup(markup),
            ..
        }) => Some(markup),
        _ => None,
    };

    match markup {
        Some(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value,
        }) => Ok(value),
        Some(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::PlainText,
            value,
        }) => Ok(value),
        _ => Err(AnalyzerError::NoData(
            "No hover content available".to_string(),
        )),
    }
}
