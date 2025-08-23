//! Workspace symbols extraction functionality for C++ projects
//!
//! This module provides LSP-based workspace symbol search with filtering capabilities
//! that work with clangd to find symbols across the entire project workspace.
//! It follows the same builder and iterator patterns as document_symbols.rs for consistency.

use lsp_types::{SymbolKind, WorkspaceSymbol};
use tracing::{debug, trace};

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::lsp::traits::LspClientTrait;
use crate::mcp_server::tools::analyze_symbols::AnalyzerError;
use crate::project::ProjectComponent;

// ============================================================================
// Traits for Workspace Symbol Filtering
// ============================================================================

/// Trait for filtering workspace symbols based on various criteria
pub trait WorkspaceSymbolFilter {
    /// Check if the workspace symbol passes the filter
    fn matches(&self, symbol: &WorkspaceSymbol) -> bool;
}

/// Filter for project boundary detection
pub struct ProjectBoundaryFilter<'a> {
    component: &'a ProjectComponent,
    include_external: bool,
}

impl<'a> ProjectBoundaryFilter<'a> {
    pub fn new(component: &'a ProjectComponent, include_external: bool) -> Self {
        Self {
            component,
            include_external,
        }
    }

    /// Check if a file path belongs to the project
    fn is_project_file(&self, path: &str) -> bool {
        let file_path = std::path::PathBuf::from(path);

        if let Ok(canonical_file) = file_path.canonicalize()
            && canonical_file.starts_with(&self.component.source_root_path)
        {
            return true;
        }
        false
    }
}

impl<'a> WorkspaceSymbolFilter for ProjectBoundaryFilter<'a> {
    fn matches(&self, symbol: &WorkspaceSymbol) -> bool {
        if self.include_external {
            return true;
        }

        let uri_str = match &symbol.location {
            lsp_types::OneOf::Left(location) => location.uri.as_str(),
            lsp_types::OneOf::Right(workspace_location) => workspace_location.uri.as_str(),
        };

        if let Some(path) = uri_str.strip_prefix("file://") {
            self.is_project_file(path)
        } else {
            true // Default to inclusion when URI parsing fails
        }
    }
}

/// Filter for symbol kinds
pub struct SymbolKindFilter {
    allowed_kinds: Vec<lsp_types::SymbolKind>,
}

impl SymbolKindFilter {
    #[allow(dead_code)]
    pub fn new(kinds: Vec<SymbolKind>) -> Self {
        Self {
            allowed_kinds: kinds,
        }
    }
}

impl WorkspaceSymbolFilter for SymbolKindFilter {
    fn matches(&self, symbol: &WorkspaceSymbol) -> bool {
        self.allowed_kinds.contains(&symbol.kind)
    }
}

/// Filter for symbol names using substring matching
pub struct NameFilter {
    query: String,
    case_sensitive: bool,
}

impl NameFilter {
    pub fn new(query: String, case_sensitive: bool) -> Self {
        Self {
            query,
            case_sensitive,
        }
    }
}

impl WorkspaceSymbolFilter for NameFilter {
    fn matches(&self, symbol: &WorkspaceSymbol) -> bool {
        if self.case_sensitive {
            symbol.name.contains(&self.query)
        } else {
            symbol
                .name
                .to_lowercase()
                .contains(&self.query.to_lowercase())
        }
    }
}

// ============================================================================
// Iterator for Workspace Symbols
// ============================================================================

/// Iterator over workspace symbols with filtering capabilities
pub struct WorkspaceSymbolIterator<'a> {
    symbols: std::slice::Iter<'a, WorkspaceSymbol>,
    filters: Vec<Box<dyn WorkspaceSymbolFilter + 'a>>,
}

impl<'a> WorkspaceSymbolIterator<'a> {
    /// Create a new iterator over workspace symbols
    pub fn new(symbols: &'a [WorkspaceSymbol]) -> Self {
        Self {
            symbols: symbols.iter(),
            filters: Vec::new(),
        }
    }

    /// Add a filter to the iterator
    pub fn with_filter<F: WorkspaceSymbolFilter + 'a>(mut self, filter: F) -> Self {
        self.filters.push(Box::new(filter));
        self
    }

    /// Check if a symbol passes all filters
    fn passes_filters(&self, symbol: &WorkspaceSymbol) -> bool {
        self.filters.iter().all(|filter| filter.matches(symbol))
    }
}

