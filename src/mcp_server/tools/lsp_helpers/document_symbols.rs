//! Document symbols extraction functionality for C++ files
//!
//! This module provides LSP-based document symbol extraction that works with
//! clangd to get hierarchical symbol information from C++ files. Expects only
//! nested (hierarchical) responses due to client capabilities configuration.

use lsp_types::{DocumentSymbol, DocumentSymbolResponse, Position, Range};
use std::collections::VecDeque;
use std::path::Path;
use tracing::{trace, warn};

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::symbol::uri_from_pathbuf;

// ============================================================================
// SymbolContext - Rich symbol information with container hierarchy
// ============================================================================

/// Rich symbol context containing DocumentSymbol and container hierarchy
#[derive(Debug, Clone)]
pub struct SymbolContext {
    /// The LSP DocumentSymbol with full AST information
    pub document_symbol: lsp_types::DocumentSymbol,
    /// Container hierarchy path (e.g., ["Math", "Complex"] for Math::Complex::add)
    #[allow(dead_code)]
    pub container_path: Vec<String>,
}

// ============================================================================
// Traits for Idiomatic Symbol Tree Operations
// ============================================================================

/// Trait for types that can check if they contain a position
pub trait PositionContains {
    /// Check if this range/symbol contains the given Position
    fn contains(&self, position: &Position) -> bool;
}

impl PositionContains for Range {
    fn contains(&self, position: &Position) -> bool {
        (position.line > self.start.line
            || (position.line == self.start.line && position.character >= self.start.character))
            && (position.line < self.end.line
                || (position.line == self.end.line && position.character <= self.end.character))
    }
}

impl PositionContains for DocumentSymbol {
    fn contains(&self, position: &Position) -> bool {
        self.selection_range.contains(position)
    }
}

/// Trait for symbol matching strategies - extensible for future use
#[allow(dead_code)]
pub trait SymbolMatcher {
    fn matches(&self, symbol: &DocumentSymbol) -> bool;
}

/// Specialized matcher for class member symbols with filtering capabilities
#[allow(dead_code)]
pub struct MemberMatcher {
    /// Target class name to extract members from
    pub class_name: String,
    /// Optional filter by member kinds (method, constructor, etc.)
    pub member_kinds: Option<Vec<lsp_types::SymbolKind>>,
    /// Filter for static methods only
    pub static_only: bool,
}

#[allow(dead_code)]
impl MemberMatcher {
    /// Create a new member matcher for the specified class
    pub fn for_class(class_name: &str) -> Self {
        Self {
            class_name: class_name.to_string(),
            member_kinds: None,
            static_only: false,
        }
    }

    /// Filter by specific member kinds (METHOD, CONSTRUCTOR, etc.)
    pub fn with_kinds(mut self, kinds: Vec<lsp_types::SymbolKind>) -> Self {
        self.member_kinds = Some(kinds);
        self
    }

    /// Filter for static methods only
    pub fn static_only(mut self) -> Self {
        self.static_only = true;
        self
    }

    /// Check if a symbol represents a callable member
    fn is_callable_member(symbol: &DocumentSymbol) -> bool {
        matches!(
            symbol.kind,
            lsp_types::SymbolKind::METHOD
                | lsp_types::SymbolKind::FUNCTION
                | lsp_types::SymbolKind::CONSTRUCTOR
        )
    }

    /// Determine if this is a static method based on symbol detail
    fn is_static_method(symbol: &DocumentSymbol) -> bool {
        if let Some(detail) = &symbol.detail {
            detail.contains("static") || detail.starts_with("static ")
        } else {
            false
        }
    }
}

impl SymbolMatcher for MemberMatcher {
    fn matches(&self, symbol: &DocumentSymbol) -> bool {
        // Only match callable members
        if !Self::is_callable_member(symbol) {
            return false;
        }

        // Filter by member kinds if specified
        if let Some(ref kinds) = self.member_kinds
            && !kinds.contains(&symbol.kind)
        {
            return false;
        }

        // Filter for static methods if requested
        if self.static_only && !Self::is_static_method(symbol) {
            return false;
        }

        true
    }
}

// ============================================================================
// Iterator for Tree Traversal
// ============================================================================

