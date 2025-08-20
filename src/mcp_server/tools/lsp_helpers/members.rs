//! Member extraction functionality for C++ classes and structs
//!
//! This module provides LSP-based member extraction capabilities that work with
//! clangd to analyze class and struct members including methods, constructors,
//! operators, and nested types. The implementation leverages the sophisticated
//! document symbols infrastructure for efficient hierarchical symbol analysis.

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::clangd::session::ClangdSession;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::mcp_server::tools::lsp_helpers::document_symbols::{
    extract_class_members, get_document_symbols,
};
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
    /// Methods (both instance and static methods)
    pub methods: Vec<Member>,
    /// Constructors
    pub constructors: Vec<Member>,
    /// Destructors
    pub destructors: Vec<Member>,
    /// Operator overloads
    pub operators: Vec<Member>,
}

// ============================================================================
// Public API
// ============================================================================

/// Extract callable members for a class or struct symbol using hierarchical document symbol analysis
///
/// This function performs comprehensive member extraction by analyzing the document symbol hierarchy
/// to identify methods, constructors, operators, and static functions within the specified class.
/// The analysis leverages clangd's hierarchical document symbols for efficient member discovery.
///
/// Note: For analyzer workflows, consider using `get_members_from_symbols` with pre-fetched
/// document symbols to avoid redundant LSP calls when the symbols are already available.
///
/// # Arguments
/// * `symbol_location` - Location information for the target symbol
/// * `session` - Active clangd session for LSP communication
/// * `target_name` - Name of the class or struct to analyze
///
/// # Returns
/// * `Ok(Members)` - Categorized member information including methods, constructors, and operators
/// * `Err(AnalyzerError)` - LSP error or symbol resolution failure
#[allow(dead_code)]
pub async fn get_members(
    symbol_location: &FileLocation,
    session: &mut ClangdSession,
    target_name: &str,
) -> Result<Members, AnalyzerError> {
    let uri = symbol_location.get_uri();

    // Get hierarchical document symbols using the advanced document symbols infrastructure
    let document_symbols = get_document_symbols(session, uri).await?;

    info!(
        "Analyzing {} document symbols for class '{}' members",
        document_symbols.len(),
        target_name
    );

    // Extract members using the sophisticated document symbol analysis
    let member_symbols = extract_class_members(&document_symbols, target_name);

    debug!(
        "Found {} callable members for class '{}'",
        member_symbols.len(),
        target_name
    );

    // Convert document symbols to structured member information
    let members = categorize_members_from_symbols(member_symbols, target_name);

    Ok(members)
}

/// Extract callable members from document symbols without LSP communication
///
/// This function performs comprehensive member extraction using document symbols that have
/// already been retrieved from the LSP server. The analysis operates entirely on the provided
/// symbol hierarchy without requiring active LSP communication or session management.
///
/// # Arguments
/// * `document_symbols` - Pre-fetched hierarchical document symbols from LSP
/// * `target_name` - Name of the class or struct to analyze
///
/// # Returns
/// * `Members` - Categorized member information including methods, constructors, and operators
#[allow(dead_code)]
pub fn get_members_from_symbols(
    document_symbols: &[lsp_types::DocumentSymbol],
    target_name: &str,
) -> Members {
    info!(
        "Analyzing {} document symbols for class '{}' members",
        document_symbols.len(),
        target_name
    );

    // Extract members using the sophisticated document symbol analysis
    let member_symbols = extract_class_members(document_symbols, target_name);

    debug!(
        "Found {} callable members for class '{}'",
        member_symbols.len(),
        target_name
    );

    // Convert document symbols to structured member information
    categorize_members_from_symbols(member_symbols, target_name)
}

/// Extract members directly from a single matched document symbol
///
/// This function extracts callable members directly from a matched document symbol,
/// avoiding the need to traverse all document symbols again when we already have
/// the target symbol. This is more efficient for analyzer workflows where the
/// matched document symbol is already available.
///
/// # Arguments
/// * `document_symbol` - The matched document symbol for the class/struct
/// * `target_name` - Name of the class or struct (used for categorization logic)
///
/// # Returns
/// * `Members` - Categorized member information including methods, constructors, and operators
pub fn get_members_from_document_symbol(
    document_symbol: &lsp_types::DocumentSymbol,
    target_name: &str,
) -> Members {
    debug!(
        "Extracting members directly from document symbol for class '{}'",
        target_name
    );

    // Get the children of the document symbol (these are the members)
    let member_symbols: Vec<&lsp_types::DocumentSymbol> = document_symbol
        .children
        .as_ref()
        .map(|children| children.iter().collect())
        .unwrap_or_default();

    debug!(
        "Found {} direct members for class '{}'",
        member_symbols.len(),
        target_name
    );

    // Convert document symbols to structured member information
    categorize_members_from_symbols(member_symbols, target_name)
}

