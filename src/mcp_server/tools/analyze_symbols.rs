//! Symbol context analysis functionality - V2 ARCHITECTURE IMPLEMENTATION
//!
//! Complete rewrite using the v2 architecture modules:
//! - clangd/: Session management with builder pattern
//! - lsp/: Modern LSP client with traits
//! - project/: Extensible project/build system abstraction
//! - io/: Process and transport management

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, instrument, warn};

use crate::clangd::session::ClangdSession;
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
use crate::project::{ComponentSession, ProjectWorkspace};
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
}

impl From<AnalyzerError> for CallToolError {
    fn from(err: AnalyzerError) -> Self {
        CallToolError::new(std::io::Error::other(err.to_string()))
    }
}

// ============================================================================
// MCP Tool Definition - PRESERVE EXACT EXTERNAL SCHEMA
// ============================================================================

#[mcp_tool(
    name = "analyze_symbol_context",
    description = "Advanced multi-dimensional C++ symbol analysis engine providing comprehensive contextual \
                   understanding of any symbol in your codebase through sophisticated clangd LSP integration. \
                   This tool performs deep semantic analysis combining multiple LSP operations to deliver \
                   complete symbol intelligence for complex C++ codebases.

                   🔍 SYMBOL RESOLUTION CAPABILITIES:
                   • Simple names: 'MyClass', 'factorial', 'process'
                   • Fully qualified names: 'std::vector', 'MyNamespace::MyClass'
                   • Global scope symbols: '::main', '::global_function'
                   • Template specializations and overloaded functions
                   • Advanced disambiguation using optional location hints

                   📊 CORE SEMANTIC ANALYSIS:
                   • Precise symbol kind classification (class, function, variable, etc.)
                   • Complete type information with template parameters
                   • Extracted documentation comments and signatures
                   • Definition and declaration locations with file mappings
                   • Fully qualified names with namespace resolution

                   🏛 CLASS MEMBER ANALYSIS (classes/structs):
                   • Flat enumeration of all class members (methods, fields, constructors)
                   • Member kind classification with string representation (method, field, constructor, etc.)
                   • Member signatures and documentation extraction
                   • Static vs instance member identification
                   • Access level determination where available

                   📈 USAGE EXAMPLES (always included):
                   • Concrete code snippets showing how the symbol is used throughout the codebase
                   • Real usage patterns from actual code references
                   • Automatically collected from all references to the symbol
                   • Configurable limit via max_examples parameter (unlimited by default)

                   🏗️ INHERITANCE HIERARCHY ANALYSIS (optional):
                   • Complete class relationship mapping and base class hierarchies
                   • Derived class discovery and virtual function relationships
                   • Multiple inheritance resolution and abstract interface identification
                   • Essential for understanding polymorphic relationships

                   📞 CALL RELATIONSHIP ANALYSIS (optional):
                   • Incoming call discovery (who calls this function)
                   • Outgoing call mapping (what functions this calls)
                   • Call chain traversal with configurable depth limits
                   • Dependency relationship mapping and recursive call detection

                   ⚡ PERFORMANCE & RELIABILITY:
                   • Leverages clangd's high-performance indexing system
                   • Concurrent LSP request processing for parallel analysis
                   • Intelligent caching and graceful degradation
                   • Automatic build directory detection and clangd setup

                   🎯 TARGET USE CASES:
                   Code navigation • Dependency analysis • Refactoring preparation • Architecture understanding
                   • Debugging inheritance issues • Code review assistance • Technical documentation • Educational exploration
                   • Class member discovery and API exploration

                   INPUT REQUIREMENTS:
                   • symbol: Required string - the symbol name to analyze
                   • build_directory: Optional - specific build directory containing compile_commands.json
                   • max_examples: Optional number - limits the number of usage examples (unlimited by default)
                   
                   FUTURE PARAMETERS (not yet implemented):
                   • location: Optional - for disambiguating overloaded/template symbols
                   • include_inheritance: Optional boolean - enables class hierarchy analysis
                   • include_call_hierarchy: Optional boolean - enables function call analysis"
)]
#[derive(Debug, ::serde::Serialize, ::serde::Deserialize, JsonSchema)]
pub struct AnalyzeSymbolContextTool {
    /// The symbol name to analyze. REQUIRED.
    ///
    /// EXAMPLES:
    /// • Simple names: "MyClass", "factorial", "calculateSum"
    /// • Fully qualified: "std::vector", "MyNamespace::MyClass"  
    /// • Global scope: "::main", "::global_var"
    /// • Methods: "MyClass::method" (class context will be analyzed)
    ///
    /// For overloaded functions or template specializations, consider providing
    /// the optional 'location' parameter for precise disambiguation.
    pub symbol: String,