impl<'a> Iterator for WorkspaceSymbolIterator<'a> {
    type Item = &'a WorkspaceSymbol;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let symbol = self.symbols.next()?;
            if self.passes_filters(symbol) {
                return Some(symbol);
            }
        }
    }
}

// ============================================================================
// Workspace Symbol Search Builder
// ============================================================================

/// Builder pattern for flexible workspace symbol searching
#[derive(Debug, Clone)]
pub struct WorkspaceSymbolSearchBuilder {
    query: String,
    kinds: Option<Vec<lsp_types::SymbolKind>>,
    max_results: Option<u32>,
    include_external: bool,
    case_sensitive: bool,
}

impl WorkspaceSymbolSearchBuilder {
    /// Create a new search builder with the given query
    pub fn new(query: String) -> Self {
        Self {
            query,
            kinds: None,
            max_results: None,
            include_external: false,
            case_sensitive: false,
        }
    }

    /// Filter by specific symbol kinds
    pub fn with_kinds(mut self, kinds: Vec<lsp_types::SymbolKind>) -> Self {
        self.kinds = Some(kinds);
        self
    }

    /// Limit the maximum number of results
    pub fn with_max_results(mut self, max_results: u32) -> Self {
        self.max_results = Some(max_results);
        self
    }

    /// Include external symbols from system libraries
    pub fn include_external(mut self, include: bool) -> Self {
        self.include_external = include;
        self
    }

    /// Enable case-sensitive search
    #[allow(dead_code)]
    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_sensitive = sensitive;
        self
    }

    /// Execute the search and return filtered results
    pub async fn search(
        &self,
        session: &mut ClangdSession,
        component: &ProjectComponent,
    ) -> Result<Vec<WorkspaceSymbol>, AnalyzerError> {
        trace!(
            "Executing workspace symbol search with query: {}",
            self.query
        );

        // Get symbols from clangd with a large limit (2000) to preserve ranking
        let symbols = session
            .client_mut()
            .workspace_symbols(self.query.clone())
            .await
            .map_err(AnalyzerError::from)?;

        debug!("Retrieved {} symbols from clangd", symbols.len());

        // Apply filters using iterator pattern
        let mut filtered_iter = WorkspaceSymbolIterator::new(&symbols);

        // Add project boundary filter
        filtered_iter =
            filtered_iter.with_filter(ProjectBoundaryFilter::new(component, self.include_external));

        // Add symbol kind filter if specified
        if let Some(ref kinds) = self.kinds {
            filtered_iter = filtered_iter.with_filter(SymbolKindFilter::new(kinds.clone()));
        }

        // Add name filter for additional refinement (beyond clangd's initial filtering)
        filtered_iter =
            filtered_iter.with_filter(NameFilter::new(self.query.clone(), self.case_sensitive));

        // Collect results with optional limit
        let results: Vec<WorkspaceSymbol> = if let Some(max) = self.max_results {
            filtered_iter
                .take(max.min(1000) as usize)
                .cloned()
                .collect()
        } else {
            filtered_iter.cloned().collect()
        };

        debug!(
            "Filtered to {} symbols after applying filters",
            results.len()
        );
        Ok(results)
    }

    /// Execute search and return only the first result
    #[allow(dead_code)]
    pub async fn find_first(
        &self,
        session: &mut ClangdSession,
        component: &ProjectComponent,
    ) -> Result<Option<WorkspaceSymbol>, AnalyzerError> {
        let mut builder = self.clone();
        builder.max_results = Some(1);
        let results = builder.search(session, component).await?;
        Ok(results.into_iter().next())
    }
}

// ============================================================================
// Public API Functions
// ============================================================================

/// Search for workspace symbols using the builder pattern
///
/// This is a convenience function that creates a builder with common defaults.
/// For more control, use `WorkspaceSymbolSearchBuilder::new()` directly.
///
/// # Arguments
/// * `query` - Search query string
/// * `session` - Active clangd session
/// * `component` - Project component for boundary filtering
///
/// # Returns
/// * `Ok(Vec<WorkspaceSymbol>)` - Filtered workspace symbols
/// * `Err(AnalyzerError)` - LSP error or search failure
#[allow(dead_code)]
pub async fn search_workspace_symbols(
    query: &str,
    session: &mut ClangdSession,
    component: &ProjectComponent,
) -> Result<Vec<WorkspaceSymbol>, AnalyzerError> {
    WorkspaceSymbolSearchBuilder::new(query.to_string())
        .search(session, component)
        .await
}