/// Iterator over document symbols with path context
pub struct DocumentSymbolIterator<'a> {
    stack: VecDeque<(&'a DocumentSymbol, Vec<&'a str>)>,
}

impl<'a> DocumentSymbolIterator<'a> {
    /// Create a new iterator starting from root symbols
    pub fn new(symbols: &'a [DocumentSymbol]) -> Self {
        let mut stack = VecDeque::new();
        for symbol in symbols {
            stack.push_back((symbol, vec![]));
        }
        Self { stack }
    }

    /// Create iterator with depth-first traversal order
    #[allow(dead_code)]
    pub fn depth_first(symbols: &'a [DocumentSymbol]) -> Self {
        Self::new(symbols)
    }

    /// Create iterator with breadth-first traversal order
    #[allow(dead_code)]
    pub fn breadth_first(symbols: &'a [DocumentSymbol]) -> Self {
        let mut stack = VecDeque::new();
        for symbol in symbols {
            stack.push_front((symbol, vec![]));
        }
        Self { stack }
    }
}

impl<'a> Iterator for DocumentSymbolIterator<'a> {
    type Item = (&'a DocumentSymbol, Vec<&'a str>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((symbol, path)) = self.stack.pop_front() {
            // Add children to stack for future iteration (depth-first by default)
            if let Some(children) = &symbol.children {
                let mut new_path = path.clone();
                new_path.push(&symbol.name);

                // Insert children in reverse order for depth-first (or normal order for breadth-first)
                for child in children.iter().rev() {
                    self.stack.push_front((child, new_path.clone()));
                }
            }

            Some((symbol, path))
        } else {
            None
        }
    }
}

// ============================================================================
// Symbol Search Builder
// ============================================================================

/// Builder pattern for flexible symbol searching
#[derive(Debug, Clone)]
pub struct SymbolSearchBuilder {
    position: Option<Position>,
    name: Option<String>,
    kind: Option<lsp_types::SymbolKind>,
    path_contains: Option<String>,
}

impl SymbolSearchBuilder {
    /// Create a new search builder
    pub fn new() -> Self {
        Self {
            position: None,
            name: None,
            kind: None,
            path_contains: None,
        }
    }

    /// Search for symbol at specific position
    pub fn at_position(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }

