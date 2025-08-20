//! Member extraction functionality for C++ classes and structs
//!
//! This module provides LSP-based member extraction capabilities that work with
//! clangd to analyze class and struct members including methods, constructors,
//! operators, and nested types.

use lsp_types::DocumentSymbolResponse;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::FileLocation;

// ============================================================================
// Member Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct Member {
    /// Member name
    pub name: String,
    /// Member type: "method", "constructor", "operator"
    pub member_type: String,
    /// Full function signature
    pub signature: String,
    /// Access level if available: "public", "private", "protected"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Members {
    /// Instance methods
    pub methods: Vec<Member>,
    /// Constructors and destructors
    pub constructors: Vec<Member>,
    /// Operator overloads
    pub operators: Vec<Member>,
    /// Static methods and functions
    pub static_methods: Vec<Member>,
}

// ============================================================================
// Public API
// ============================================================================

/// Get callable members for a class or struct symbol
pub async fn get_members(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
    target_name: &str,
) -> Result<Members, AnalyzerError> {
    let file_path = symbol_location
        .file_path
        .to_str()
        .ok_or_else(|| AnalyzerError::NoData("Invalid file path in symbol location".to_string()))?;
    let uri = format!("file://{}", file_path);

    session
        .ensure_file_ready(&symbol_location.file_path)
        .await?;

    let client = session.client_mut();

    // Get document symbols for the file containing the class/struct
    let document_symbols = client
        .text_document_document_symbol(uri)
        .await
        .map_err(AnalyzerError::from)?;

    trace!("Document symbols response: {:?}", document_symbols);

    // Extract members from the document symbols
    let members = extract_members_from_symbols(document_symbols, target_name)?;

    Ok(members)
}

// ============================================================================
// Member Extraction Logic
// ============================================================================

/// Extract members from DocumentSymbolResponse
fn extract_members_from_symbols(
    symbols: DocumentSymbolResponse,
    target_name: &str,
) -> Result<Members, AnalyzerError> {
    let mut methods = Vec::new();
    let mut constructors = Vec::new();
    let mut operators = Vec::new();
    let static_methods = Vec::new();

    match symbols {
        DocumentSymbolResponse::Nested(nested_symbols) => {
            debug!(
                "Processing nested document symbols, looking for type '{}'",
                target_name
            );
            debug!("Found {} top-level symbols", nested_symbols.len());

            // Recursively search for the target type in the nested hierarchy
            search_nested_symbols_for_members(
                &nested_symbols,
                target_name,
                &mut methods,
                &mut constructors,
                &mut operators,
            );
        }
        DocumentSymbolResponse::Flat(flat_symbols) => {
            debug!(
                "Processing flat document symbols, looking for members of type '{}'",
                target_name
            );
            debug!("Found {} flat symbols", flat_symbols.len());

            // For flat symbols, find members by container name
            for symbol in flat_symbols {
                if let Some(container) = &symbol.container_name {
                    debug!("Symbol '{}' has container '{}'", symbol.name, container);
                    if container == target_name
                        || container.ends_with(&format!("::{}", target_name))
                    {
                        debug!("Found member '{}' for type '{}'", symbol.name, target_name);
                        let member = create_member_from_symbol_info(&symbol);
                        categorize_member(member, &mut methods, &mut constructors, &mut operators);
                    }
                } else {
                    debug!("Symbol '{}' has no container", symbol.name);
                }
            }
        }
    }

    debug!(
        "Final counts: {} methods, {} constructors, {} operators, {} static methods",
        methods.len(),
        constructors.len(),
        operators.len(),
        static_methods.len()
    );

    Ok(Members {
        methods,
        constructors,
        operators,
        static_methods,
    })
}

