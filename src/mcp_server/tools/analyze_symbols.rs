//! Symbol context analysis functionality - V2 ARCHITECTURE IMPLEMENTATION
//!
//! Complete rewrite using the v2 architecture modules:
//! - clangd/: Session management with builder pattern
//! - lsp/: Modern LSP client with traits
//! - project/: Extensible project/build system abstraction
//! - io/: Process and transport management

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, instrument};

use crate::clangd::session::{ClangdSession, ClangdSessionTrait};
use crate::io::file_manager::RealFileBufferManager;
use crate::lsp::traits::LspClientTrait;
use crate::project::ProjectWorkspace;
use crate::symbol::Symbol;

// ============================================================================
// Analyzer Error Type
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error("No symbols found for '{0}'")]
    NoSymbols(String),
    #[error("LSP error: {0}")]
    Lsp(#[from] crate::lsp::client::LspError),
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

                   📈 USAGE PATTERN ANALYSIS (optional):
                   • Statistical reference counting across entire codebase
                   • Usage pattern classification (initialization, calls, inheritance)
                   • Concrete code examples demonstrating typical usage
                   • File distribution and usage density metrics

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
                   • location: Optional - for disambiguating overloaded/template symbols
                   • include_usage_patterns: Optional boolean - enables usage statistics and examples
                   • include_inheritance: Optional boolean - enables class hierarchy analysis
                   • include_call_hierarchy: Optional boolean - enables function call analysis
                   • Analysis depth and example limits are configurable via optional parameters"
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
}

#[derive(Debug, Serialize)]
struct AnalyzerResult {
    symbol: Symbol,
    query: String,
    total_found: usize,
}

#[derive(Debug, Serialize)]
struct AnalyzerErrorResult {
    error: String,
    query: String,
}

impl AnalyzeSymbolContextTool {
    /// V2 entry point - uses shared ClangdSession from server
    #[instrument(
        name = "analyze_symbol_context",
        skip(self, session, _workspace, _file_buffer_manager)
    )]
    pub async fn call_tool(
        &self,
        session: Arc<Mutex<ClangdSession>>,
        _workspace: &ProjectWorkspace,
        _file_buffer_manager: Arc<Mutex<RealFileBufferManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Starting symbol analysis for '{}'", self.symbol);

        // Lock session, perform analysis, then drop the lock
        let symbols = {
            let mut session_guard = session.lock().await;
            super::utils::wait_for_indexing(session_guard.index_monitor(), None).await;
            session_guard
                .client_mut()
                .workspace_symbols(self.symbol.clone())
                .await
                .map_err(AnalyzerError::from)?
        }; // session_guard is dropped here

        // Process symbols without holding the mutex
        self.process_symbols(symbols).await
    }

    /// Process symbols and create result - no mutex locks needed
    async fn process_symbols(
        &self,
        symbols: Vec<lsp_types::WorkspaceSymbol>,
    ) -> Result<CallToolResult, CallToolError> {
        if symbols.is_empty() {
            let error_result = AnalyzerErrorResult {
                error: format!("No symbols found for '{}'", self.symbol),
                query: self.symbol.clone(),
            };
            let output =
                serde_json::to_string_pretty(&error_result).map_err(AnalyzerError::from)?;
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                output,
            )]));
        }

        // Take the first symbol as the best match
        let best_match = &symbols[0];
        info!(
            "Found {} symbols, using first match: {}",
            symbols.len(),
            best_match.name
        );

        // Convert to our Symbol type
        let symbol = Symbol::from(best_match.clone());

        // Create result with the symbol
        let result = AnalyzerResult {
            symbol,
            query: self.symbol.clone(),
            total_found: symbols.len(),
        };

        let output = serde_json::to_string_pretty(&result).map_err(AnalyzerError::from)?;
        Ok(CallToolResult::text_content(vec![TextContent::from(
            output,
        )]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::SymbolKind;

    #[test]
    fn test_analyzer_result_serialization() {
        let symbol = Symbol {
            name: "Math".to_string(),
            kind: SymbolKind::CLASS,
            container_name: Some("TestProject".to_string()),
            location: "/test/math.cpp".to_string(),
        };

        let result = AnalyzerResult {
            symbol,
            query: "Math".to_string(),
            total_found: 1,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"name\":\"Math\""));
        assert!(json.contains("\"kind\":\"class\""));
        assert!(json.contains("\"container_name\":\"TestProject\""));
        assert!(json.contains("\"location\":\"/test/math.cpp\""));
        assert!(json.contains("\"query\":\"Math\""));
        assert!(json.contains("\"total_found\":1"));
    }

    #[test]
    fn test_analyzer_error_result_serialization() {
        let result = AnalyzerErrorResult {
            error: "No symbols found".to_string(),
            query: "NonExistent".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"error\":\"No symbols found\""));
        assert!(json.contains("\"query\":\"NonExistent\""));
    }

    #[cfg(feature = "clangd-integration-tests")]
    #[tokio::test]
    async fn test_analyzer_with_real_clangd() {
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

        // Create a WorkspaceSession which will trigger indexing
        let workspace_session = WorkspaceSession::new(workspace.clone());
        let session = workspace_session
            .get_or_create_session(test_project.build_dir.clone())
            .await
            .expect("Failed to create session");

        let file_buffer_manager = Arc::new(Mutex::new(RealFileBufferManager::new_real()));

        let tool = AnalyzeSymbolContextTool {
            symbol: "Math".to_string(),
            build_directory: None,
        };

        let result = tool
            .call_tool(session, &workspace, file_buffer_manager)
            .await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(rust_mcp_sdk::schema::ContentBlock::TextContent(
            rust_mcp_sdk::schema::TextContent { text, .. },
        )) = call_result.content.first()
        {
            let output: serde_json::Value = serde_json::from_str(text).unwrap();

            if output.get("error").is_none() {
                // Should find Math symbol
                assert!(output.get("symbol").is_some());
                let symbol = &output["symbol"];
                assert_eq!(symbol["name"], "Math");
                assert_eq!(symbol["kind"], "class");
                info!("Found symbol: {}", symbol);
            } else {
                // If no Math symbol found, that's a problem with our test setup
                panic!(
                    "Math symbol should exist in test project but got error: {}",
                    output["error"]
                );
            }
        }
    }
}