// ============================================================================
// Member Categorization Logic
// ============================================================================

/// Convert document symbols to categorized member information using modern symbol analysis
///
/// This function processes the hierarchical document symbols extracted by the advanced
/// document symbols infrastructure and categorizes them into structured member types
/// (methods, constructors, destructors, operators) for comprehensive class analysis.
fn categorize_members_from_symbols(
    member_symbols: Vec<&lsp_types::DocumentSymbol>,
    target_name: &str,
) -> Members {
    let mut methods = Vec::new();
    let mut constructors = Vec::new();
    let mut destructors = Vec::new();
    let mut operators = Vec::new();

    debug!(
        "Categorizing {} member symbols for class '{}'",
        member_symbols.len(),
        target_name
    );

    for symbol in member_symbols {
        debug!(
            "Processing member: '{}' (kind: {:?}, detail: {:?})",
            symbol.name, symbol.kind, symbol.detail
        );

        let member = create_member_from_document_symbol(symbol);

        // Categorize member into appropriate type
        categorize_member(
            member,
            &mut methods,
            &mut constructors,
            &mut destructors,
            &mut operators,
        );
    }

    debug!(
        "Member categorization complete for '{}': {} methods, {} constructors, {} destructors, {} operators",
        target_name,
        methods.len(),
        constructors.len(),
        destructors.len(),
        operators.len()
    );

    Members {
        methods,
        constructors,
        destructors,
        operators,
    }
}

// ============================================================================
// Member Creation and Classification
// ============================================================================

/// Create a Member from a DocumentSymbol using hierarchical symbol information
///
/// This function converts a hierarchical document symbol into a structured Member
/// representation, extracting the symbol name, type classification, signature details,
/// and other relevant member characteristics for C++ class analysis.
fn create_member_from_document_symbol(symbol: &lsp_types::DocumentSymbol) -> Member {
    let member_type = classify_member_kind(&symbol.name, symbol.kind);

    Member {
        name: symbol.name.clone(),
        member_type,
        signature: symbol.detail.clone().unwrap_or_else(|| symbol.name.clone()),
        access: None, // Access level information is not typically available in DocumentSymbol
    }
}

/// Classify member kind based on symbol name and LSP SymbolKind
///
/// This function analyzes the symbol name and LSP type information to determine
/// the appropriate member classification (method, constructor, operator) for
/// structured member categorization in C++ class analysis.
fn classify_member_kind(name: &str, symbol_kind: lsp_types::SymbolKind) -> String {
    use lsp_types::SymbolKind;

    match symbol_kind {
        SymbolKind::CONSTRUCTOR => {
            if name.starts_with("~") {
                "destructor".to_string()
            } else {
                "constructor".to_string()
            }
        }
        SymbolKind::METHOD => {
            if name.starts_with("operator") {
                "operator".to_string()
            } else {
                "method".to_string()
            }
        }
        SymbolKind::FUNCTION => "method".to_string(), // Treat function symbols as regular methods
        _ => "method".to_string(),                    // Default fallback to method classification
    }
}

/// Categorize a Member into the appropriate collection based on member type
///
/// This function distributes Member objects into their respective categories
/// (methods, constructors, destructors, operators) based on the classified member type,
/// providing structured organization for comprehensive class member analysis.
fn categorize_member(
    member: Member,
    methods: &mut Vec<Member>,
    constructors: &mut Vec<Member>,
    destructors: &mut Vec<Member>,
    operators: &mut Vec<Member>,
) {
    match member.member_type.as_str() {
        "method" => methods.push(member),
        "constructor" => constructors.push(member),
        "destructor" => destructors.push(member),
        "operator" => operators.push(member),
        _ => methods.push(member), // Default to methods for unknown member types
    }
}