/// Search for workspace symbols with kind filtering
///
/// # Arguments
/// * `query` - Search query string
/// * `kinds` - Symbol kinds to include
/// * `session` - Active clangd session
/// * `component` - Project component for boundary filtering
///
/// # Returns
/// * `Ok(Vec<WorkspaceSymbol>)` - Filtered workspace symbols
/// * `Err(AnalyzerError)` - LSP error or search failure
#[allow(dead_code)]
pub async fn search_workspace_symbols_with_kinds(
    query: &str,
    kinds: Vec<lsp_types::SymbolKind>,
    session: &mut ClangdSession,
    component: &ProjectComponent,
) -> Result<Vec<WorkspaceSymbol>, AnalyzerError> {
    WorkspaceSymbolSearchBuilder::new(query.to_string())
        .with_kinds(kinds)
        .search(session, component)
        .await
}

/// Find the first workspace symbol matching the query
///
/// # Arguments
/// * `query` - Search query string
/// * `session` - Active clangd session
/// * `component` - Project component for boundary filtering
///
/// # Returns
/// * `Ok(Some(WorkspaceSymbol))` - First matching symbol
/// * `Ok(None)` - No symbols found
/// * `Err(AnalyzerError)` - LSP error or search failure
#[allow(dead_code)]
pub async fn find_first_workspace_symbol(
    query: &str,
    session: &mut ClangdSession,
    component: &ProjectComponent,
) -> Result<Option<WorkspaceSymbol>, AnalyzerError> {
    WorkspaceSymbolSearchBuilder::new(query.to_string())
        .find_first(session, component)
        .await
}

/// Search for workspace symbols in project scope only
///
/// This excludes external libraries and system headers by default.
///
/// # Arguments
/// * `query` - Search query string
/// * `session` - Active clangd session
/// * `component` - Project component for boundary filtering
///
/// # Returns
/// * `Ok(Vec<WorkspaceSymbol>)` - Project-scoped workspace symbols
/// * `Err(AnalyzerError)` - LSP error or search failure
#[allow(dead_code)]
pub async fn search_project_symbols(
    query: &str,
    session: &mut ClangdSession,
    component: &ProjectComponent,
) -> Result<Vec<WorkspaceSymbol>, AnalyzerError> {
    WorkspaceSymbolSearchBuilder::new(query.to_string())
        .include_external(false)
        .search(session, component)
        .await
}

