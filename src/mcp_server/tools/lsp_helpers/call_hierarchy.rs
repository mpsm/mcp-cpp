//! Call hierarchy analysis functionality for C++ functions and methods
//!
//! This module provides LSP-based call hierarchy analysis capabilities that work with
//! clangd to analyze function call relationships including incoming calls (callers)
//! and outgoing calls (callees).

use serde::{Deserialize, Serialize};

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::FileLocation;

// ============================================================================
// Call Hierarchy Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct CallHierarchy {
    /// Functions that call this function (incoming calls)
    pub callers: Vec<String>,
    /// Functions that this function calls (outgoing calls)
    pub callees: Vec<String>,
}

// ============================================================================
// Public API
// ============================================================================

/// Get call hierarchy information for a symbol (functions and methods)
pub async fn get_call_hierarchy(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
) -> Result<CallHierarchy, AnalyzerError> {
    let uri = symbol_location.get_uri();
    let lsp_position: lsp_types::Position = symbol_location.range.start.into();

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let client = session.client_mut();

    // Prepare call hierarchy at the symbol location
    let call_hierarchy_items = client
        .text_document_prepare_call_hierarchy(uri, lsp_position)
        .await
        .map_err(AnalyzerError::from)?;

    // If we don't get any call hierarchy items, return empty hierarchy
    let call_hierarchy_item = if call_hierarchy_items.is_empty() {
        return Ok(CallHierarchy {
            callers: Vec::new(),
            callees: Vec::new(),
        });
    } else {
        call_hierarchy_items.into_iter().next().unwrap()
    };

    // Get incoming calls (callers)
    let callers = client
        .call_hierarchy_incoming_calls(call_hierarchy_item.clone())
        .await
        .map_err(AnalyzerError::from)?
        .into_iter()
        .map(|call| call.from.name)
        .collect();

    // Get outgoing calls (callees)
    let callees = client
        .call_hierarchy_outgoing_calls(call_hierarchy_item)
        .await
        .map_err(AnalyzerError::from)?
        .into_iter()
        .map(|call| call.to.name)
        .collect();

    Ok(CallHierarchy { callers, callees })
}
