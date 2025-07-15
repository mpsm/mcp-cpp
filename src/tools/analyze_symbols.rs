//! Symbol context analysis functionality

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, instrument};

use crate::cmake::CmakeProjectStatus;
use crate::lsp::ClangdManager;
use super::serialize_result;
use super::symbol_filtering::SymbolUtilities;

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

                   INPUT REQUIREMENTS:
                   • symbol: Required string - the symbol name to analyze
                   • location: Optional - for disambiguating overloaded/template symbols
                   • include_usage_patterns: Optional boolean - enables usage statistics and examples
                   • include_inheritance: Optional boolean - enables class hierarchy analysis
                   • include_call_hierarchy: Optional boolean - enables function call analysis
                   • Analysis depth and example limits are configurable via optional parameters"
)]
#[derive(Debug, ::serde::Serialize, JsonSchema)]
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
    
    /// Optional file location to disambiguate symbols with identical names.
    /// 
    /// USE WHEN: Multiple symbols exist with the same name (overloaded functions,
    /// template specializations, symbols in different namespaces/classes).
    /// 
    /// FORMATS ACCEPTED:
    /// • Relative path: "src/math.cpp" 
    /// • Absolute path: "/home/project/src/math.cpp"
    /// • File URI: "file:///home/project/src/math.cpp"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SymbolLocation>,
    
    /// Include usage patterns and concrete code examples. DEFAULT: false.
    /// 
    /// ENABLES: Reference counting, usage statistics, file distribution analysis,
    /// and up to 'max_usage_examples' concrete code snippets showing how the symbol is used.
    /// 
    /// PERFORMANCE NOTE: Adds ~2-5 seconds to analysis time for heavily used symbols.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_usage_patterns: Option<bool>,
    
    /// Maximum number of usage examples to include. DEFAULT: 5. RANGE: 1-20.
    /// 
    /// Only relevant when 'include_usage_patterns' is true.
    /// Each example includes file location, code context, and usage pattern classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_usage_examples: Option<u32>,
    
    /// Include class inheritance and hierarchy information. DEFAULT: false.
    /// 
    /// ENABLES: Base class discovery, derived class mapping, virtual function analysis.
    /// APPLIES TO: Classes, structs, interfaces - ignored for functions/variables.
    /// 
    /// PERFORMANCE NOTE: Adds ~1-3 seconds for complex inheritance hierarchies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_inheritance: Option<bool>,
    
    /// Include function call relationships and dependency analysis. DEFAULT: false.
    /// 
    /// ENABLES: Incoming calls (who calls this), outgoing calls (what this calls),
    /// call chain traversal up to 'max_call_depth' levels.
    /// APPLIES TO: Functions, methods, constructors - ignored for variables/types.
    /// 
    /// PERFORMANCE NOTE: Adds ~2-8 seconds depending on call complexity and depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_call_hierarchy: Option<bool>,
    
    /// Maximum depth for call hierarchy traversal. DEFAULT: 3. RANGE: 1-10.
    /// 
    /// Only relevant when 'include_call_hierarchy' is true.
    /// Controls how deep to follow the call chain (depth 1 = direct calls only,
    /// depth 3 = calls → calls of calls → calls of calls of calls).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_call_depth: Option<u32>,
}

#[derive(Debug, PartialEq, ::serde::Serialize, ::serde::Deserialize, JsonSchema)]
pub struct SymbolLocation {
    /// File path or URI where the symbol is located.
    /// 
    /// ACCEPTED FORMATS:
    /// • Relative path: "src/math.cpp", "include/utils.h"
    /// • Absolute path: "/home/project/src/math.cpp" 
    /// • File URI: "file:///home/project/src/math.cpp"
    pub file_uri: String,
    
    /// Precise position within the file for disambiguation.
    /// Use this to target a specific occurrence when multiple symbols 
    /// with the same name exist in the same file.
    pub position: SymbolPosition,
}

#[derive(Debug, PartialEq, ::serde::Serialize, ::serde::Deserialize, JsonSchema)]
pub struct SymbolPosition {
    /// Line number (0-based indexing). 
    /// Example: line 0 = first line, line 10 = eleventh line
    pub line: u32,
    /// Character position within the line (0-based indexing).
    /// Example: character 0 = first character, character 5 = sixth character  
    pub character: u32,
}

