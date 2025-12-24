//! Symbol context analysis functionality - V2 ARCHITECTURE IMPLEMENTATION
//!
//! Complete rewrite using the v2 architecture modules:
//! - clangd/: Session management with builder pattern
//! - lsp/: Modern LSP client with traits
//! - project/: Extensible project/build system abstraction
//! - io/: Process and transport management

use rmcp::{
    ErrorData,
    model::{CallToolResult, Content},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

use crate::io::file_buffer::FileBufferError;
use crate::mcp_server::tools::lsp_helpers::{
    call_hierarchy::{CallHierarchy, get_call_hierarchy},
    definitions::{get_declarations, get_definitions},
    document_symbols::{SymbolContext, find_symbol_at_position_with_path, get_document_symbols},
    examples::get_examples,
    hover::get_hover_info,
    members::{Members, get_members_from_document_symbol},
    symbol_resolution::get_matching_symbol,
    type_hierarchy::{TypeHierarchy, get_type_hierarchy},
};
use crate::mcp_server::tools::utils;
use crate::project::index::IndexStatusView;
use crate::project::{ComponentSession, ProjectError, ProjectWorkspace};
use crate::symbol::{FileLocation, Symbol};

// ============================================================================
// Analyzer Error Type
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error("No symbols found for '{0}'")]
    NoSymbols(String),
    #[error("No data found for '{0}'")]
    NoData(String),
    #[error("File buffer error: {0}")]
    FileBuffer(#[from] FileBufferError),
    #[error("LSP error: {0}")]
    Lsp(#[from] crate::lsp::client::LspError),
    #[error("Clangd session error: {0}")]
    Session(#[from] crate::clangd::error::ClangdSessionError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Project error: {0}")]
    Project(#[from] ProjectError),
}

impl From<AnalyzerError> for ErrorData {
    fn from(err: AnalyzerError) -> Self {
        ErrorData::internal_error(err.to_string(), None)
    }
}

// ============================================================================
// MCP Tool Definition - PRESERVE EXACT EXTERNAL SCHEMA
// ============================================================================

/// Tool parameters for analyze_symbol_context
#[derive(Debug, ::serde::Serialize, ::serde::Deserialize)]
pub struct AnalyzeSymbolContextTool {
    /// The C++ SYMBOL NAME to analyze (NOT file paths, component names, or directory names).
    pub symbol: String,

    /// Build directory path containing compile_commands.json. STRONGLY RECOMMENDED: Use absolute paths from get_project_details output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_directory: Option<String>,

    /// Maximum number of usage examples to include in the analysis. OPTIONAL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_examples: Option<u32>,

    /// Location hint for disambiguating overloaded symbols. OPTIONAL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_hint: Option<String>,

    /// Timeout in seconds to wait for indexing completion (default: 20s, 0 = no wait)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_timeout: Option<u64>,

    // Note: The following parameters are accepted for compatibility but currently
    // not used - hierarchies and usage are determined automatically based on symbol type
    /// (Deprecated - automatic) Include type hierarchy analysis
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_type_hierarchy: Option<bool>,

    /// (Deprecated - automatic) Include call hierarchy analysis
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_call_hierarchy: Option<bool>,

    /// (Deprecated - automatic) Include usage pattern analysis
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_usage_patterns: Option<bool>,

    /// (Deprecated - automatic) Include class members analysis
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_members: Option<bool>,

    /// (Deprecated - automatic) Include code in examples
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_code: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzerResult {
    pub symbol: Symbol,
    pub query: String,
    pub definitions: Vec<FileLocation>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub declarations: Vec<FileLocation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_documentation: Option<String>,

    /// Detail information from DocumentSymbol (signature, type info, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// Usage examples showing how the symbol is used throughout the codebase
    pub examples: Vec<FileLocation>,

    /// Type hierarchy information for classes, structs, and interfaces
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_hierarchy: Option<TypeHierarchy>,

    /// Call hierarchy information for functions and methods
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_hierarchy: Option<CallHierarchy>,

    /// Callable members for classes and structs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Members>,

    /// Index status information when timeout occurred or no indexing wait
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_status: Option<IndexStatusView>,
}

impl AnalyzeSymbolContextTool {
    /// Check if a symbol represents a structural type (class or struct) that can contain members
    fn is_structural_type(symbol_kind: lsp_types::SymbolKind) -> bool {
        matches!(
            symbol_kind,
            lsp_types::SymbolKind::CLASS | lsp_types::SymbolKind::STRUCT
        )
    }

    /// Extract members from structural types if applicable
    fn extract_members_if_structural(
        symbol: &Symbol,
        matched_document_symbol: &Option<lsp_types::DocumentSymbol>,
        query_name: &str,
    ) -> Option<Members> {
        if Self::is_structural_type(symbol.kind) {
            if let Some(matched_ds) = matched_document_symbol {
                let members = get_members_from_document_symbol(matched_ds, &symbol.name);
                info!(
                    "Found members for '{}': {} methods, {} constructors, {} destructors, {} operators",
                    query_name,
                    members.methods.len(),
                    members.constructors.len(),
                    members.destructors.len(),
                    members.operators.len()
                );
                Some(members)
            } else {
                warn!("No matched document symbol available for member extraction");
                None
            }
        } else {
            None
        }
    }

    /// Check if a symbol represents a type that supports type hierarchies
    fn supports_type_hierarchy(symbol_kind: lsp_types::SymbolKind) -> bool {
        matches!(
            symbol_kind,
            lsp_types::SymbolKind::CLASS
                | lsp_types::SymbolKind::STRUCT
                | lsp_types::SymbolKind::INTERFACE
        )
    }

    async fn resolve_symbol_via_workspace_with_context(
        &self,
        component_session: &ComponentSession,
    ) -> Result<(Symbol, SymbolContext), ErrorData> {
        let workspace_symbol = get_matching_symbol(&self.symbol, component_session)
            .await
            .map_err(|err| {
                error!("Failed to get matching workspace symbol: {}", err);
                ErrorData::from(err)
            })?;

        let symbol = workspace_symbol.clone();

        let file_uri = crate::symbol::uri_from_pathbuf(&symbol.location.file_path);

        let document_symbols = get_document_symbols(component_session, file_uri)
            .await
            .map_err(ErrorData::from)?;

        let position: lsp_types::Position = symbol.location.range.start.into();

        let (doc_symbol, container_path) =
            find_symbol_at_position_with_path(&document_symbols, &position).ok_or_else(|| {
                ErrorData::invalid_params(
                    format!(
                        "Could not find document symbol for workspace symbol '{}'",
                        self.symbol
                    ),
                    None,
                )
            })?;

        let context = SymbolContext {
            document_symbol: doc_symbol.clone(),
            container_path,
        };

        Ok((symbol, context))
    }

    async fn resolve_symbol_context_at_location(
        &self,
        location: &FileLocation,
        component_session: &ComponentSession,
    ) -> Result<(Symbol, SymbolContext), ErrorData> {
        let file_uri = crate::symbol::uri_from_pathbuf(&location.file_path);

        let document_symbols = get_document_symbols(component_session, file_uri)
            .await
            .map_err(ErrorData::from)?;

        let position: lsp_types::Position = location.range.start.into();

        let (doc_symbol, container_path) =
            find_symbol_at_position_with_path(&document_symbols, &position).ok_or_else(|| {
                ErrorData::invalid_params(
                    format!(
                        "No symbol found at location {}",
                        location.to_compact_range()
                    ),
                    None,
                )
            })?;

        let mut symbol = Symbol::from((doc_symbol, location.file_path.as_path()));
        symbol.container_name = container_path.last().cloned();

        let context = SymbolContext {
            document_symbol: doc_symbol.clone(),
            container_path,
        };

        Ok((symbol, context))
    }

    /// Retrieves definitions and declarations for the symbol
    async fn get_definitions_and_declarations(
        &self,
        symbol_location: &crate::symbol::FileLocation,
        component_session: &ComponentSession,
    ) -> Result<(Vec<FileLocation>, Vec<FileLocation>), ErrorData> {
        let definitions = get_definitions(symbol_location, component_session).await?;
        info!(
            "Found {} definitions for '{}'",
            definitions.len(),
            self.symbol
        );

        let declarations = get_declarations(symbol_location, component_session).await?;
        info!(
            "Found {} declarations for '{}'",
            declarations.len(),
            self.symbol
        );

        Ok((definitions, declarations))
    }

    /// Retrieves hover documentation for the symbol
    async fn get_hover_documentation(
        &self,
        symbol_location: &crate::symbol::FileLocation,
        component_session: &ComponentSession,
    ) -> Option<String> {
        match get_hover_info(symbol_location, component_session).await {
            Ok(info) => Some(info),
            Err(err) => {
                warn!("Failed to get hover information: {}", err);
                None
            }
        }
    }

    /// Retrieves usage examples for the symbol
    async fn get_usage_examples(
        &self,
        symbol_location: &crate::symbol::FileLocation,
        component_session: &ComponentSession,
    ) -> Vec<FileLocation> {
        match get_examples(component_session, symbol_location, self.max_examples).await {
            Ok(examples) => {
                info!("Found {} examples for '{}'", examples.len(), self.symbol);
                examples
            }
            Err(err) => {
                warn!("Failed to get usage examples: {}", err);
                Vec::new()
            }
        }
    }

    /// Retrieves type and call hierarchies based on symbol type
    async fn get_hierarchies(
        &self,
        symbol: &Symbol,
        symbol_location: &crate::symbol::FileLocation,
        component_session: &ComponentSession,
    ) -> (Option<TypeHierarchy>, Option<CallHierarchy>) {
        let type_hierarchy = if Self::supports_type_hierarchy(symbol.kind) {
            match get_type_hierarchy(symbol_location, component_session).await {
                Ok(hierarchy) => {
                    info!(
                        "Found type hierarchy for '{}': {} supertypes, {} subtypes",
                        self.symbol,
                        hierarchy.supertypes.len(),
                        hierarchy.subtypes.len()
                    );
                    Some(hierarchy)
                }
                Err(err) => {
                    warn!("Failed to get type hierarchy: {}", err);
                    None
                }
            }
        } else {
            None
        };

        let call_hierarchy = match symbol.kind {
            lsp_types::SymbolKind::FUNCTION
            | lsp_types::SymbolKind::METHOD
            | lsp_types::SymbolKind::CONSTRUCTOR => {
                match get_call_hierarchy(symbol_location, component_session).await {
                    Ok(hierarchy) => {
                        info!(
                            "Found call hierarchy for '{}': {} callers, {} callees",
                            self.symbol,
                            hierarchy.callers.len(),
                            hierarchy.callees.len()
                        );
                        Some(hierarchy)
                    }
                    Err(err) => {
                        warn!("Failed to get call hierarchy: {}", err);
                        None
                    }
                }
            }
            _ => None,
        };

        (type_hierarchy, call_hierarchy)
    }

    /// V2 entry point - uses shared ClangdSession from server
    #[instrument(
        name = "analyze_symbol_context",
        skip(self, component_session, _workspace)
    )]
    pub async fn call_tool(
        &self,
        component_session: Arc<ComponentSession>,
        _workspace: &ProjectWorkspace,
    ) -> Result<CallToolResult, ErrorData> {
        info!(
            "Starting symbol analysis for '{}', location_hint={:?}, wait_timeout={:?}",
            self.symbol, self.location_hint, self.wait_timeout
        );

        // Selective indexing wait logic based on location_hint
        let index_status = utils::handle_selective_indexing_wait(
            &component_session,
            self.location_hint.is_some(), // Skip indexing for document-specific analysis (location hint provided)
            self.wait_timeout,
            if self.location_hint.is_some() {
                "Document-specific analysis"
            } else {
                "Workspace symbol resolution"
            },
        )
        .await;

        // Note: LSP session access is now handled by individual helper functions

        let (symbol, symbol_context) = match &self.location_hint {
            None => {
                self.resolve_symbol_via_workspace_with_context(&component_session)
                    .await?
            }
            Some(location_str) => {
                let location: FileLocation = location_str.parse().map_err(|e| {
                    ErrorData::invalid_params(
                        format!("Invalid location format '{}': {}", location_str, e),
                        None,
                    )
                })?;
                self.resolve_symbol_context_at_location(&location, &component_session)
                    .await?
            }
        };

        // Get definitions and declarations
        let (definitions, mut declarations) = self
            .get_definitions_and_declarations(&symbol.location, &component_session)
            .await?;

        // Deduplicate: if definitions == declarations, clear declarations
        if definitions == declarations {
            info!("Definitions and declarations are identical, clearing declarations");
            declarations.clear();
        }

        // Get hover information
        let hover = self
            .get_hover_documentation(&symbol.location, &component_session)
            .await;

        // Get usage examples
        let examples = self
            .get_usage_examples(&symbol.location, &component_session)
            .await;

        // Get hierarchies based on symbol type
        let (type_hierarchy, call_hierarchy) = self
            .get_hierarchies(&symbol, &symbol.location, &component_session)
            .await;

        let detail = symbol_context.document_symbol.detail.clone();

        if let Some(ref d) = detail {
            info!("Found detail for '{}': {}", self.symbol, d);
        }

        let members = Self::extract_members_if_structural(
            &symbol,
            &Some(symbol_context.document_symbol.clone()),
            &self.symbol,
        );

        let result = AnalyzerResult {
            symbol,
            query: self.symbol.clone(),
            hover_documentation: hover,
            detail,
            definitions,
            declarations,
            examples,
            type_hierarchy,
            call_hierarchy,
            members,
            index_status,
        };

        let output = serde_json::to_string_pretty(&result).map_err(AnalyzerError::from)?;
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_analyzer_with_real_clangd() {
        use super::*;

        // Create a test project first
        use crate::test_utils::integration::TestProject;
        let test_project = TestProject::new().await.unwrap();
        test_project.cmake_configure().await.unwrap();

        // Scan the test project to create a proper workspace with components
        use crate::project::{ProjectScanner, WorkspaceSession};
        let scanner = ProjectScanner::with_default_providers();
        let workspace = scanner
            .scan_project(&test_project.project_root, 3, None)
            .expect("Failed to scan test project");

        // Create a WorkspaceSession with test clangd path
        let clangd_path = crate::test_utils::get_test_clangd_path();
        let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path)
            .expect("Failed to create workspace session");
        // ComponentSession handles session management internally

        let tool = AnalyzeSymbolContextTool {
            symbol: "Math".to_string(),
            build_directory: None,
            max_examples: None,
            location_hint: None,
            wait_timeout: None,
            include_type_hierarchy: None,
            include_call_hierarchy: None,
            include_usage_patterns: None,
            include_members: None,
            include_code: None,
        };

        let component_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await
            .unwrap();
        let result = tool.call_tool(component_session, &workspace).await;

        // Check and log error if present
        if let Err(ref err) = result {
            error!("Failed to analyze symbol: {}", err);
        }

        assert!(result.is_ok());

        let call_result = result.unwrap();
        // Extract text from CallToolResult
        let text = match call_result.content.first().map(|c| &c.raw) {
            Some(rmcp::model::RawContent::Text(rmcp::model::RawTextContent { text, .. })) => text,
            _ => panic!("Expected TextContent in call_result"),
        };
        let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

        assert_eq!(analyzer_result.symbol.name, "Math");
        assert_eq!(analyzer_result.symbol.kind, lsp_types::SymbolKind::CLASS);
        assert_eq!(analyzer_result.query, "Math");

        info!("Found symbol: {:?}", analyzer_result.symbol);

        assert!(&analyzer_result.hover_documentation.is_some());

        if let Some(hover_doc) = &analyzer_result.hover_documentation {
            info!("Hover documentation: {}", hover_doc);
        }

        assert!(!analyzer_result.definitions.is_empty());
        // Note: declarations may be empty if they are identical to definitions

        for definition in &analyzer_result.definitions {
            info!(
                "Definition: {} at {}:{}",
                definition.to_compact_range(),
                definition.file_path.display(),
                definition.range.start.line + 1
            );
        }

        for declaration in &analyzer_result.declarations {
            info!(
                "Declaration: {} at {}:{}",
                declaration.to_compact_range(),
                declaration.file_path.display(),
                declaration.range.start.line + 1
            );
        }

        // Verify examples are collected
        info!("Found {} usage examples", analyzer_result.examples.len());
        for (i, example) in analyzer_result.examples.iter().enumerate() {
            info!(
                "Example {}: {} at {}:{}",
                i + 1,
                example.to_compact_range(),
                example.file_path.display(),
                example.range.start.line + 1
            );
        }

        // The Math class should have usage examples in main.cpp
        assert!(
            !analyzer_result.examples.is_empty(),
            "Should have usage examples"
        );
    }
}