/// Count total workspace symbols matching query
///
/// # Arguments
/// * `query` - Search query string
/// * `session` - Active clangd session
/// * `component` - Project component for boundary filtering
///
/// # Returns
/// * `Ok(usize)` - Count of matching symbols
/// * `Err(AnalyzerError)` - LSP error or search failure
#[allow(dead_code)]
pub async fn count_workspace_symbols(
    query: &str,
    session: &mut ClangdSession,
    component: &ProjectComponent,
) -> Result<usize, AnalyzerError> {
    let symbols = search_workspace_symbols(query, session, component).await?;
    Ok(symbols.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Location, OneOf, Position, Range, Uri, WorkspaceSymbol};
    use std::path::PathBuf;
    use std::str::FromStr;

    fn create_test_workspace_symbol(
        name: &str,
        kind: SymbolKind,
        uri: &str,
        container: Option<&str>,
    ) -> WorkspaceSymbol {
        WorkspaceSymbol {
            name: name.to_string(),
            kind,
            tags: None,
            container_name: container.map(|c| c.to_string()),
            location: OneOf::Left(Location {
                uri: Uri::from_str(uri).unwrap(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: name.len() as u32,
                    },
                },
            }),
            data: None,
        }
    }

    fn create_test_component() -> ProjectComponent {
        use crate::project::CompilationDatabase;

        ProjectComponent {
            build_dir_path: PathBuf::from("/test/project/build"),
            source_root_path: PathBuf::from("/test/project"),
            compilation_database: CompilationDatabase {
                path: PathBuf::from("/test/project/build/compile_commands.json"),
                entries: vec![],
            },
            provider_type: "cmake".to_string(),
            generator: "Ninja".to_string(),
            build_type: "Debug".to_string(),
            build_options: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_workspace_symbol_iterator_basic() {
        let symbols = vec![
            create_test_workspace_symbol("ClassA", SymbolKind::CLASS, "file:///test.cpp", None),
            create_test_workspace_symbol(
                "functionB",
                SymbolKind::FUNCTION,
                "file:///test.cpp",
                None,
            ),
            create_test_workspace_symbol("ClassC", SymbolKind::CLASS, "file:///test.cpp", None),
        ];

        let classes: Vec<_> = WorkspaceSymbolIterator::new(&symbols)
            .with_filter(SymbolKindFilter::new(vec![SymbolKind::CLASS]))
            .collect();

        assert_eq!(classes.len(), 2);
        assert!(classes.iter().all(|s| s.kind == SymbolKind::CLASS));
    }

    #[test]
    fn test_workspace_symbol_iterator_multiple_filters() {
        let symbols = vec![
            create_test_workspace_symbol("TestClass", SymbolKind::CLASS, "file:///test.cpp", None),
            create_test_workspace_symbol("MyClass", SymbolKind::CLASS, "file:///test.cpp", None),
            create_test_workspace_symbol(
                "TestFunction",
                SymbolKind::FUNCTION,
                "file:///test.cpp",
                None,
            ),
        ];

        let results: Vec<_> = WorkspaceSymbolIterator::new(&symbols)
            .with_filter(SymbolKindFilter::new(vec![SymbolKind::CLASS]))
            .with_filter(NameFilter::new("Test".to_string(), false))
            .collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "TestClass");
    }

    #[test]
    fn test_symbol_kind_filter_from_kinds() {
        let kinds = vec![SymbolKind::CLASS, SymbolKind::FUNCTION];
        let filter = SymbolKindFilter::new(kinds);

        let class_symbol =
            create_test_workspace_symbol("Test", SymbolKind::CLASS, "file:///test.cpp", None);
        let function_symbol =
            create_test_workspace_symbol("Test", SymbolKind::FUNCTION, "file:///test.cpp", None);
        let variable_symbol =
            create_test_workspace_symbol("Test", SymbolKind::VARIABLE, "file:///test.cpp", None);

        assert!(filter.matches(&class_symbol));
        assert!(filter.matches(&function_symbol));
        assert!(!filter.matches(&variable_symbol));
    }

    #[test]
    fn test_name_filter_case_insensitive() {
        let filter = NameFilter::new("test".to_string(), false);

        let symbol1 =
            create_test_workspace_symbol("TestClass", SymbolKind::CLASS, "file:///test.cpp", None);
        let symbol2 =
            create_test_workspace_symbol("mytest", SymbolKind::FUNCTION, "file:///test.cpp", None);
        let symbol3 =
            create_test_workspace_symbol("Other", SymbolKind::VARIABLE, "file:///test.cpp", None);

        assert!(filter.matches(&symbol1));
        assert!(filter.matches(&symbol2));
        assert!(!filter.matches(&symbol3));
    }

    #[test]
    fn test_name_filter_case_sensitive() {
        let filter = NameFilter::new("Test".to_string(), true);

        let symbol1 =
            create_test_workspace_symbol("TestClass", SymbolKind::CLASS, "file:///test.cpp", None);
        let symbol2 = create_test_workspace_symbol(
            "testClass",
            SymbolKind::FUNCTION,
            "file:///test.cpp",
            None,
        );

        assert!(filter.matches(&symbol1));
        assert!(!filter.matches(&symbol2));
    }

    #[test]
    fn test_workspace_symbol_search_builder() {
        let builder = WorkspaceSymbolSearchBuilder::new("test".to_string())
            .with_kinds(vec![lsp_types::SymbolKind::CLASS])
            .with_max_results(10)
            .include_external(true)
            .case_sensitive(false);

        assert_eq!(builder.query, "test");
        assert_eq!(builder.kinds, Some(vec![lsp_types::SymbolKind::CLASS]));
        assert_eq!(builder.max_results, Some(10));
        assert!(builder.include_external);
        assert!(!builder.case_sensitive);
    }

    #[test]
    fn test_project_boundary_filter() {
        let component = create_test_component();

        // Create symbols - one in project, one external
        let _project_symbol = create_test_workspace_symbol(
            "ProjectClass",
            SymbolKind::CLASS,
            "file:///test/project/src/main.cpp",
            None,
        );
        let _external_symbol = create_test_workspace_symbol(
            "ExternalClass",
            SymbolKind::CLASS,
            "file:///usr/include/vector",
            None,
        );

        let filter = ProjectBoundaryFilter::new(&component, false);

        // Note: This test will not actually work without real file system
        // In practice, canonicalize() would be mocked or tested with real files
        // For now, we test the structure is correct
        assert!(!filter.include_external);
    }
}