    /// Search for symbol by name
    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Search for symbol by kind
    pub fn with_kind(mut self, kind: lsp_types::SymbolKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Search for symbols matching any of the given kinds
    pub fn with_kinds(mut self, kinds: &[lsp_types::SymbolKind]) -> Self {
        // For simplicity, just take the first kind
        // In practice, we'd need a more sophisticated approach for multiple kinds
        if let Some(&kind) = kinds.first() {
            self.kind = Some(kind);
        }
        self
    }

    /// Search for symbols whose path contains the given string
    #[allow(dead_code)]
    pub fn path_contains<S: Into<String>>(mut self, path_part: S) -> Self {
        self.path_contains = Some(path_part.into());
        self
    }

    /// Find the first matching symbol
    pub fn find_first(self, symbols: &[DocumentSymbol]) -> Option<&DocumentSymbol> {
        DocumentSymbolIterator::new(symbols)
            .find(|(symbol, path)| self.matches_symbol(symbol, path))
            .map(|(symbol, _)| symbol)
    }

    /// Find all matching symbols
    pub fn find_all(self, symbols: &[DocumentSymbol]) -> Vec<&DocumentSymbol> {
        DocumentSymbolIterator::new(symbols)
            .filter(|(symbol, path)| self.matches_symbol(symbol, path))
            .map(|(symbol, _)| symbol)
            .collect()
    }

    /// Filter for class members only (methods, constructors, operators)
    #[allow(dead_code)]
    pub fn class_members_only(mut self) -> Self {
        self.kind = None; // Reset single kind filter
        self
    }

    /// Filter for static methods only
    #[allow(dead_code)]
    pub fn static_methods_only(mut self) -> Self {
        self.kind = Some(lsp_types::SymbolKind::METHOD);
        self
    }

    /// Filter for constructors only
    #[allow(dead_code)]
    pub fn constructors_only(mut self) -> Self {
        self.kind = Some(lsp_types::SymbolKind::CONSTRUCTOR);
        self
    }

    /// Filter for methods within a specific class (by path)
    #[allow(dead_code)]
    pub fn methods_in_class<S: Into<String>>(mut self, class_name: S) -> Self {
        let class_name = class_name.into();
        self.path_contains = Some(class_name.clone());
        self.kind = Some(lsp_types::SymbolKind::METHOD);
        self
    }

    /// Search across multiple files for symbols matching criteria
    pub async fn search_multiple_files(
        self,
        session: &mut ClangdSession,
        files: &[String],
        max_results: Option<u32>,
    ) -> Result<Vec<(String, Vec<DocumentSymbol>)>, AnalyzerError> {
        let mut file_results = Vec::new();

        for file_path in files {
            match self.search_single_file(session, file_path).await {
                Ok(symbols) => {
                    file_results.push((file_path.clone(), symbols));
                }
                Err(e) => {
                    trace!("Failed to search file {}: {}", file_path, e);
                    file_results.push((file_path.clone(), Vec::new()));
                }
            }
        }

        // Apply top-level limiting if specified
        if let Some(max) = max_results {
            let mut all_symbols = Vec::new();
            for (file_path, symbols) in &file_results {
                for symbol in symbols {
                    all_symbols.push((file_path.clone(), symbol.clone()));
                    if all_symbols.len() >= max as usize {
                        break;
                    }
                }
                if all_symbols.len() >= max as usize {
                    break;
                }
            }

            // Reconstruct file_results with limited symbols
            let mut limited_results = Vec::new();
            let mut current_file = String::new();
            let mut current_symbols = Vec::new();

            for (file_path, symbol) in all_symbols {
                if file_path != current_file {
                    if !current_file.is_empty() {
                        limited_results.push((current_file.clone(), current_symbols));
                        current_symbols = Vec::new();
                    }
                    current_file = file_path;
                }
                current_symbols.push(symbol);
            }

            if !current_file.is_empty() {
                limited_results.push((current_file, current_symbols));
            }

            Ok(limited_results)
        } else {
            Ok(file_results)
        }
    }

    /// Search a single file for symbols matching criteria  
    async fn search_single_file(
        &self,
        session: &mut ClangdSession,
        file_path: &str,
    ) -> Result<Vec<DocumentSymbol>, AnalyzerError> {
        let file_uri = if file_path.starts_with("file://") {
            file_path
                .parse()
                .map_err(|e| AnalyzerError::NoData(format!("Invalid URI: {}", e)))?
        } else {
            uri_from_pathbuf(Path::new(file_path))
        };

        let document_symbols = get_document_symbols(session, file_uri).await?;

        let filtered_symbols: Vec<DocumentSymbol> = self
            .clone()
            .find_all(&document_symbols)
            .into_iter()
            .cloned()
            .collect();

        Ok(filtered_symbols)
    }

    /// Check if a symbol matches the search criteria
    fn matches_symbol(&self, symbol: &DocumentSymbol, path: &[&str]) -> bool {
        // Position matching
        if let Some(ref position) = self.position
            && !symbol.contains(position)
        {
            return false;
        }

        // Name matching
        if let Some(ref name) = self.name
            && symbol.name != *name
        {
            return false;
        }

        // Kind matching
        if let Some(kind) = self.kind
            && symbol.kind != kind
        {
            return false;
        }

        // Path matching
        if let Some(ref path_contains) = self.path_contains {
            let full_path = path.join("::");
            if !full_path.contains(path_contains) {
                return false;
            }
        }

        // Additional class member filtering for class_members_only
        if self.kind.is_none() && self.path_contains.is_some() {
            // When class_members_only is used, filter for callable members
            if !matches!(
                symbol.kind,
                lsp_types::SymbolKind::METHOD
                    | lsp_types::SymbolKind::FUNCTION
                    | lsp_types::SymbolKind::CONSTRUCTOR
            ) {
                return false;
            }
        }

        true
    }
}

impl Default for SymbolSearchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Get document symbols for a symbol and find the matching DocumentSymbol
///
/// This function retrieves document symbols for the file containing the symbol,
/// then searches the hierarchical structure to find the matching DocumentSymbol
/// based on the symbol's name and location.
///
/// # Arguments
/// * `session` - Active clangd session
/// * `symbol_name` - Name of the symbol to find
/// * `symbol_location` - Location of the symbol
///
/// Get document symbols for a file URI, returning only hierarchical symbols
///
/// This function calls the LSP `textDocument/documentSymbol` method and expects
/// a hierarchical response due to our client capabilities. If a flat response
/// is received, it logs a warning and returns an error.
///
/// # Arguments
/// * `session` - Active clangd session
/// * `file_uri` - URI of the file to analyze
///
/// # Returns
/// * `Ok(Vec<DocumentSymbol>)` - Hierarchical document symbols
/// * `Err(AnalyzerError)` - LSP error or unexpected flat response
#[allow(dead_code)]
pub async fn get_document_symbols(
    session: &mut ClangdSession,
    file_uri: lsp_types::Uri,
) -> Result<Vec<DocumentSymbol>, AnalyzerError> {
    trace!("Requesting document symbols for URI: {:?}", file_uri);

    // Ensure file is ready in clangd session
    let uri_str = file_uri.to_string();
    let file_path_str = uri_str.strip_prefix("file://").unwrap_or(&uri_str);

    session.ensure_file_ready(Path::new(file_path_str)).await?;

    let client = session.client_mut();

    // Get document symbols from LSP
    let document_symbols = client
        .text_document_document_symbol(file_uri.clone())
        .await
        .map_err(AnalyzerError::from)?;

    trace!(
        "Document symbols response type: {:?}",
        match &document_symbols {
            DocumentSymbolResponse::Nested(_) => "Nested (hierarchical)",
            DocumentSymbolResponse::Flat(_) => "Flat (legacy)",
        }
    );

    // Extract hierarchical symbols or warn about flat response
    match document_symbols {
        DocumentSymbolResponse::Nested(nested_symbols) => {
            trace!(
                "Successfully received {} top-level hierarchical symbols",
                nested_symbols.len()
            );
            Ok(nested_symbols)
        }
        DocumentSymbolResponse::Flat(flat_symbols) => {
            warn!(
                "Received flat document symbols response for '{:?}' despite hierarchical support enabled. \
                 This is unexpected and may indicate a clangd configuration issue. \
                 Flat response contains {} symbols.",
                file_uri,
                flat_symbols.len()
            );
            Err(AnalyzerError::NoData(format!(
                "Unexpected flat document symbols response for '{:?}' despite hierarchical client capability. Expected nested DocumentSymbol format.",
                file_uri
            )))
        }
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Find a specific symbol within the hierarchical document symbols using position
///
/// This is a convenience function that uses the builder pattern internally.
/// For more complex searches, use `SymbolSearchBuilder` directly.
///
/// # Arguments
/// * `symbols` - Hierarchical document symbols to search
/// * `target_line` - Target line number (0-based)
/// * `target_character` - Target character position (0-based)
///
/// # Returns
/// * `Some(&DocumentSymbol)` - Found symbol reference
/// * `None` - No symbol found at the target position
#[allow(dead_code)]
pub fn find_symbol_at_position<'a>(
    symbols: &'a [DocumentSymbol],
    position: &Position,
) -> Option<&'a DocumentSymbol> {
    SymbolSearchBuilder::new()
        .at_position(*position)
        .find_first(symbols)
}

/// Find symbol at position with container hierarchy path
pub fn find_symbol_at_position_with_path<'a>(
    symbols: &'a [DocumentSymbol],
    position: &Position,
) -> Option<(&'a DocumentSymbol, Vec<String>)> {
    find_symbol_recursive_with_path(symbols, position, Vec::new())
}

