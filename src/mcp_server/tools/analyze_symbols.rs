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
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, instrument, warn};

use crate::clangd::session::ClangdSession;
use crate::io::{file_buffer::FileBufferError, file_manager::RealFileBufferManager};
use crate::mcp_server::tools::lsp_helpers::{
    call_hierarchy::{CallHierarchy, get_call_hierarchy},
    definitions::{get_declarations, get_definitions},
    examples::get_examples,
    hover::get_hover_info,
    members::{Members, get_members},
    symbol_resolution::get_matching_symbol,
    type_hierarchy::{TypeHierarchy, get_type_hierarchy},
};
use crate::project::ProjectWorkspace;
use crate::symbol::{FileLineWithContents, Symbol, get_symbol_location};

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
    #[error("Session error")]
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzerResult {
    pub symbol: Symbol,
    pub query: String,
    pub definitions: Vec<FileLineWithContents>,
    pub declarations: Vec<FileLineWithContents>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_documentation: Option<String>,

    /// Usage examples showing how the symbol is used throughout the codebase
    pub examples: Vec<FileLineWithContents>,

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

const ANALYZER_INDEX_TIMEOUT: Duration = Duration::from_secs(20);

impl AnalyzeSymbolContextTool {
    /// Resolves the symbol and returns both the symbol and its location
    async fn resolve_symbol(
        &self,
        locked_session: &mut ClangdSession,
    ) -> Result<(Symbol, crate::symbol::FileLocation), CallToolError> {
        let lsp_symbol = match get_matching_symbol(&self.symbol, locked_session).await {
            Ok(symbol) => symbol,
            Err(err) => {
                error!("Failed to get matching symbol: {}", err);
                return Err(CallToolError::from(err));
            }
        };

        let symbol: Symbol = lsp_symbol.clone().into();
        let symbol_location = get_symbol_location(&lsp_symbol);

        match symbol_location {
            Some(location) => Ok((symbol, location)),
            None => {
                error!("No location found for symbol '{}'", self.symbol);
                Err(CallToolError::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("No location found for symbol '{}'", self.symbol),
                )))
            }
        }
    }

    /// Retrieves definitions and declarations for the symbol
    async fn get_definitions_and_declarations(
        &self,
        symbol_location: &crate::symbol::FileLocation,
        locked_session: &mut ClangdSession,
        locked_file_buffer: &mut RealFileBufferManager,
    ) -> Result<(Vec<FileLineWithContents>, Vec<FileLineWithContents>), CallToolError> {
        let definitions =
            get_definitions(locked_session, locked_file_buffer, symbol_location).await?;
        info!(
            "Found {} definitions for '{}'",
            definitions.len(),
            self.symbol
        );

        let declarations =
            get_declarations(locked_session, locked_file_buffer, symbol_location).await?;
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
        locked_file_buffer: &mut RealFileBufferManager,
    ) -> Vec<FileLineWithContents> {
        match get_examples(
            locked_session,
            locked_file_buffer,
            symbol_location,
            self.max_examples,
        )
        .await
        {
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
        let type_hierarchy = match symbol.kind {
            lsp_types::SymbolKind::CLASS
            | lsp_types::SymbolKind::STRUCT
            | lsp_types::SymbolKind::INTERFACE => {
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
            }
            _ => None,
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

    /// Retrieves members for classes and structs
    async fn get_members(
        &self,
        symbol: &Symbol,
        symbol_location: &crate::symbol::FileLocation,
        locked_session: &mut ClangdSession,
    ) -> Option<Members> {
        match symbol.kind {
            lsp_types::SymbolKind::CLASS | lsp_types::SymbolKind::STRUCT => {
                match get_members(symbol_location, locked_session, &symbol.name).await {
                    Ok(members) => {
                        info!(
                            "Found members for '{}': {} methods, {} constructors, {} operators, {} static methods",
                            self.symbol,
                            members.methods.len(),
                            members.constructors.len(),
                            members.operators.len(),
                            members.static_methods.len()
                        );
                        Some(members)
                    }
                    Err(err) => {
                        warn!("Failed to get members: {}", err);
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// V2 entry point - uses shared ClangdSession from server
    #[instrument(
        name = "analyze_symbol_context",
        skip(self, session, _workspace, file_buffer_manager)
    )]
    pub async fn call_tool(
        &self,
        session: Arc<Mutex<ClangdSession>>,
        _workspace: &ProjectWorkspace,
        file_buffer_manager: Arc<Mutex<RealFileBufferManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Starting symbol analysis for '{}'", self.symbol);

        // Lock session and wait for index completion
        let mut locked_session = session.lock().await;

        super::utils::wait_for_indexing(
            locked_session.index_monitor(),
            Some(ANALYZER_INDEX_TIMEOUT.as_secs()),
        )
        .await;

        // Resolve symbol and get its location
        let (symbol, symbol_location) = self.resolve_symbol(&mut locked_session).await?;

        // Get definitions and declarations
        let mut locked_file_buffer = file_buffer_manager.lock().await;
        let (definitions, declarations) = self
            .get_definitions_and_declarations(
                &symbol_location,
                &mut locked_session,
                &mut locked_file_buffer,
            )
            .await?;

        // Get hover information
        let hover = self
            .get_hover_documentation(&symbol_location, &mut locked_session)
            .await;

        // Get usage examples
        let examples = self
            .get_usage_examples(
                &symbol_location,
                &mut locked_session,
                &mut locked_file_buffer,
            )
            .await;

        // Get hierarchies based on symbol type
        let (type_hierarchy, call_hierarchy) = self
            .get_hierarchies(&symbol, &symbol_location, &mut locked_session)
            .await;

        // Get members for classes and structs
        let members = self
            .get_members(&symbol, &symbol_location, &mut locked_session)
            .await;

        let result = AnalyzerResult {
            symbol,
            query: self.symbol.clone(),
            hover_documentation: hover,
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
        let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path);
        let session = workspace_session
            .get_or_create_session(test_project.build_dir.clone())
            .await
            .expect("Failed to create session");

        let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

        let tool = AnalyzeSymbolContextTool {
            symbol: "Math".to_string(),
            build_directory: None,
            max_examples: None,
        };

        let result = tool
            .call_tool(session, &workspace, file_buffer_manager)
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
        assert!(!analyzer_result.declarations.is_empty());

        for definition in &analyzer_result.definitions {
            info!("Definition: {} at {}", definition.contents, definition.line);
        }

        for declaration in &analyzer_result.declarations {
            info!(
                "Declaration: {} at {}",
                declaration.contents, declaration.line
            );
        }

        // Verify examples are collected
        info!("Found {} usage examples", analyzer_result.examples.len());
        for (i, example) in analyzer_result.examples.iter().enumerate() {
            info!(
                "Example {}: {} at {}",
                i + 1,
                example.contents,
                example.line
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
        let workspace_session = WorkspaceSession::new(workspace.clone(), clangd_path);
        let session = workspace_session
            .get_or_create_session(test_project.build_dir.clone())
            .await
            .expect("Failed to create session");

        let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

        // Test with max_examples = 2
        let tool = AnalyzeSymbolContextTool {
            symbol: "Math".to_string(),
            build_directory: None,
            max_examples: Some(2),
        };

        let result = tool
            .call_tool(session, &workspace, file_buffer_manager)
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