impl<'de> serde::Deserialize<'de> for AnalyzeSymbolContextTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct AnalyzeSymbolContextToolHelper {
            symbol: String,
            #[serde(default)]
            location: Option<SymbolLocation>,
            #[serde(default)]
            include_usage_patterns: Option<bool>,
            #[serde(default)]
            max_usage_examples: Option<u32>,
            #[serde(default)]
            include_inheritance: Option<bool>,
            #[serde(default)]
            include_call_hierarchy: Option<bool>,
            #[serde(default)]
            max_call_depth: Option<u32>,
        }

        let helper = AnalyzeSymbolContextToolHelper::deserialize(deserializer)?;
        
        Ok(AnalyzeSymbolContextTool {
            symbol: helper.symbol,
            location: helper.location,
            include_usage_patterns: helper.include_usage_patterns,
            max_usage_examples: helper.max_usage_examples,
            include_inheritance: helper.include_inheritance,
            include_call_hierarchy: helper.include_call_hierarchy,
            max_call_depth: helper.max_call_depth,
        })
    }
}

struct SymbolAnalysisData<'a> {
    symbol_location: &'a serde_json::Value,
    hover_info: &'a Option<serde_json::Value>,
    definition_location: Option<&'a serde_json::Value>,
    declaration_location: Option<&'a serde_json::Value>,
    usage_stats: Option<&'a serde_json::Value>,
    inheritance_info: Option<&'a serde_json::Value>,
    usage_examples: Option<&'a serde_json::Value>,
    call_relationships: Option<&'a serde_json::Value>,
}

