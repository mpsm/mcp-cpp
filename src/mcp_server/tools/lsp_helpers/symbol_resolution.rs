//! Symbol resolution functionality for finding symbols in C++ codebases
//!
//! This module provides LSP-based symbol resolution capabilities that work with
//! clangd to find and identify symbols based on user queries, handling ambiguous
//! matches and providing the best candidate symbol for analysis.

use tracing::debug;

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::Symbol;

// ============================================================================
// Public API
// ============================================================================

/// Get the first (best) matching symbol from the session based on the user query
pub async fn get_matching_symbol(
    symbol_query: &str,
    session: &mut ClangdSession,
) -> Result<Symbol, AnalyzerError> {
    // Use the LSP client to find symbols matching the provided name
    let symbols = session
        .client_mut()
        .workspace_symbols(symbol_query.to_string())
        .await
        .map_err(AnalyzerError::from)?;

    if symbols.is_empty() {
        return Err(AnalyzerError::NoSymbols(symbol_query.to_string()));
    }

    debug!(
        "Found {} symbols matching '{}'",
        symbols.len(),
        symbol_query
    );

    // Convert to our Symbol type and return the first as the best match
    Ok(symbols[0].clone().into())
}