/// Recursive helper for path-aware symbol finding
fn find_symbol_recursive_with_path<'a>(
    symbols: &'a [DocumentSymbol],
    position: &Position,
    current_path: Vec<String>,
) -> Option<(&'a DocumentSymbol, Vec<String>)> {
    for symbol in symbols {
        if symbol.range.contains(position) {
            // Check children first for most specific match
            if let Some(children) = &symbol.children {
                let mut child_path = current_path.clone();
                child_path.push(symbol.name.clone());

                if let Some(result) =
                    find_symbol_recursive_with_path(children, position, child_path)
                {
                    return Some(result);
                }
            }
            // No more specific child found, this is the target
            return Some((symbol, current_path));
        }
    }
    None
}

/// Find symbols by name using idiomatic iterator approach
///
/// # Arguments
/// * `symbols` - Hierarchical document symbols to search
/// * `name` - Symbol name to search for
///
/// # Returns
/// * `Vec<&DocumentSymbol>` - All symbols with matching name
#[allow(dead_code)]
pub fn find_symbols_by_name<'a>(
    symbols: &'a [DocumentSymbol],
    name: &str,
) -> Vec<&'a DocumentSymbol> {
    SymbolSearchBuilder::new().with_name(name).find_all(symbols)
}

/// Find symbols by kind using idiomatic iterator approach
///
/// # Arguments
/// * `symbols` - Hierarchical document symbols to search
/// * `kind` - Symbol kind to search for
///
/// # Returns
/// * `Vec<&DocumentSymbol>` - All symbols with matching kind
#[allow(dead_code)]
pub fn find_symbols_by_kind(
    symbols: &[DocumentSymbol],
    kind: lsp_types::SymbolKind,
) -> Vec<&DocumentSymbol> {
    SymbolSearchBuilder::new().with_kind(kind).find_all(symbols)
}