/// Recursively search nested document symbols to find the target type and extract its members
fn search_nested_symbols_for_members(
    symbols: &[lsp_types::DocumentSymbol],
    target_name: &str,
    methods: &mut Vec<Member>,
    constructors: &mut Vec<Member>,
    operators: &mut Vec<Member>,
) {
    use lsp_types::SymbolKind;

    for symbol in symbols {
        debug!(
            "Checking symbol: '{}' (kind: {:?})",
            symbol.name, symbol.kind
        );

        // If this is the target type, extract its members
        if symbol.name == target_name
            && matches!(symbol.kind, SymbolKind::CLASS | SymbolKind::STRUCT)
        {
            debug!("Found target type '{}', extracting members", target_name);

            if let Some(children) = &symbol.children {
                debug!("Type '{}' has {} children", target_name, children.len());

                for child in children {
                    debug!("Checking child: '{}' (kind: {:?})", child.name, child.kind);

                    // Only process callable members
                    if matches!(
                        child.kind,
                        SymbolKind::METHOD
                            | SymbolKind::FUNCTION
                            | SymbolKind::CONSTRUCTOR
                            | SymbolKind::CLASS // Nested classes
                            | SymbolKind::STRUCT // Nested structs
                    ) {
                        let member = create_member_from_document_symbol(child);
                        debug!(
                            "Adding member: '{}' (type: '{}')",
                            member.name, member.member_type
                        );
                        categorize_member(member, methods, constructors, operators);

                        // If this is a nested class/struct, recursively process its members too
                        if matches!(child.kind, SymbolKind::CLASS | SymbolKind::STRUCT)
                            && let Some(nested_children) = &child.children
                        {
                            debug!(
                                "Processing nested type '{}' with {} children",
                                child.name,
                                nested_children.len()
                            );
                            for nested_child in nested_children {
                                if matches!(
                                    nested_child.kind,
                                    SymbolKind::METHOD
                                        | SymbolKind::FUNCTION
                                        | SymbolKind::CONSTRUCTOR
                                ) {
                                    let mut nested_member =
                                        create_member_from_document_symbol(nested_child);
                                    // Prefix with the nested type name for clarity
                                    nested_member.name =
                                        format!("{}::{}", child.name, nested_member.name);
                                    debug!(
                                        "Adding nested member: '{}' (type: '{}')",
                                        nested_member.name, nested_member.member_type
                                    );
                                    categorize_member(
                                        nested_member,
                                        methods,
                                        constructors,
                                        operators,
                                    );
                                }
                            }
                        }
                    } else {
                        debug!(
                            "Skipping non-callable member: '{}' (kind: {:?})",
                            child.name, child.kind
                        );
                    }
                }
            } else {
                debug!("Type '{}' has no children", target_name);
            }
            return; // Found the target type, no need to continue searching
        }

        // Recursively search nested symbols (e.g., inside namespaces)
        if let Some(children) = &symbol.children {
            debug!(
                "Recursively searching in '{}' namespace/container with {} children",
                symbol.name,
                children.len()
            );
            search_nested_symbols_for_members(
                children,
                target_name,
                methods,
                constructors,
                operators,
            );
        }
    }
}

// ============================================================================
// Member Creation and Classification
// ============================================================================

/// Create a Member from a DocumentSymbol (nested format)
fn create_member_from_document_symbol(symbol: &lsp_types::DocumentSymbol) -> Member {
    let member_type = classify_member_kind(&symbol.name, symbol.kind);

    Member {
        name: symbol.name.clone(),
        member_type,
        signature: symbol.detail.clone().unwrap_or_else(|| symbol.name.clone()),
        access: None, // Access level not typically available in DocumentSymbol
    }
}

/// Create a Member from a SymbolInformation (flat format)
fn create_member_from_symbol_info(symbol: &lsp_types::SymbolInformation) -> Member {
    let member_type = classify_member_kind(&symbol.name, symbol.kind);

    Member {
        name: symbol.name.clone(),
        member_type,
        signature: symbol.name.clone(), // SymbolInformation doesn't have detail
        access: None,
    }
}

/// Classify member kind based on name and SymbolKind
fn classify_member_kind(name: &str, symbol_kind: lsp_types::SymbolKind) -> String {
    use lsp_types::SymbolKind;

    match symbol_kind {
        SymbolKind::CONSTRUCTOR => "constructor".to_string(),
        SymbolKind::METHOD => {
            if name.starts_with("operator") {
                "operator".to_string()
            } else {
                "method".to_string()
            }
        }
        SymbolKind::FUNCTION => "method".to_string(), // Simplify: treat as regular method
        _ => "method".to_string(),                    // Fallback: treat as regular method
    }
}

/// Categorize a Member into the appropriate vector
fn categorize_member(
    member: Member,
    methods: &mut Vec<Member>,
    constructors: &mut Vec<Member>,
    operators: &mut Vec<Member>,
) {
    match member.member_type.as_str() {
        "method" => methods.push(member),
        "constructor" => constructors.push(member),
        "operator" => operators.push(member),
        _ => methods.push(member), // Default to methods for unknown types
    }
}
