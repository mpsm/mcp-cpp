//! Type hierarchy analysis functionality for C++ classes and structs
//!
//! This module provides LSP-based type hierarchy analysis capabilities that work with
//! clangd to analyze class inheritance relationships including supertypes (parent classes)
//! and subtypes (derived classes).

use crate::clangd::session::ClangdSessionTrait;
use serde::{Deserialize, Serialize};

use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::project::component_session::ComponentSession;
use crate::symbol::FileLocation;

// ============================================================================
// Type Hierarchy Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct TypeHierarchy {
    /// Parent classes/interfaces that this type inherits from
    pub supertypes: Vec<String>,
    /// Derived classes that inherit from this type
    pub subtypes: Vec<String>,
}

// ============================================================================
// Public API
// ============================================================================

/// Get type hierarchy information for a symbol (classes, structs, interfaces)
pub async fn get_type_hierarchy(
    symbol_location: &FileLocation,
    component_session: &ComponentSession,
) -> Result<TypeHierarchy, AnalyzerError> {
    let uri = symbol_location.get_uri();
    let lsp_position: lsp_types::Position = symbol_location.range.start.into();

    component_session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let mut session = component_session.lsp_session().await;
    let client = session.client_mut();

    // Prepare type hierarchy at the symbol location
    let hierarchy_items = client
        .text_document_prepare_type_hierarchy(uri, lsp_position)
        .await
        .map_err(AnalyzerError::from)?;

    // If we don't get any hierarchy items, return empty hierarchy
    let hierarchy_item = match hierarchy_items {
        Some(items) if !items.is_empty() => items.into_iter().next().unwrap(),
        _ => {
            return Ok(TypeHierarchy {
                supertypes: Vec::new(),
                subtypes: Vec::new(),
            });
        }
    };

    // Get supertypes (parent classes/interfaces)
    let supertypes = client
        .type_hierarchy_supertypes(hierarchy_item.clone())
        .await
        .map_err(AnalyzerError::from)?
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.name)
        .collect();

    // Get subtypes (derived classes)
    let subtypes = client
        .type_hierarchy_subtypes(hierarchy_item)
        .await
        .map_err(AnalyzerError::from)?
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.name)
        .collect();

    Ok(TypeHierarchy {
        supertypes,
        subtypes,
    })
}