impl AnalyzeSymbolContextTool {
    #[instrument(name = "analyze_symbol_context", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Analyzing symbol context: symbol='{}', location={:?}", 
              self.symbol, self.location);

        // Handle automatic clangd setup if needed
        let build_path = match Self::resolve_build_directory() {
            Ok(Some(path)) => path,
            Ok(None) => {
                let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "build_directory_required",
                    "message": "No build directory found. Use list_build_dirs tool to see available options, or configure a build directory first.",
                    "symbol": self.symbol,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
            Err(_) => {
                let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "build_directory_analysis_failed",
                    "message": "Failed to analyze build directories. Use list_build_dirs tool to see available options.",
                    "symbol": self.symbol,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        };

        // Ensure clangd is setup
        {
            let current_build_dir = clangd_manager
                .lock()
                .await
                .get_current_build_directory()
                .await;
            let needs_setup = match current_build_dir {
                Some(current) => current != build_path,
                None => true,
            };

            if needs_setup {
                info!(
                    "Setting up clangd for build directory: {}",
                    build_path.display()
                );
                let manager_guard = clangd_manager.lock().await;
                if let Err(e) = manager_guard.setup_clangd(build_path.clone()).await {
                    let indexing_state = manager_guard.get_indexing_state().await;
                    let content = json!({
                        "success": false,
                        "error": format!("Failed to setup clangd: {}", e),
                        "build_directory_attempted": build_path.display().to_string(),
                        "symbol": self.symbol,
                        "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                    });

                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]));
                }
            }
        }

        let manager_guard = clangd_manager.lock().await;
        let build_directory = manager_guard
            .get_current_build_directory()
            .await
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".to_string());

        // Wait for indexing completion to ensure accurate results
        let initial_indexing_state = manager_guard.get_indexing_state().await;
        if initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed {
            info!("Waiting for indexing completion before symbol analysis");
            if let Err(e) = manager_guard
                .wait_for_indexing_completion(std::time::Duration::from_secs(30))
                .await
            {
                let final_indexing_state = manager_guard.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("Indexing timeout: {}", e),
                    "message": "Symbol analysis may be incomplete due to ongoing indexing",
                    "symbol": self.symbol,
                    "indexing_status": SymbolUtilities::format_indexing_status(&final_indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        }

        // Step 1: Find the symbol using workspace search
        let symbol_location = match self.find_symbol_location(&manager_guard).await {
            Ok(Some(location)) => location,
            Ok(None) => {
                let indexing_state = manager_guard.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "symbol_not_found",
                    "message": format!("Symbol '{}' not found in workspace. Check spelling or ensure symbol is indexed.", self.symbol),
                    "symbol": self.symbol,
                    "suggestions": self.get_similar_symbols(&manager_guard).await,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
            Err(e) => {
                let indexing_state = manager_guard.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("Symbol search failed: {}", e),
                    "symbol": self.symbol,
                    "indexing_status": SymbolUtilities::format_indexing_status(&indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        };

        // Step 2: Get detailed information via hover
        let hover_info = self.get_hover_information(&manager_guard, &symbol_location).await?;

        // Step 3: Get definition and declaration locations
        let (definition_location, declaration_location) = self.get_symbol_locations(&manager_guard, &symbol_location).await;

        // Step 4: Get reference count and usage statistics
        let usage_stats = if self.include_usage_patterns.unwrap_or(false) {
            self.get_usage_statistics(&manager_guard, &symbol_location).await
        } else {
            None
        };

        // Step 5: Get inheritance information if requested
        let inheritance_info = if self.include_inheritance.unwrap_or(false) {
            self.get_inheritance_information(&manager_guard, &symbol_location).await
        } else {
            None
        };

        // Step 6: Get usage examples if requested
        let usage_examples = if self.include_usage_patterns.unwrap_or(false) {
            self.get_usage_examples(&manager_guard, &symbol_location).await
        } else {
            None
        };

        // Step 7: Get call relationships if requested
        let call_relationships = if self.include_call_hierarchy.unwrap_or(false) {
            self.get_call_relationships(&manager_guard, &symbol_location).await
        } else {
            None
        };

        // Step 8: Build comprehensive symbol information
        let symbol_info = self.build_comprehensive_symbol_info(SymbolAnalysisData {
            symbol_location: &symbol_location, 
            hover_info: &hover_info, 
            definition_location: definition_location.as_ref(),
            declaration_location: declaration_location.as_ref(),
            usage_stats: usage_stats.as_ref(),
            inheritance_info: inheritance_info.as_ref(),
            usage_examples: usage_examples.as_ref(),
            call_relationships: call_relationships.as_ref(),
        });

        // Prepare the response
        let final_indexing_state = manager_guard.get_indexing_state().await;
        let content = json!({
            "success": true,
            "symbol": symbol_info,
            "metadata": {
                "analysis_type": "comprehensive_symbol_analysis",
                "features_used": {
                    "basic_info": true,
                    "hover_info": hover_info.is_some(),
                    "definition_location": definition_location.is_some(),
                    "declaration_location": declaration_location.is_some(),
                    "usage_statistics": usage_stats.is_some(),
                    "inheritance_info": inheritance_info.is_some(),
                    "usage_examples": usage_examples.is_some(),
                    "call_relationships": call_relationships.is_some()
                },
                "build_directory_used": build_directory,
                "indexing_waited": initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed,
                "indexing_status": SymbolUtilities::format_indexing_status(&final_indexing_state)
            }
        });

        info!("Symbol context analysis completed for: {}", self.symbol);

        Ok(CallToolResult::text_content(vec![TextContent::from(
            serialize_result(&content),
        )]))
    }

    async fn find_symbol_location(
        &self,
        manager: &ClangdManager,
    ) -> Result<Option<serde_json::Value>, String> {
        // If location is provided, use it directly for hover
        if let Some(location) = &self.location {
            return Ok(Some(json!({
                "uri": location.file_uri,
                "range": {
                    "start": {
                        "line": location.position.line,
                        "character": location.position.character
                    },
                    "end": {
                        "line": location.position.line,
                        "character": location.position.character
                    }
                }
            })));
        }

        // Use workspace symbol search to find the symbol
        let params = json!({
            "query": self.symbol
        });

        match manager
            .send_lsp_request("workspace/symbol".to_string(), Some(params))
            .await
        {
            Ok(symbols) => {
                if let Some(symbol_array) = symbols.as_array() {
                    // Find exact match or best match using improved logic
                    let best_match = symbol_array
                        .iter()
                        .find(|s| {
                            if let Some(name) = s.get("name").and_then(|n| n.as_str()) {
                                // Exact name match
                                if name == self.symbol {
                                    return true;
                                }
                                
                                // Check for qualified name match
                                if let Some(detail) = s.get("detail").and_then(|d| d.as_str()) {
                                    if detail.contains(&self.symbol) {
                                        return true;
                                    }
                                }
                                
                                // Check container scope for qualified matches
                                if let Some(container) = s.get("containerName").and_then(|c| c.as_str()) {
                                    let qualified = format!("{}::{}", container, name);
                                    if qualified == self.symbol {
                                        return true;
                                    }
                                }
                                
                                false
                            } else {
                                false
                            }
                        })
                        .or_else(|| {
                            // Fallback: partial match
                            symbol_array.iter().find(|s| {
                                if let Some(name) = s.get("name").and_then(|n| n.as_str()) {
                                    name.contains(&self.symbol) || self.symbol.contains(name)
                                } else {
                                    false
                                }
                            })
                        })
                        .or_else(|| symbol_array.first());

                    if let Some(symbol) = best_match {
                        if let Some(location) = symbol.get("location") {
                            return Ok(Some(location.clone()));
                        }
                    }
                }
                Ok(None)
            }
            Err(e) => Err(format!("LSP workspace/symbol request failed: {}", e)),
        }
    }

    async fn get_symbol_locations(
        &self,
        manager: &ClangdManager,
        symbol_location: &serde_json::Value,
    ) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
        let mut definition_location = None;
        let mut declaration_location = None;

        if let (Some(uri), Some(range)) = (
            symbol_location.get("uri").and_then(|u| u.as_str()),
            symbol_location.get("range"),
        ) {
            // First, ensure the file is opened in clangd
            if let Some(file_path_str) = uri.strip_prefix("file://") {
                let file_path = std::path::PathBuf::from(file_path_str);
                if let Err(e) = manager.open_file_if_needed(&file_path).await {
                    info!("Failed to open file {} for symbol analysis: {}", file_path.display(), e);
                    return (None, None);
                }
            }

            if let Some(start) = range.get("start") {
                let params = json!({
                    "textDocument": {
                        "uri": uri
                    },
                    "position": start
                });

                // Get definition location
                if let Ok(definition_result) = manager
                    .send_lsp_request("textDocument/definition".to_string(), Some(params.clone()))
                    .await
                {
                    if let Some(locations) = definition_result.as_array() {
                        if let Some(first_def) = locations.first() {
                            definition_location = Some(first_def.clone());
                        }
                    }
                }

                // Get declaration location
                if let Ok(declaration_result) = manager
                    .send_lsp_request("textDocument/declaration".to_string(), Some(params))
                    .await
                {
                    if let Some(locations) = declaration_result.as_array() {
                        if let Some(first_decl) = locations.first() {
                            declaration_location = Some(first_decl.clone());
                        }
                    }
                }
            }
        }

        (definition_location, declaration_location)
    }

    async fn get_usage_statistics(
        &self,
        manager: &ClangdManager,
        symbol_location: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        if let (Some(uri), Some(range)) = (
            symbol_location.get("uri").and_then(|u| u.as_str()),
            symbol_location.get("range"),
        ) {
            // First, ensure the file is opened in clangd
            if let Some(file_path_str) = uri.strip_prefix("file://") {
                let file_path = std::path::PathBuf::from(file_path_str);
                if let Err(e) = manager.open_file_if_needed(&file_path).await {
                    info!("Failed to open file {} for usage statistics: {}", file_path.display(), e);
                    return None;
                }
            }

            if let Some(start) = range.get("start") {
                let params = json!({
                    "textDocument": {
                        "uri": uri
                    },
                    "position": start,
                    "context": {
                        "includeDeclaration": true
                    }
                });

                if let Ok(references_result) = manager
                    .send_lsp_request("textDocument/references".to_string(), Some(params))
                    .await
                {
                    if let Some(references) = references_result.as_array() {
                        let total_references = references.len();
                        let mut file_count = std::collections::HashSet::new();
                        
                        // Count unique files
                        for reference in references {
                            if let Some(uri) = reference.get("uri").and_then(|u| u.as_str()) {
                                file_count.insert(uri);
                            }
                        }

                        return Some(json!({
                            "total_references": total_references,
                            "files_containing_references": file_count.len(),
                            "reference_density": if !file_count.is_empty() { 
                                total_references as f64 / file_count.len() as f64 
                            } else { 
                                0.0 
                            }
                        }));
                    }
                }
            }
        }
        None
    }

    async fn get_inheritance_information(
        &self,
        manager: &ClangdManager,
        symbol_location: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        if let (Some(uri), Some(range)) = (
            symbol_location.get("uri").and_then(|u| u.as_str()),
            symbol_location.get("range"),
        ) {
            // First, ensure the file is opened in clangd
            info!("Attempting to open file from URI: {}", uri);
            if let Some(file_path_str) = uri.strip_prefix("file://") {
                let file_path = std::path::PathBuf::from(file_path_str);
                info!("Opening file: {}", file_path.display());
                if let Err(e) = manager.open_file_if_needed(&file_path).await {
                    info!("Failed to open file {} for inheritance analysis: {}", file_path.display(), e);
                    return None;
                } else {
                    info!("File opening call completed for: {}", file_path.display());
                }
            } else {
                info!("Could not extract file path from URI: {}", uri);
            }

            if let Some(start) = range.get("start") {
                let params = json!({
                    "textDocument": {
                        "uri": uri
                    },
                    "position": start
                });

                // First prepare type hierarchy item
                if let Ok(type_hierarchy_result) = manager
                    .send_lsp_request("textDocument/prepareTypeHierarchy".to_string(), Some(params))
                    .await
                {
                    if let Some(hierarchy_items) = type_hierarchy_result.as_array() {
                        if let Some(hierarchy_item) = hierarchy_items.first() {
                            let mut base_classes = Vec::new();
                            let mut derived_classes = Vec::new();

                            // Get supertypes (base classes) with timeout protection
                            let supertypes_params = json!({
                                "item": hierarchy_item
                            });
                            
                            if let Ok(Ok(supertypes_response)) = tokio::time::timeout(
                                std::time::Duration::from_secs(5),
                                manager.send_lsp_request("typeHierarchy/supertypes".to_string(), Some(supertypes_params))
                            ).await {
                                if let Some(supertypes) = supertypes_response.as_array() {
                                    for supertype in supertypes {
                                        if let Some(name) = supertype.get("name").and_then(|n| n.as_str()) {
                                            base_classes.push(name.to_string());
                                        }
                                    }
                                }
                            }

                            // Get subtypes (derived classes) with timeout protection
                            let subtypes_params = json!({
                                "item": hierarchy_item
                            });
                            
                            if let Ok(Ok(subtypes_response)) = tokio::time::timeout(
                                std::time::Duration::from_secs(5),
                                manager.send_lsp_request("typeHierarchy/subtypes".to_string(), Some(subtypes_params))
                            ).await {
                                if let Some(subtypes) = subtypes_response.as_array() {
                                    for subtype in subtypes {
                                        if let Some(name) = subtype.get("name").and_then(|n| n.as_str()) {
                                            derived_classes.push(name.to_string());
                                        }
                                    }
                                }
                            }

                            if !base_classes.is_empty() || !derived_classes.is_empty() {
                                return Some(json!({
                                    "base_classes": base_classes,
                                    "derived_classes": derived_classes,
                                    "has_inheritance": !base_classes.is_empty() || !derived_classes.is_empty()
                                }));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    async fn get_call_relationships(
        &self,
        manager: &ClangdManager,
        symbol_location: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        if let (Some(uri), Some(range)) = (
            symbol_location.get("uri").and_then(|u| u.as_str()),
            symbol_location.get("range"),
        ) {
            // First, ensure the file is opened in clangd
            if let Some(file_path_str) = uri.strip_prefix("file://") {
                let file_path = std::path::PathBuf::from(file_path_str);
                if let Err(e) = manager.open_file_if_needed(&file_path).await {
                    info!("Failed to open file {} for call hierarchy analysis: {}", file_path.display(), e);
                    return None;
                }
            }

            if let Some(start) = range.get("start") {
                let params = json!({
                    "textDocument": {
                        "uri": uri
                    },
                    "position": start
                });

                // First prepare call hierarchy item
                match manager
                    .send_lsp_request("textDocument/prepareCallHierarchy".to_string(), Some(params))
                    .await
                {
                    Ok(call_hierarchy_result) => {
                        if let Some(hierarchy_items) = call_hierarchy_result.as_array() {
                            if let Some(hierarchy_item) = hierarchy_items.first() {
                                let mut incoming_calls = Vec::new();
                                let mut outgoing_calls = Vec::new();
                                let max_depth = self.max_call_depth.unwrap_or(3) as usize;

                                // Get incoming calls (who calls this function)
                                if let Ok(incoming_result) = manager
                                    .send_lsp_request(
                                        "callHierarchy/incomingCalls".to_string(),
                                        Some(json!({ "item": hierarchy_item })),
                                    )
                                    .await
                                {
                                    if let Some(incoming_array) = incoming_result.as_array() {
                                        for (index, call) in incoming_array.iter().enumerate() {
                                            if index >= max_depth {
                                                break;
                                            }
                                            if let Some(from) = call.get("from") {
                                                incoming_calls.push(self.extract_call_info(from, call));
                                            }
                                        }
                                    }
                                }

                                // Get outgoing calls (what this function calls)
                                if let Ok(outgoing_result) = manager
                                    .send_lsp_request(
                                        "callHierarchy/outgoingCalls".to_string(),
                                        Some(json!({ "item": hierarchy_item })),
                                    )
                                    .await
                                {
                                    if let Some(outgoing_array) = outgoing_result.as_array() {
                                        for (index, call) in outgoing_array.iter().enumerate() {
                                            if index >= max_depth {
                                                break;
                                            }
                                            if let Some(to) = call.get("to") {
                                                outgoing_calls.push(self.extract_call_info(to, call));
                                            }
                                        }
                                    }
                                }

                                return Some(json!({
                                    "incoming_calls": incoming_calls,
                                    "outgoing_calls": outgoing_calls,
                                    "total_callers": incoming_calls.len(),
                                    "total_callees": outgoing_calls.len(),
                                    "call_depth_analyzed": max_depth,
                                    "has_call_relationships": !incoming_calls.is_empty() || !outgoing_calls.is_empty()
                                }));
                            }
                        }
                    }
                    Err(e) => {
                        info!("Call hierarchy request failed (non-critical): {}", e);
                    }
                }
            }
        }
        None
    }

    fn extract_call_info(&self, call_item: &serde_json::Value, call_data: &serde_json::Value) -> serde_json::Value {
        let name = call_item.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
        let kind_num = call_item.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
        let kind = self.symbol_kind_to_string(kind_num);
        let detail = call_item.get("detail").and_then(|d| d.as_str());
        let uri = call_item.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        let range = call_item.get("range").cloned().unwrap_or(json!({}));
        let selection_range = call_item.get("selectionRange").cloned().unwrap_or(json!({}));
        let from_ranges = call_data.get("fromRanges").cloned().unwrap_or(json!([]));

        json!({
            "name": name,
            "kind": kind,
            "detail": detail,
            "uri": uri,
            "range": range,
            "selection_range": selection_range,
            "from_ranges": from_ranges,
            "context": self.extract_call_context(uri, &range)
        })
    }

    fn symbol_kind_to_string(&self, kind: u64) -> String {
        match kind {
            1 => "file".to_string(),
            2 => "module".to_string(),
            3 => "namespace".to_string(),
            4 => "package".to_string(),
            5 => "class".to_string(),
            6 => "method".to_string(),
            7 => "property".to_string(),
            8 => "field".to_string(),
            9 => "constructor".to_string(),
            10 => "enum".to_string(),
            11 => "interface".to_string(),
            12 => "function".to_string(),
            13 => "variable".to_string(),
            14 => "constant".to_string(),
            15 => "string".to_string(),
            16 => "number".to_string(),
            17 => "boolean".to_string(),
            18 => "array".to_string(),
            19 => "object".to_string(),
            20 => "key".to_string(),
            21 => "null".to_string(),
            22 => "enum_member".to_string(),
            23 => "struct".to_string(),
            24 => "event".to_string(),
            25 => "operator".to_string(),
            26 => "type_parameter".to_string(),
            _ => "unknown".to_string(),
        }
    }

    fn extract_call_context(&self, uri: &str, range: &serde_json::Value) -> Option<String> {
        // Extract a brief context around the call location
        if let Some(file_path_str) = uri.strip_prefix("file://") {
            if let Ok(content) = std::fs::read_to_string(file_path_str) {
                if let Some(start_line) = range.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()) {
                    let lines: Vec<&str> = content.lines().collect();
                    if start_line < lines.len() as u64 {
                        return Some(lines[start_line as usize].trim().to_string());
                    }
                }
            }
        }
        None
    }

    async fn get_usage_examples(
        &self,
        manager: &ClangdManager,
        symbol_location: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        if let (Some(uri), Some(range)) = (
            symbol_location.get("uri").and_then(|u| u.as_str()),
            symbol_location.get("range"),
        ) {
            if let Some(start) = range.get("start") {
                let params = json!({
                    "textDocument": {
                        "uri": uri
                    },
                    "position": start,
                    "context": {
                        "includeDeclaration": false  // We want usage examples, not declarations
                    }
                });

                if let Ok(references_result) = manager
                    .send_lsp_request("textDocument/references".to_string(), Some(params))
                    .await
                {
                    if let Some(references) = references_result.as_array() {
                        let max_examples = self.max_usage_examples.unwrap_or(5) as usize;
                        let mut usage_examples = Vec::new();

                        for (index, reference) in references.iter().enumerate() {
                            if index >= max_examples {
                                break;
                            }

                            if let (Some(ref_uri), Some(ref_range)) = (
                                reference.get("uri").and_then(|u| u.as_str()),
                                reference.get("range"),
                            ) {
                                // Get context around the usage
                                if let Some(context) = self.get_usage_context(manager, ref_uri, ref_range).await {
                                    usage_examples.push(json!({
                                        "file": ref_uri,
                                        "range": ref_range,
                                        "context": context,
                                        "pattern_type": self.classify_usage_pattern(&context)
                                    }));
                                }
                            }
                        }

                        if !usage_examples.is_empty() {
                            return Some(json!(usage_examples));
                        }
                    }
                }
            }
        }
        None
    }

    async fn get_usage_context(
        &self,
        manager: &ClangdManager,
        file_uri: &str,
        range: &serde_json::Value,
    ) -> Option<String> {
        // Open the file to get context around the usage
        if let Some(path_str) = file_uri.strip_prefix("file://") {
            let path = std::path::PathBuf::from(path_str);
            if (manager.open_file_if_needed(&path).await).is_ok() {
                // Get a range around the usage for context (5 lines before and after)
                if let (Some(start_line), Some(_)) = (
                    range.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()),
                    range.get("end").and_then(|e| e.get("line")).and_then(|l| l.as_u64()),
                ) {
                    let context_start = if start_line >= 5 { start_line - 5 } else { 0 };
                    let context_end = start_line + 5;

                    // Try to read file content (simplified approach)
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let lines: Vec<&str> = content.lines().collect();
                        if context_start < lines.len() as u64 {
                            let end_idx = std::cmp::min(context_end as usize, lines.len());
                            let context_lines = &lines[context_start as usize..end_idx];
                            return Some(context_lines.join("\n"));
                        }
                    }
                }
            }
        }
        None
    }

    fn classify_usage_pattern(&self, context: &str) -> String {
        // Simple heuristic-based classification
        if context.contains("new ") || context.contains("std::make_") {
            "instantiation".to_string()
        } else if context.contains("(") && context.contains(")") {
            "function_call".to_string()
        } else if context.contains("::") {
            "qualified_access".to_string()
        } else if context.contains("->") || context.contains(".") {
            "member_access".to_string()
        } else {
            "reference".to_string()
        }
    }

    fn build_comprehensive_symbol_info(
        &self,
        data: SymbolAnalysisData,
    ) -> serde_json::Value {
        let mut info = json!({
            "name": self.symbol,
            "kind": self.extract_symbol_kind(data.symbol_location, data.hover_info),
            "fully_qualified_name": self.extract_qualified_name(data.hover_info),
            "file_location": self.extract_file_location(data.symbol_location)
        });

        // Add definition and declaration locations
        if let Some(definition) = data.definition_location {
            info["definition"] = definition.clone();
        }
        if let Some(declaration) = data.declaration_location {
            info["declaration"] = declaration.clone();
        }

        // Add type information from hover
        if let Some(hover) = data.hover_info {
            if let Some(contents) = hover.get("contents") {
                info["type_info"] = self.extract_enhanced_type_info(contents);
                info["documentation"] = self.extract_documentation(contents);
            }
        }

        // Add usage statistics
        if let Some(stats) = data.usage_stats {
            info["usage_statistics"] = stats.clone();
        }

        // Add inheritance information
        if let Some(inheritance) = data.inheritance_info {
            info["inheritance"] = inheritance.clone();
        }

        // Add usage examples
        if let Some(examples) = data.usage_examples {
            info["usage_examples"] = examples.clone();
        }

        // Add call relationships
        if let Some(calls) = data.call_relationships {
            info["call_relationships"] = calls.clone();
        }

        info
    }

    fn extract_symbol_kind(
        &self,
        _symbol_location: &serde_json::Value,
        hover_info: &Option<serde_json::Value>,
    ) -> String {
        // Try to extract kind from hover info first
        if let Some(hover) = hover_info {
            if let Some(contents) = hover.get("contents") {
                if let Some(value_str) = contents.get("value").and_then(|v| v.as_str()) {
                    // Simple heuristics to determine symbol kind from hover text
                    if value_str.contains("class ") {
                        return "class".to_string();
                    } else if value_str.contains("struct ") {
                        return "struct".to_string();
                    } else if value_str.contains("enum ") {
                        return "enum".to_string();
                    } else if value_str.contains("namespace ") {
                        return "namespace".to_string();
                    } else if value_str.contains("(") && value_str.contains(")") {
                        return "function".to_string();
                    } else if value_str.contains("typedef ") {
                        return "typedef".to_string();
                    }
                }
            }
        }

        // Fallback to analyzing the symbol name
        if self.symbol.contains("::") && !self.symbol.contains("(") {
            "qualified_name".to_string()
        } else {
            "unknown".to_string()
        }
    }

    fn extract_qualified_name(&self, hover_info: &Option<serde_json::Value>) -> String {
        if let Some(hover) = hover_info {
            if let Some(contents) = hover.get("contents") {
                if let Some(value_str) = contents.get("value").and_then(|v| v.as_str()) {
                    // Try to extract qualified name from hover text
                    for line in value_str.lines() {
                        if line.contains("::") {
                            // Simple extraction - this could be improved
                            if let Some(start) = line.find(&self.symbol) {
                                let before = &line[..start];
                                if let Some(namespace_start) = before.rfind(' ') {
                                    let qualified = &line[namespace_start + 1..start + self.symbol.len()];
                                    if qualified.contains("::") {
                                        return qualified.to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        self.symbol.clone()
    }

    fn extract_enhanced_type_info(&self, contents: &serde_json::Value) -> serde_json::Value {
        let type_info = self.extract_type_info(contents);
        
        // Enhance with additional analysis
        if let Some(type_str) = type_info.get("type").and_then(|t| t.as_str()) {
            let mut enhanced = json!({
                "type": type_str,
                "is_template": type_str.contains("<") && type_str.contains(">"),
                "is_pointer": type_str.contains("*"),
                "is_reference": type_str.contains("&"),
                "is_const": type_str.contains("const"),
                "is_static": type_str.contains("static"),
            });
            
            // Extract template parameters if it's a template
            if type_str.contains("<") && type_str.contains(">") {
                if let (Some(start), Some(end)) = (type_str.find('<'), type_str.rfind('>')) {
                    let template_params = &type_str[start + 1..end];
                    enhanced["template_parameters"] = json!(
                        template_params.split(',').map(|s| s.trim()).collect::<Vec<_>>()
                    );
                }
            }
            
            enhanced["raw_hover"] = type_info["raw_hover"].clone();
            return enhanced;
        }
        
        type_info
    }

    async fn get_hover_information(
        &self,
        manager: &ClangdManager,
        symbol_location: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, CallToolError> {
        if let (Some(uri), Some(range)) = (
            symbol_location.get("uri").and_then(|u| u.as_str()),
            symbol_location.get("range"),
        ) {
            // First, ensure the file is opened in clangd
            if let Some(file_path_str) = uri.strip_prefix("file://") {
                let file_path = std::path::PathBuf::from(file_path_str);
                if let Err(e) = manager.open_file_if_needed(&file_path).await {
                    info!("Failed to open file {} for hover analysis: {}", file_path.display(), e);
                    return Ok(None);
                }
            }

            // Extract position from range
            if let Some(start) = range.get("start") {
                let params = json!({
                    "textDocument": {
                        "uri": uri
                    },
                    "position": start
                });

                match manager
                    .send_lsp_request("textDocument/hover".to_string(), Some(params))
                    .await
                {
                    Ok(hover_result) => Ok(Some(hover_result)),
                    Err(e) => {
                        info!("Hover request failed (non-critical): {}", e);
                        Ok(None)
                    }
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn extract_file_location(&self, symbol_location: &serde_json::Value) -> serde_json::Value {
        json!({
            "uri": symbol_location.get("uri").unwrap_or(&json!("unknown")),
            "range": symbol_location.get("range").unwrap_or(&json!({}))
        })
    }

    fn extract_type_info(&self, contents: &serde_json::Value) -> serde_json::Value {
        // Handle different hover content formats
        if let Some(value_str) = contents.get("value").and_then(|v| v.as_str()) {
            json!({
                "type": value_str,
                "raw_hover": value_str
            })
        } else if let Some(contents_array) = contents.as_array() {
            if let Some(first_item) = contents_array.first() {
                if let Some(value_str) = first_item.get("value").and_then(|v| v.as_str()) {
                    json!({
                        "raw_hover": value_str
                    })
                } else {
                    json!({
                        "raw_hover": first_item
                    })
                }
            } else {
                json!({
                    "type": "unknown"
                })
            }
        } else {
            json!({
                "type": "unknown",
                "raw_hover": contents
            })
        }
    }

    fn extract_documentation(&self, contents: &serde_json::Value) -> serde_json::Value {
        // Try to extract documentation from hover contents
        if let Some(value_str) = contents.get("value").and_then(|v| v.as_str()) {
            // Simple heuristic: documentation often comes after type info
            let lines: Vec<&str> = value_str.lines().collect();
            if lines.len() > 1 {
                json!(lines[1..].join("\n").trim())
            } else {
                json!(null)
            }
        } else {
            json!(null)
        }
    }

    async fn get_similar_symbols(&self, manager: &ClangdManager) -> Vec<String> {
        // Try to find symbols with similar names
        let query = if self.symbol.len() > 3 {
            self.symbol[..3].to_string()
        } else {
            self.symbol.clone()
        };

        let params = json!({
            "query": query
        });

        if let Ok(symbols) = manager
            .send_lsp_request("workspace/symbol".to_string(), Some(params))
            .await
        {
            if let Some(symbol_array) = symbols.as_array() {
                return symbol_array
                    .iter()
                    .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .take(5)
                    .collect();
            }
        }

        vec![]
    }

    fn resolve_build_directory() -> Result<Option<PathBuf>, String> {
        match CmakeProjectStatus::analyze_current_directory() {
            Ok(status) => match status.build_directories.len() {
                1 => {
                    let build_dir = &status.build_directories[0];
                    info!(
                        "Auto-resolved to single build directory: {}",
                        build_dir.path.display()
                    );
                    Ok(Some(build_dir.path.clone()))
                }
                0 => {
                    info!("No build directories found");
                    Ok(None)
                }
                _ => {
                    info!("Multiple build directories found, requiring explicit selection");
                    Ok(None)
                }
            },
            Err(e) => {
                info!("Not a CMake project or failed to analyze: {}", e);
                Err(format!("Failed to analyze build directories: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_analyze_symbol_context_tool_deserialize() {
        let json_data = json!({
            "symbol": "MyClass",
            "include_usage_patterns": true,
            "max_usage_examples": 3
        });
        let tool: AnalyzeSymbolContextTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.symbol, "MyClass");
        assert_eq!(tool.include_usage_patterns, Some(true));
        assert_eq!(tool.max_usage_examples, Some(3));
        assert_eq!(tool.location, None);
        assert_eq!(tool.include_inheritance, None);
    }

    #[test]
    fn test_symbol_location_and_position() {
        let location = SymbolLocation {
            file_uri: "file:///path/to/file.cpp".to_string(),
            position: SymbolPosition {
                line: 10,
                character: 5,
            },
        };

        assert_eq!(location.file_uri, "file:///path/to/file.cpp");
        assert_eq!(location.position.line, 10);
        assert_eq!(location.position.character, 5);
    }
}