/// Count total symbols in hierarchical document symbols using iterator
///
/// Uses functional programming approach with iterator combinators.
///
/// # Arguments
/// * `symbols` - Hierarchical document symbols
///
/// # Returns
/// * `usize` - Total count of all symbols in the hierarchy
#[allow(dead_code)]
pub fn count_total_symbols(symbols: &[DocumentSymbol]) -> usize {
    DocumentSymbolIterator::new(symbols).count()
}

/// Count symbols by kind using iterator approach
///
/// # Arguments
/// * `symbols` - Hierarchical document symbols
/// * `kind` - Symbol kind to count
///
/// # Returns
/// * `usize` - Count of symbols with matching kind
#[allow(dead_code)]
pub fn count_symbols_by_kind(symbols: &[DocumentSymbol], kind: lsp_types::SymbolKind) -> usize {
    DocumentSymbolIterator::new(symbols)
        .filter(|(symbol, _)| symbol.kind == kind)
        .count()
}

/// Get all symbol names with their paths
///
/// Returns a vector of (symbol_name, path) tuples for all symbols in the hierarchy.
///
/// # Arguments
/// * `symbols` - Hierarchical document symbols
///
/// # Returns
/// * `Vec<(String, String)>` - Vector of (name, path) tuples
#[allow(dead_code)]
pub fn get_symbol_paths(symbols: &[DocumentSymbol]) -> Vec<(String, String)> {
    DocumentSymbolIterator::new(symbols)
        .map(|(symbol, path)| (symbol.name.clone(), path.join("::")))
        .collect()
}