    /// Build directory path containing compile_commands.json. OPTIONAL.
    ///
    /// FORMATS ACCEPTED:
    /// • Relative path: "build", "build-debug", "out/Debug"
    /// • Absolute path: "/home/project/build", "/path/to/build-dir"
    ///
    /// BEHAVIOR: When specified, uses this build directory instead of auto-detection.
    /// The build directory must contain compile_commands.json for clangd integration.
    ///
    /// AUTO-DETECTION (when not specified): Attempts to find single build directory
    /// in current workspace. Fails if multiple or zero build directories found.
    ///
    /// CLANGD SETUP: clangd CWD will be set to project root (from CMAKE_SOURCE_DIR),
    /// and build directory will be passed via --compile-commands-dir argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_directory: Option<String>,

    /// Maximum number of usage examples to include in the analysis. OPTIONAL.
    ///
    /// BEHAVIOR:
    /// • Not specified or None: Returns all available usage examples (unlimited)
    /// • Some(n): Returns at most n usage examples
    ///
    /// EXAMPLES are code snippets showing how the symbol is used throughout the codebase.
    /// They are collected from references to the symbol, excluding the declaration itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_examples: Option<u32>,

    /// Location hint for disambiguating overloaded symbols. OPTIONAL.
    ///
    /// FORMAT: Compact LSP-style location string with 1-based line/column numbers:
    /// • "/absolute/path/to/file.cpp:line:column"
    /// • Example: "/home/project/src/Math.cpp:89:8"
    ///
    /// BEHAVIOR:
    /// • None: Uses workspace symbol resolution (fuzzy matching across project)
    /// • Some(location): Finds document symbol at the specified location
    ///
    /// USE CASES:
    /// • Disambiguating function overloads with same name but different signatures
    /// • Targeting specific template specializations
    /// • Precise symbol selection in files with multiple symbols of same name
    ///
    /// NOTE: Column number is required. Use editor or LSP tools to get exact position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_hint: Option<String>,
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
        locked_session: &mut ClangdSession,
    ) -> Result<(Symbol, SymbolContext), CallToolError> {
        let workspace_symbol = get_matching_symbol(&self.symbol, locked_session)
            .await
            .map_err(|err| {
                error!("Failed to get matching workspace symbol: {}", err);
                CallToolError::from(err)
            })?;

        let symbol = workspace_symbol.clone();

        let file_uri = crate::symbol::uri_from_pathbuf(&symbol.location.file_path);

        let document_symbols = get_document_symbols(locked_session, file_uri)
            .await
            .map_err(CallToolError::from)?;

        let position: lsp_types::Position = symbol.location.range.start.into();

        let (doc_symbol, container_path) =
            find_symbol_at_position_with_path(&document_symbols, &position).ok_or_else(|| {
                CallToolError::new(std::io::Error::other(format!(
                    "Could not find document symbol for workspace symbol '{}'",
                    self.symbol
                )))
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
        locked_session: &mut ClangdSession,
    ) -> Result<(Symbol, SymbolContext), CallToolError> {
        let file_uri = crate::symbol::uri_from_pathbuf(&location.file_path);

        let document_symbols = get_document_symbols(locked_session, file_uri)
            .await
            .map_err(CallToolError::from)?;

        let position: lsp_types::Position = location.range.start.into();

        let (doc_symbol, container_path) =
            find_symbol_at_position_with_path(&document_symbols, &position).ok_or_else(|| {
                CallToolError::new(std::io::Error::other(format!(
                    "No symbol found at location {}",
                    location.to_compact_range()
                )))
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
        locked_session: &mut ClangdSession,
    ) -> Result<(Vec<FileLocation>, Vec<FileLocation>), CallToolError> {
        let definitions = get_definitions(symbol_location, locked_session).await?;
        info!(
            "Found {} definitions for '{}'",
            definitions.len(),
            self.symbol
        );

        let declarations = get_declarations(symbol_location, locked_session).await?;
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
        locked_session: &mut ClangdSession,
    ) -> Option<String> {
        match get_hover_info(symbol_location, locked_session).await {
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
        locked_session: &mut ClangdSession,
    ) -> Vec<FileLocation> {
        match get_examples(locked_session, symbol_location, self.max_examples).await {
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
        locked_session: &mut ClangdSession,
    ) -> (Option<TypeHierarchy>, Option<CallHierarchy>) {
        let type_hierarchy = if Self::supports_type_hierarchy(symbol.kind) {
            match get_type_hierarchy(symbol_location, locked_session).await {
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
                match get_call_hierarchy(symbol_location, locked_session).await {
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
        skip(self, component_session, _workspace, _file_buffer_manager)
    )]
    pub async fn call_tool(
        &self,
        component_session: Arc<ComponentSession>,
        _workspace: &ProjectWorkspace,
        _file_buffer_manager: Arc<Mutex<crate::io::file_manager::RealFileBufferManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Starting symbol analysis for '{}'", self.symbol);

        // Ensure indexing completion using ComponentSession
        component_session
            .ensure_indexed(std::time::Duration::from_secs(30))
            .await
            .map_err(|e| {
                error!("Indexing failed for symbol analysis: {}", e);
                CallToolError::new(std::io::Error::other(format!("Indexing failed: {}", e)))
            })?;

        // Lock session for LSP operations
        let session_arc = component_session.clangd_session();
        let mut locked_session = session_arc.lock().await;

        let (symbol, symbol_context) = match &self.location_hint {
            None => {
                self.resolve_symbol_via_workspace_with_context(&mut locked_session)
                    .await?
            }
            Some(location_str) => {
                let location: FileLocation = location_str.parse().map_err(|e| {
                    CallToolError::new(std::io::Error::other(format!(
                        "Invalid location format '{}': {}",
                        location_str, e
                    )))
                })?;
                self.resolve_symbol_context_at_location(&location, &mut locked_session)
                    .await?
            }
        };

        // Get definitions and declarations
        let (definitions, mut declarations) = self
            .get_definitions_and_declarations(&symbol.location, &mut locked_session)
            .await?;

        // Deduplicate: if definitions == declarations, clear declarations
        if definitions == declarations {
            info!("Definitions and declarations are identical, clearing declarations");
            declarations.clear();
        }

        // Get hover information
        let hover = self
            .get_hover_documentation(&symbol.location, &mut locked_session)
            .await;

        // Get usage examples
        let examples = self
            .get_usage_examples(&symbol.location, &mut locked_session)
            .await;

        // Get hierarchies based on symbol type
        let (type_hierarchy, call_hierarchy) = self
            .get_hierarchies(&symbol, &symbol.location, &mut locked_session)
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
        };

        let output = serde_json::to_string_pretty(&result).map_err(AnalyzerError::from)?;
        Ok(CallToolResult::text_content(vec![TextContent::from(
            output,
        )]))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_analyzer_with_real_clangd() {
        use super::*;
        use crate::io::file_manager::RealFileBufferManager;
        use std::sync::Arc;
        use tokio::sync::Mutex;

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

        let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

        let tool = AnalyzeSymbolContextTool {
            symbol: "Math".to_string(),
            build_directory: None,
            max_examples: None,
            location_hint: None,
        };

        let component_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await
            .unwrap();
        let result = tool
            .call_tool(component_session, &workspace, file_buffer_manager)
            .await;

        // Check and log error if present
        if let Err(ref err) = result {
            error!("Failed to analyze symbol: {}", err);
        }

        assert!(result.is_ok());

        let call_result = result.unwrap();
        let text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
            rust_mcp_sdk::schema::TextContent { text, .. },
        )) = call_result.content.first()
        {
            text
        } else {
            panic!("Expected TextContent in call_result");
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

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_analyzer_with_max_examples() {
        use super::*;
        use crate::io::file_manager::RealFileBufferManager;
        use std::sync::Arc;
        use tokio::sync::Mutex;

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

        let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

        // Test with max_examples = 2
        let tool = AnalyzeSymbolContextTool {
            symbol: "Math".to_string(),
            build_directory: None,
            max_examples: Some(2),
            location_hint: None,
        };

        let component_session = workspace_session
            .get_component_session(test_project.build_dir.clone())
            .await
            .unwrap();
        let result = tool
            .call_tool(component_session, &workspace, file_buffer_manager)
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        let text = if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
            rust_mcp_sdk::schema::TextContent { text, .. },
        )) = call_result.content.first()
        {
            text
        } else {
            panic!("Expected TextContent in call_result");
        };
        let analyzer_result: AnalyzerResult = serde_json::from_str(text).unwrap();

        // Should have at most 2 examples
        assert!(
            analyzer_result.examples.len() <= 2,
            "Should have at most 2 examples, but got {}",
            analyzer_result.examples.len()
        );

        info!(
            "Found {} usage examples (max was 2)",
            analyzer_result.examples.len()
        );
    }
}
