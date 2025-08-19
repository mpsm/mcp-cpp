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
    let file_path = symbol_location
        .file_path
        .to_str()
        .ok_or_else(|| AnalyzerError::NoData("Invalid file path in symbol location".to_string()))?;
    let uri = format!("file://{}", file_path);
    let position = symbol_location.range.start;

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let client = session.client_mut();
    let hover_info = client
        .text_document_hover(uri.to_string(), position.into())
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