/// Extract class members from hierarchical document symbols using the builder pattern
///
/// This function provides a higher-level interface for extracting class members
/// by finding the target class and collecting its callable members (methods, constructors, operators).
/// The search leverages the document symbol hierarchy for comprehensive member discovery.
///
/// # Arguments
/// * `symbols` - Hierarchical document symbols to search
/// * `class_name` - Name of the class to extract members from
///
/// # Returns
/// * `Vec<&DocumentSymbol>` - All callable members found within the specified class
#[allow(dead_code)]
pub fn extract_class_members<'a>(
    symbols: &'a [DocumentSymbol],
    class_name: &str,
) -> Vec<&'a DocumentSymbol> {
    // First find the target class
    if let Some(target_class) = SymbolSearchBuilder::new()
        .with_name(class_name)
        .with_kind(lsp_types::SymbolKind::CLASS)
        .find_first(symbols)
    {
        // Extract callable members from the class's children
        if let Some(children) = &target_class.children {
            return SymbolSearchBuilder::new()
                .class_members_only()
                .find_all(children);
        }
    }

    // Also search for struct types
    if let Some(target_struct) = SymbolSearchBuilder::new()
        .with_name(class_name)
        .with_kind(lsp_types::SymbolKind::STRUCT)
        .find_first(symbols)
        && let Some(children) = &target_struct.children
    {
        return SymbolSearchBuilder::new()
            .class_members_only()
            .find_all(children);
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range, SymbolKind};

    fn create_test_symbol(
        name: &str,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> DocumentSymbol {
        DocumentSymbol {
            name: name.to_string(),
            detail: None,
            kind: SymbolKind::CLASS,
            tags: None,
            #[allow(deprecated)]
            deprecated: None,
            range: Range {
                start: Position {
                    line: start_line,
                    character: start_char,
                },
                end: Position {
                    line: end_line,
                    character: end_char,
                },
            },
            selection_range: Range {
                start: Position {
                    line: start_line,
                    character: start_char,
                },
                end: Position {
                    line: start_line,
                    character: start_char + name.len() as u32,
                },
            },
            children: None,
        }
    }

    fn create_test_symbol_with_kind(
        name: &str,
        kind: SymbolKind,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> DocumentSymbol {
        let mut symbol = create_test_symbol(name, start_line, start_char, end_line, end_char);
        symbol.kind = kind;
        symbol
    }

    #[test]
    fn test_position_contains_trait() {
        let range = Range {
            start: Position {
                line: 10,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 10,
            },
        };

        // Test position within range
        assert!(range.contains(&Position {
            line: 10,
            character: 5
        }));
        assert!(range.contains(&Position {
            line: 10,
            character: 5
        }));

        // Test position outside range
        assert!(!range.contains(&Position {
            line: 11,
            character: 5
        }));
        assert!(!range.contains(&Position {
            line: 10,
            character: 15
        }));
    }

    #[test]
    fn test_document_symbol_iterator() {
        let mut parent = create_test_symbol("Parent", 0, 0, 10, 0);
        let child1 = create_test_symbol("Child1", 2, 4, 4, 0);
        let child2 = create_test_symbol("Child2", 6, 4, 8, 0);

        parent.children = Some(vec![child1, child2]);
        let symbols = vec![parent];

        let collected: Vec<_> = DocumentSymbolIterator::new(&symbols)
            .map(|(symbol, path)| (symbol.name.clone(), path.join("::")))
            .collect();

        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], ("Parent".to_string(), "".to_string()));
        assert_eq!(collected[1], ("Child1".to_string(), "Parent".to_string()));
        assert_eq!(collected[2], ("Child2".to_string(), "Parent".to_string()));
    }

    #[test]
    fn test_symbol_search_builder_position() {
        let symbol = create_test_symbol("TestClass", 10, 0, 20, 0);
        let symbols = vec![symbol];

        // Position within selection range
        let found = SymbolSearchBuilder::new()
            .at_position(Position {
                line: 10,
                character: 5,
            })
            .find_first(&symbols);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "TestClass");

        // Position outside range
        let not_found = SymbolSearchBuilder::new()
            .at_position(Position {
                line: 25,
                character: 0,
            })
            .find_first(&symbols);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_symbol_search_builder_name() {
        let symbol1 = create_test_symbol("ClassA", 0, 0, 5, 0);
        let symbol2 = create_test_symbol("ClassB", 6, 0, 10, 0);
        let symbols = vec![symbol1, symbol2];

        let found = SymbolSearchBuilder::new()
            .with_name("ClassB")
            .find_first(&symbols);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "ClassB");
    }

    #[test]
    fn test_symbol_search_builder_kind() {
        let class_symbol = create_test_symbol_with_kind("MyClass", SymbolKind::CLASS, 0, 0, 5, 0);
        let function_symbol =
            create_test_symbol_with_kind("myFunction", SymbolKind::FUNCTION, 6, 0, 8, 0);
        let symbols = vec![class_symbol, function_symbol];

        let functions = SymbolSearchBuilder::new()
            .with_kind(SymbolKind::FUNCTION)
            .find_all(&symbols);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "myFunction");
    }

    #[test]
    fn test_symbol_search_builder_combined() {
        let mut parent = create_test_symbol_with_kind("Parent", SymbolKind::CLASS, 0, 0, 10, 0);
        let child = create_test_symbol_with_kind("method", SymbolKind::METHOD, 2, 4, 4, 0);

        parent.children = Some(vec![child]);
        let symbols = vec![parent];

        // Search for method by name and kind
        let found = SymbolSearchBuilder::new()
            .with_name("method")
            .with_kind(SymbolKind::METHOD)
            .find_first(&symbols);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "method");

        // Search for method in specific path
        let found_with_path = SymbolSearchBuilder::new()
            .with_name("method")
            .path_contains("Parent")
            .find_first(&symbols);
        assert!(found_with_path.is_some());
        assert_eq!(found_with_path.unwrap().name, "method");
    }

    #[test]
    fn test_find_symbol_at_position() {
        let symbol = create_test_symbol("TestClass", 10, 0, 20, 0);
        let symbols = vec![symbol];

        // Position within selection range
        let found = find_symbol_at_position(
            &symbols,
            &Position {
                line: 10,
                character: 5,
            },
        );
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "TestClass");

        // Position outside range
        let not_found = find_symbol_at_position(
            &symbols,
            &Position {
                line: 25,
                character: 0,
            },
        );
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_symbols_by_name() {
        let symbol1 = create_test_symbol("Test", 0, 0, 5, 0);
        let symbol2 = create_test_symbol("Other", 6, 0, 10, 0);
        let symbol3 = create_test_symbol("Test", 11, 0, 15, 0);
        let symbols = vec![symbol1, symbol2, symbol3];

        let found = find_symbols_by_name(&symbols, "Test");
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|s| s.name == "Test"));
    }

    #[test]
    fn test_find_symbols_by_kind() {
        let class_symbol = create_test_symbol_with_kind("MyClass", SymbolKind::CLASS, 0, 0, 5, 0);
        let function_symbol =
            create_test_symbol_with_kind("myFunction", SymbolKind::FUNCTION, 6, 0, 8, 0);
        let symbols = vec![class_symbol, function_symbol];

        let classes = find_symbols_by_kind(&symbols, SymbolKind::CLASS);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "MyClass");
    }

    #[test]
    fn test_count_total_symbols() {
        let mut parent = create_test_symbol("Parent", 0, 0, 10, 0);
        let child1 = create_test_symbol("Child1", 2, 4, 4, 0);
        let child2 = create_test_symbol("Child2", 6, 4, 8, 0);

        parent.children = Some(vec![child1, child2]);
        let symbols = vec![parent];

        // Should count parent + 2 children = 3 total
        assert_eq!(count_total_symbols(&symbols), 3);
    }

    #[test]
    fn test_count_symbols_by_kind() {
        let class1 = create_test_symbol_with_kind("Class1", SymbolKind::CLASS, 0, 0, 5, 0);
        let class2 = create_test_symbol_with_kind("Class2", SymbolKind::CLASS, 6, 0, 10, 0);
        let function = create_test_symbol_with_kind("func", SymbolKind::FUNCTION, 11, 0, 13, 0);
        let symbols = vec![class1, class2, function];

        assert_eq!(count_symbols_by_kind(&symbols, SymbolKind::CLASS), 2);
        assert_eq!(count_symbols_by_kind(&symbols, SymbolKind::FUNCTION), 1);
        assert_eq!(count_symbols_by_kind(&symbols, SymbolKind::VARIABLE), 0);
    }

    #[test]
    fn test_get_symbol_paths() {
        let mut parent = create_test_symbol("Namespace", 0, 0, 10, 0);
        let mut child_class = create_test_symbol("MyClass", 2, 0, 8, 0);
        let child_method = create_test_symbol("method", 4, 4, 6, 0);

        child_class.children = Some(vec![child_method]);
        parent.children = Some(vec![child_class]);
        let symbols = vec![parent];

        let paths = get_symbol_paths(&symbols);
        assert_eq!(paths.len(), 3);

        // Check paths
        assert!(paths.contains(&("Namespace".to_string(), "".to_string())));
        assert!(paths.contains(&("MyClass".to_string(), "Namespace".to_string())));
        assert!(paths.contains(&("method".to_string(), "Namespace::MyClass".to_string())));
    }
}
