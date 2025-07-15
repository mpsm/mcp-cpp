use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument};

use crate::cmake::{CmakeError, CmakeProjectStatus};
use crate::lsp::{ClangdManager, LspError};

/// Helper function to serialize JSON content and handle errors gracefully
fn serialize_result(content: &serde_json::Value) -> String {
    serde_json::to_string_pretty(content)
        .unwrap_or_else(|e| format!("Error serializing result: {}", e))
}

/// MCP Tool Design Guidelines
///
/// **Async vs Sync Pattern:**
/// - Tools that interact with external processes, file I/O, or network should be **async**
/// - Tools that perform pure computation or analysis on in-memory data should be **sync**
///
/// **Current Tool Classifications:**
/// - `ListBuildDirsTool` - **sync** (file system analysis, no process interaction)
/// - `LspRequestTool` - **async** (sends requests to LSP process)
///
/// **Error Handling Pattern:**
/// - All tools use `CallToolResult::text_content()` for responses
/// - All tools use `serialize_result()` helper for consistent JSON formatting
/// - Errors are logged with appropriate level (error, warn, info) before returning

#[mcp_tool(
    name = "list_build_dirs",
    description = "List available CMake build directories with their configurations, generators, build types, and options. Use this to discover build directories before using other compilation-dependent tools."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct ListBuildDirsTool {
    // No parameters needed for analyzing current directory
}

impl ListBuildDirsTool {
    #[instrument(name = "list_build_dirs", skip(self))]
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        info!("Listing available CMake build directories");

        match CmakeProjectStatus::analyze_current_directory() {
            Ok(status) => {
                let content = json!({
                    "success": true,
                    "project_type": "cmake",
                    "is_configured": !status.build_directories.is_empty(),
                    "build_directories": status.build_directories.iter().map(|bd| {
                        json!({
                            "path": bd.relative_path,
                            "absolute_path": bd.path,
                            "generator": bd.generator,
                            "build_type": bd.build_type,
                            "cache_exists": bd.cache_exists,
                            "cache_readable": bd.cache_readable,
                            "configured_options": bd.configured_options.iter().map(|(k, v)| {
                                json!({ "name": k, "value": v })
                            }).collect::<Vec<_>>()
                        })
                    }).collect::<Vec<_>>(),
                    "issues": status.issues,
                    "summary": Self::generate_summary(&status)
                });

                info!("Successfully listed CMake build directories");

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(CmakeError::NotCmakeProject) => {
                let content = json!({
                    "success": true,
                    "project_type": "unknown",
                    "is_configured": false,
                    "message": "Current directory is not a CMake project (no CMakeLists.txt found)",
                    "build_directories": [],
                    "issues": [],
                    "summary": "Not a CMake project"
                });

                info!("Directory is not a CMake project");

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(CmakeError::MultipleIssues(issues)) => {
                let content = json!({
                    "success": false,
                    "project_type": "cmake",
                    "is_configured": false,
                    "error": "Multiple issues detected with build directories",
                    "build_directories": [],
                    "issues": issues,
                    "summary": format!("CMake project with {} issues", issues.len())
                });

                error!("Multiple issues detected: {:?}", issues);

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(e) => {
                let error_msg = format!("Failed to analyze project: {}", e);
                let content = json!({
                    "success": false,
                    "project_type": "unknown",
                    "is_configured": false,
                    "error": error_msg,
                    "build_directories": [],
                    "issues": [error_msg.clone()],
                    "summary": "Analysis failed"
                });

                error!("Project analysis failed: {}", e);

                // Return error result with JSON content
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
        }
    }

    fn generate_summary(status: &CmakeProjectStatus) -> String {
        if !status.is_cmake_project {
            return "Not a CMake project".to_string();
        }

        match status.build_directories.len() {
            0 => "CMake project (not configured)".to_string(),
            1 => {
                let bd = &status.build_directories[0];
                let generator = bd.generator.as_deref().unwrap_or("unknown generator");
                let build_type = bd.build_type.as_deref().unwrap_or("unspecified");
                format!(
                    "CMake project configured with {} ({})",
                    generator, build_type
                )
            }
            n => {
                let generators: Vec<String> = status
                    .build_directories
                    .iter()
                    .filter_map(|bd| bd.generator.as_ref())
                    .cloned()
                    .collect();
                let unique_generators: std::collections::HashSet<_> =
                    generators.into_iter().collect();

                if unique_generators.len() == 1 {
                    let generator = unique_generators
                        .iter()
                        .next()
                        .map(|g| g.as_str())
                        .unwrap_or("unknown generator");
                    format!("CMake project with {} build directories ({})", n, generator)
                } else {
                    format!(
                        "CMake project with {} build directories (mixed generators)",
                        n
                    )
                }
            }
        }
    }
}

#[derive(Debug, ::serde::Serialize, JsonSchema)]
pub struct LspRequestTool {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(
        rename = "buildDirectory",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub build_directory: Option<String>,
}

impl<'de> serde::Deserialize<'de> for LspRequestTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct LspRequestToolHelper {
            method: String,
            #[serde(default)]
            params: Option<serde_json::Value>,
            #[serde(rename = "buildDirectory", default)]
            build_directory: Option<String>,
        }

        let helper = LspRequestToolHelper::deserialize(deserializer)?;

        let processed_params = match helper.params {
            Some(value) => {
                match value {
                    // If it's already a proper JSON object/array/primitive, use as-is
                    serde_json::Value::Object(_)
                    | serde_json::Value::Array(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::Bool(_)
                    | serde_json::Value::Null => Some(value),

                    // If it's a string, try to parse it as JSON
                    serde_json::Value::String(s) => {
                        if s.trim().is_empty() {
                            None
                        } else {
                            match serde_json::from_str::<serde_json::Value>(&s) {
                                Ok(parsed) => {
                                    tracing::debug!(
                                        "Successfully parsed JSON string params: {} -> {:?}",
                                        s,
                                        parsed
                                    );
                                    Some(parsed)
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to parse params as JSON string '{}': {}. Using as literal string.",
                                        s,
                                        e
                                    );
                                    Some(serde_json::Value::String(s))
                                }
                            }
                        }
                    }
                }
            }
            None => None,
        };

        Ok(LspRequestTool {
            method: helper.method,
            params: processed_params,
            build_directory: helper.build_directory,
        })
    }
}

impl LspRequestTool {
    /// Returns the name of the tool as a string.
    pub fn tool_name() -> String {
        "lsp_request".to_string()
    }

    /// Constructs and returns a `rust_mcp_schema::Tool` instance.
    pub fn tool() -> rust_mcp_sdk::schema::Tool {
        use std::collections::HashMap;

        let mut properties = HashMap::new();

        // method field - required string
        let mut method_schema = serde_json::Map::new();
        method_schema.insert(
            "type".to_string(),
            serde_json::Value::String("string".to_string()),
        );
        properties.insert("method".to_string(), method_schema);

        // params field - optional, accepts any JSON value
        let params_schema = serde_json::Map::new();
        // Use an empty schema which accepts any JSON value according to JSON Schema Draft 2020-12
        properties.insert("params".to_string(), params_schema);

        // buildDirectory field - optional string
        let mut build_directory_schema = serde_json::Map::new();
        build_directory_schema.insert(
            "type".to_string(),
            serde_json::Value::String("string".to_string()),
        );
        properties.insert("buildDirectory".to_string(), build_directory_schema);

        rust_mcp_sdk::schema::Tool {
            name: "lsp_request".to_string(),
            description: Some("Direct proxy to clangd LSP server for advanced use cases not covered by specialized tools. Supports all LSP methods and protocols. Use this for custom LSP requests, experimental features, or when you need direct access to clangd's full capabilities. Automatically sets up clangd if not already running. Requires clangd version 20+. Optional build_directory parameter - if not provided, requires single build directory in workspace. All responses include real-time indexing status with progress tracking and completion estimates.".to_string()),
            input_schema: rust_mcp_sdk::schema::ToolInputSchema::new(
                vec!["method".to_string()],
                Some(properties),
            ),
            title: None,
            meta: None,
            annotations: None,
            output_schema: None,
        }
    }

    #[instrument(name = "lsp_request", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Sending LSP request: {}", self.method);

        // Handle automatic clangd setup if needed
        let build_path = match &self.build_directory {
            Some(build_dir) => {
                info!("Using specified build directory: {}", build_dir);
                let path = PathBuf::from(build_dir);

                // Validate the build directory is absolute or make it relative to current dir
                if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir().unwrap_or_default().join(path)
                }
            }
            None => {
                info!("No build directory specified for LSP request, attempting auto-resolution");
                let resolved_path = Self::resolve_build_directory()?;
                match resolved_path {
                    Some(path) => path,
                    None => {
                        let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                        let content = json!({
                            "success": false,
                            "error": "build_directory parameter required",
                            "message": "Multiple or no build directories detected. Use list_build_dirs tool to see available options, or specify any build directory path you believe is correct - the tool will attempt to use it.",
                            "suggested_workflow": [
                                "Call list_build_dirs to see discovered build directories",
                                "Or provide any build_directory path (tool will validate and attempt to use)"
                            ],
                            "flexibility_note": "build_directory parameter accepts any path - not restricted to discovered directories",
                            "indexing_status": {
                                "status": match indexing_state.status {
                                    crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                                    crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                                    crate::lsp::types::IndexingStatus::Completed => "completed",
                                },
                                "is_indexing": indexing_state.is_indexing(),
                                "files_processed": indexing_state.files_processed,
                                "total_files": indexing_state.total_files,
                                "percentage": indexing_state.percentage,
                                "message": indexing_state.message,
                                "estimated_completion_seconds": indexing_state.estimated_completion_seconds
                            }
                        });

                        return Ok(CallToolResult::text_content(vec![TextContent::from(
                            serialize_result(&content),
                        )]));
                    }
                }
            }
        };

        // Ensure clangd is setup for this build directory
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
                        "suggestion": "Verify build directory exists and contains compile_commands.json",
                        "indexing_status": {
                            "status": match indexing_state.status {
                                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                                crate::lsp::types::IndexingStatus::Completed => "completed",
                            },
                            "is_indexing": indexing_state.is_indexing(),
                            "files_processed": indexing_state.files_processed,
                            "total_files": indexing_state.total_files,
                            "percentage": indexing_state.percentage,
                            "message": indexing_state.message,
                            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
                        }
                    });

                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]));
                }
                info!("Clangd setup completed for: {}", build_path.display());
            }
        }

        // Enhanced logging for parameter diagnostics
        match &self.params {
            Some(params) => {
                let param_type = match params {
                    serde_json::Value::Object(_) => "object",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Null => "null",
                };
                debug!(
                    "LSP request params - method: {}, type: {}, value: {}",
                    self.method,
                    param_type,
                    serde_json::to_string(params)
                        .unwrap_or_else(|_| "failed to serialize".to_string())
                );
            }
            None => {
                debug!("LSP request params - method: {}, type: none", self.method);
            }
        }

        let manager_guard = clangd_manager.lock().await;

        // Check if this is a notification (methods that don't expect responses)
        let is_notification = matches!(
            self.method.as_str(),
            "initialized"
                | "textDocument/didOpen"
                | "textDocument/didClose"
                | "textDocument/didChange"
                | "textDocument/didSave"
                | "exit"
        );

        if is_notification {
            match manager_guard
                .send_lsp_notification(self.method.clone(), self.params.clone())
                .await
            {
                Ok(()) => {
                    let build_directory = manager_guard
                        .get_current_build_directory()
                        .await
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "none".to_string());

                    let indexing_state = manager_guard.get_indexing_state().await;
                    let content = json!({
                        "success": true,
                        "method": self.method,
                        "message": "Notification sent successfully",
                        "build_directory_used": build_directory,
                        "indexing_status": {
                            "status": match indexing_state.status {
                                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                                crate::lsp::types::IndexingStatus::Completed => "completed",
                            },
                            "is_indexing": indexing_state.is_indexing(),
                            "files_processed": indexing_state.files_processed,
                            "total_files": indexing_state.total_files,
                            "percentage": indexing_state.percentage,
                            "message": indexing_state.message,
                            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
                        }
                    });

                    info!("LSP notification sent successfully: {}", self.method);

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let indexing_state = manager_guard.get_indexing_state().await;
                    let content = json!({
                        "success": false,
                        "error": error_msg,
                        "method": self.method,
                        "params": self.params,
                        "indexing_status": {
                            "status": match indexing_state.status {
                                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                                crate::lsp::types::IndexingStatus::Completed => "completed",
                            },
                            "is_indexing": indexing_state.is_indexing(),
                            "files_processed": indexing_state.files_processed,
                            "total_files": indexing_state.total_files,
                            "percentage": indexing_state.percentage,
                            "message": indexing_state.message,
                            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
                        }
                    });

                    error!("LSP notification failed: {}", error_msg);

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
            }
        } else {
            match manager_guard
                .send_lsp_request(self.method.clone(), self.params.clone())
                .await
            {
                Ok(result) => {
                    let build_directory = manager_guard
                        .get_current_build_directory()
                        .await
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "none".to_string());

                    let indexing_state = manager_guard.get_indexing_state().await;
                    let content = json!({
                        "success": true,
                        "method": self.method,
                        "result": result,
                        "build_directory_used": build_directory,
                        "indexing_status": {
                            "status": match indexing_state.status {
                                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                                crate::lsp::types::IndexingStatus::Completed => "completed",
                            },
                            "is_indexing": indexing_state.is_indexing(),
                            "files_processed": indexing_state.files_processed,
                            "total_files": indexing_state.total_files,
                            "percentage": indexing_state.percentage,
                            "message": indexing_state.message,
                            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
                        }
                    });

                    info!("LSP request successful: {}", self.method);

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
                Err(LspError::NotSetup) => {
                    let indexing_state = manager_guard.get_indexing_state().await;
                    let content = json!({
                        "success": false,
                        "error": "clangd setup failed",
                        "message": "Unable to automatically setup clangd. This should not happen - clangd setup is now automatic.",
                        "workflow": "1. Use list_build_dirs to verify build directories, 2. Ensure build directory contains compile_commands.json",
                        "method": self.method,
                        "indexing_status": {
                            "status": match indexing_state.status {
                                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                                crate::lsp::types::IndexingStatus::Completed => "completed",
                            },
                            "is_indexing": indexing_state.is_indexing(),
                            "files_processed": indexing_state.files_processed,
                            "total_files": indexing_state.total_files,
                            "percentage": indexing_state.percentage,
                            "message": indexing_state.message,
                            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
                        }
                    });

                    error!("LSP request failed - clangd setup failed");

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let indexing_state = manager_guard.get_indexing_state().await;
                    let content = json!({
                        "success": false,
                        "error": error_msg,
                        "method": self.method,
                        "params": self.params,
                        "indexing_status": {
                            "status": match indexing_state.status {
                                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                                crate::lsp::types::IndexingStatus::Completed => "completed",
                            },
                            "is_indexing": indexing_state.is_indexing(),
                            "files_processed": indexing_state.files_processed,
                            "total_files": indexing_state.total_files,
                            "percentage": indexing_state.percentage,
                            "message": indexing_state.message,
                            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
                        }
                    });

                    error!("LSP request failed: {}", error_msg);

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
            }
        }
    }

    fn resolve_build_directory() -> Result<Option<PathBuf>, CallToolError> {
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
            Err(_) => {
                info!("Not a CMake project or failed to analyze");
                Ok(None)
            }
        }
    }
}

#[mcp_tool(
    name = "search_symbols",
    description = "Search C++ symbols using clangd LSP server with comprehensive filtering and scope control. \
                   QUERY SYNTAX: Supports fuzzy search ('vector'), qualified names ('std::vector'), namespace exploration ('MyNamespace::'), \
                   global scope ('::main'), and partial matches ('get_'). \
                   SYMBOL KINDS: class, struct, enum, function, method, variable, field, namespace, typedef, macro, constructor, destructor, \
                   operator, interface, property, event, constant, array, boolean, key, null, number, object, string, enumMember, typeParameter. \
                   SCOPE CONTROL: By default searches project symbols only (source files + headers in project directory tree). \
                   Use include_external=true for system headers and libraries. \
                   FILE FILTERING: Accepts relative paths from project root or absolute paths. \
                   PROJECT DETECTION: Automatically detects project boundaries using CMake compilation database and common ancestor analysis. \
                   OUTPUT: Returns JSON with symbol name, kind (human-readable), qualified name, file location, line/column, container scope, and detail information. \
                   Perfect for code exploration, navigation, refactoring, and understanding large codebases."
)]
#[derive(Debug, ::serde::Serialize, JsonSchema)]
pub struct SearchSymbolsTool {
    /// Search query using clangd's native syntax. 
    /// EXAMPLES: 'vector' (fuzzy match), 'std::vector' (qualified name), 'MyNamespace::' (explore namespace contents), 
    /// '::main' (global scope), 'get_' (prefix match), 'Math' (class name)
    pub query: String,
    
    /// Optional symbol kinds to filter results. 
    /// SUPPORTED KINDS: "class", "struct", "enum", "function", "method", "variable", "field", "namespace", "typedef", 
    /// "macro", "constructor", "destructor", "operator", "interface", "property", "event", "constant", "array", 
    /// "boolean", "key", "null", "number", "object", "string", "enumMember", "typeParameter". 
    /// Multiple kinds can be specified: ["class", "struct", "enum"]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,
    
    /// Optional file paths to limit search scope to specific files. 
    /// Accepts relative paths from project root (e.g., "src/math.cpp") or absolute paths. 
    /// When specified, uses document symbol search instead of workspace search for more detailed results within those files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    
    /// Maximum number of results to return. DEFAULT: 100. 
    /// Useful for limiting output when searching broad terms or exploring large codebases. 
    /// Results are returned in relevance order from clangd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,
    
    /// Include external symbols from system headers and libraries (e.g., std::vector, stdio functions). 
    /// DEFAULT: false (project-only scope). When true, includes symbols from outside the detected project boundaries. 
    /// Project boundaries are determined by CMake compilation database and directory tree analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_external: Option<bool>,
}

impl<'de> serde::Deserialize<'de> for SearchSymbolsTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct SearchSymbolsToolHelper {
            query: String,
            #[serde(default)]
            kinds: Option<Vec<String>>,
            #[serde(default)]
            files: Option<Vec<String>>,
            #[serde(default)]
            max_results: Option<u32>,
            #[serde(default)]
            include_external: Option<bool>,
        }

        let helper = SearchSymbolsToolHelper::deserialize(deserializer)?;
        
        Ok(SearchSymbolsTool {
            query: helper.query,
            kinds: helper.kinds,
            files: helper.files,
            max_results: helper.max_results,
            include_external: helper.include_external,
        })
    }
}

impl SearchSymbolsTool {
    #[instrument(name = "search_symbols", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Searching symbols: query='{}', kinds={:?}, files={:?}, max_results={:?}, include_external={:?}", 
              self.query, self.kinds, self.files, self.max_results, self.include_external);

        // Handle automatic clangd setup if needed
        let build_path = match Self::resolve_build_directory() {
            Ok(Some(path)) => path,
            Ok(None) => {
                let indexing_state = clangd_manager.lock().await.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": "build_directory_required",
                    "message": "No build directory found. Use list_build_dirs tool to see available options, or configure a build directory first.",
                    "query": self.query,
                    "indexing_status": Self::format_indexing_status(&indexing_state)
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
                    "query": self.query,
                    "indexing_status": Self::format_indexing_status(&indexing_state)
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
                        "query": self.query,
                        "indexing_status": Self::format_indexing_status(&indexing_state)
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

        // Determine search strategy based on files parameter
        if let Some(files) = &self.files {
            // File-specific search using document symbols
            self.search_in_files(files, &manager_guard, &build_directory)
                .await
        } else {
            // Workspace-wide search using workspace symbols
            self.search_workspace(&manager_guard, &build_directory)
                .await
        }
    }

    async fn search_in_files(
        &self,
        files: &[String],
        manager: &ClangdManager,
        build_directory: &str,
    ) -> Result<CallToolResult, CallToolError> {
        let mut all_symbols = Vec::new();
        let mut processed_files = Vec::new();

        for file_path in files {
            // Convert to URI format if needed
            let file_uri = if file_path.starts_with("file://") {
                file_path.clone()
            } else {
                format!("file://{}", file_path)
            };

            // Convert URI to file path for opening
            let path = if let Some(stripped) = file_uri.strip_prefix("file://") {
                PathBuf::from(stripped)
            } else {
                PathBuf::from(file_path)
            };

            // Open file if needed
            match manager.open_file_if_needed(&path).await {
                Ok(file_opened) => {
                    processed_files.push(json!({
                        "file": file_path,
                        "file_opened": file_opened,
                        "status": "success"
                    }));

                    // Get document symbols
                    let params = json!({
                        "textDocument": {
                            "uri": file_uri
                        }
                    });

                    match manager
                        .send_lsp_request("textDocument/documentSymbol".to_string(), Some(params))
                        .await
                    {
                        Ok(symbols) => {
                            if let Some(symbol_array) = symbols.as_array() {
                                for symbol in symbol_array {
                                    if self.matches_query_and_filters(symbol) {
                                        all_symbols.push(symbol.clone());
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            processed_files.last_mut().unwrap()["error"] = json!(format!("LSP request failed: {}", e));
                        }
                    }
                }
                Err(e) => {
                    processed_files.push(json!({
                        "file": file_path,
                        "file_opened": false,
                        "status": "error",
                        "error": format!("Failed to open file: {}", e)
                    }));
                }
            }
        }

        // Apply external filtering and limit results
        let filtered_symbols = self.filter_symbols(all_symbols, manager).await;
        let limited_symbols = self.limit_results(filtered_symbols);
        let converted_symbols = self.convert_symbol_kinds(limited_symbols);

        let indexing_state = manager.get_indexing_state().await;
        let opened_files_count = manager.get_opened_files_count().await;

        let content = json!({
            "success": true,
            "query": self.query,
            "total_matches": converted_symbols.len(),
            "symbols": converted_symbols,
            "metadata": {
                "search_type": "file_specific",
                "files_processed": processed_files,
                "opened_files_count": opened_files_count,
                "build_directory_used": build_directory,
                "indexing_status": Self::format_indexing_status(&indexing_state)
            }
        });

        Ok(CallToolResult::text_content(vec![TextContent::from(
            serialize_result(&content),
        )]))
    }

    async fn search_workspace(
        &self,
        manager: &ClangdManager,
        build_directory: &str,
    ) -> Result<CallToolResult, CallToolError> {
        // Check current indexing state before waiting
        let initial_indexing_state = manager.get_indexing_state().await;
        info!("Initial indexing state: {:?}", initial_indexing_state.status);
        
        // Wait for indexing to complete if not already completed
        if initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed {
            info!("Waiting for indexing completion before workspace symbol search (current status: {:?})", initial_indexing_state.status);
            if let Err(e) = manager
                .wait_for_indexing_completion(std::time::Duration::from_secs(30))
                .await
            {
                let final_indexing_state = manager.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("Indexing timeout: {}", e),
                    "message": "Symbol search may be incomplete due to ongoing indexing",
                    "query": self.query,
                    "initial_status": match initial_indexing_state.status {
                        crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                        crate::lsp::types::IndexingStatus::InProgress => "in_progress", 
                        crate::lsp::types::IndexingStatus::Completed => "completed",
                    },
                    "indexing_status": Self::format_indexing_status(&final_indexing_state)
                });

                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]));
            }
        }

        // Send workspace symbol request with user's query
        let params = json!({
            "query": self.query
        });

        match manager
            .send_lsp_request("workspace/symbol".to_string(), Some(params))
            .await
        {
            Ok(symbols) => {
                let symbol_array = symbols.as_array().unwrap_or(&vec![]).clone();
                
                // Apply external filtering and other filters
                let filtered_symbols = self.filter_symbols(symbol_array, manager).await;
                let limited_symbols = self.limit_results(filtered_symbols);
                let converted_symbols = self.convert_symbol_kinds(limited_symbols);

                let final_indexing_state = manager.get_indexing_state().await;
                let opened_files_count = manager.get_opened_files_count().await;

                let content = json!({
                    "success": true,
                    "query": self.query,
                    "total_matches": converted_symbols.len(),
                    "symbols": converted_symbols,
                    "metadata": {
                        "search_type": "workspace",
                        "opened_files_count": opened_files_count,
                        "build_directory_used": build_directory,
                        "indexing_waited": initial_indexing_state.status != crate::lsp::types::IndexingStatus::Completed,
                        "indexing_status": Self::format_indexing_status(&final_indexing_state)
                    }
                });

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(e) => {
                let indexing_state = manager.get_indexing_state().await;
                let content = json!({
                    "success": false,
                    "error": format!("LSP request failed: {}", e),
                    "query": self.query,
                    "indexing_status": Self::format_indexing_status(&indexing_state)
                });

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
        }
    }

    fn matches_query_and_filters(&self, symbol: &serde_json::Value) -> bool {
        // For file-specific search, we need to do our own query matching
        // since clangd's documentSymbol doesn't take a query parameter
        if let Some(name) = symbol.get("name").and_then(|n| n.as_str()) {
            // Simple fuzzy matching - check if query is contained in symbol name (case insensitive)
            let query_lower = self.query.to_lowercase();
            let name_lower = name.to_lowercase();
            
            if !name_lower.contains(&query_lower) {
                return false;
            }
        } else {
            return false;
        }

        // Apply kind filtering
        if let Some(kinds) = &self.kinds {
            if let Some(kind) = symbol.get("kind").and_then(|k| k.as_u64()) {
                let kind_name = Self::symbol_kind_to_string(kind);
                if !kinds.iter().any(|k| k.to_lowercase() == kind_name.to_lowercase()) {
                    return false;
                }
            }
        }

        true
    }

    async fn filter_symbols(
        &self,
        symbols: Vec<serde_json::Value>,
        manager: &ClangdManager,
    ) -> Vec<serde_json::Value> {
        let include_external = self.include_external.unwrap_or(false);
        
        debug!("🔍 Filtering {} symbols, include_external={}", symbols.len(), include_external);
        
        if include_external {
            // Include all symbols when external is enabled
            debug!("✅ Including all symbols (external enabled)");
            self.apply_kind_filter(symbols)
        } else {
            // Filter out external symbols (system headers, libraries, etc.)
            let compilation_database = manager.get_compilation_database().await;
            debug!("📁 Using compilation database for project filtering");
            
            let filtered: Vec<_> = symbols
                .into_iter()
                .filter(|symbol| {
                    let is_project = self.is_project_symbol(symbol, &compilation_database);
                    let symbol_name = symbol.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                    if is_project {
                        debug!("✅ Keeping project symbol: {}", symbol_name);
                    } else {
                        debug!("❌ Filtering out external symbol: {}", symbol_name);
                    }
                    is_project
                })
                .collect();
            
            debug!("📊 After project filtering: {} symbols remaining", filtered.len());
            self.apply_kind_filter(filtered)
        }
    }

    fn apply_kind_filter(&self, symbols: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        if let Some(kinds) = &self.kinds {
            symbols
                .into_iter()
                .filter(|symbol| {
                    if let Some(kind) = symbol.get("kind").and_then(|k| k.as_u64()) {
                        let kind_name = Self::symbol_kind_to_string(kind);
                        kinds.iter().any(|k| k.to_lowercase() == kind_name.to_lowercase())
                    } else {
                        false
                    }
                })
                .collect()
        } else {
            symbols
        }
    }

    fn is_project_symbol(
        &self,
        symbol: &serde_json::Value,
        compilation_database: &Option<std::collections::HashSet<PathBuf>>,
    ) -> bool {
        // Extract file path from symbol location
        let file_path = if let Some(location) = symbol.get("location") {
            if let Some(uri) = location.get("uri").and_then(|u| u.as_str()) {
                if let Some(path_str) = uri.strip_prefix("file://") {
                    Some(PathBuf::from(path_str))
                } else {
                    Some(PathBuf::from(uri))
                }
            } else {
                None
            }
        } else {
            None
        };

        let symbol_name = symbol.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
        
        if let Some(path) = file_path {
            debug!("🔍 Checking symbol '{}' at path: {}", symbol_name, path.display());
            
            // First check if it's directly in the compilation database (source files)
            if let Some(db) = compilation_database {
                debug!("📁 Compilation database contains {} files", db.len());
                if db.contains(&path) {
                    debug!("✅ Symbol '{}' is in compilation database (source file): {}", symbol_name, path.display());
                    return true;
                }
                debug!("❌ Symbol '{}' not in compilation database: {}", symbol_name, path.display());
            }

            // If not in compilation database, check if it's a project header
            // by seeing if it's under the project source directory
            if let Some(db) = compilation_database {
                if !db.is_empty() {
                    // Get the project root by finding common ancestor of source files
                    if let Some(project_root) = self.find_project_root(db) {
                        debug!("📂 Project root detected as: {}", project_root.display());
                        let starts_with_root = path.starts_with(&project_root);
                        debug!("🔎 Symbol '{}': starts_with_root={}", symbol_name, starts_with_root);
                        
                        // Include any file under the project root
                        if starts_with_root {
                            debug!("✅ Symbol '{}' is project header: {}", symbol_name, path.display());
                            return true;
                        } else {
                            debug!("❌ Symbol '{}' excluded: not under project root", symbol_name);
                        }
                    } else {
                        debug!("⚠️ Could not determine project root for symbol '{}'", symbol_name);
                    }
                }
            }

            debug!("❌ Symbol '{}' marked as external: {}", symbol_name, path.display());
            false
        } else {
            // If we can't determine the file, exclude it for safety
            debug!("⚠️ Symbol '{}' has no file location, excluding for safety", symbol_name);
            false
        }
    }

    fn find_project_root(&self, _compilation_database: &std::collections::HashSet<PathBuf>) -> Option<PathBuf> {
        // Get project root from CMake build directory instead of inferring from source files
        debug!("🏗️ Reading project root from CMake build configuration");
        
        match CmakeProjectStatus::analyze_current_directory() {
            Ok(status) => {
                if !status.build_directories.is_empty() {
                    // Use the first available build directory to determine project root
                    let build_dir = &status.build_directories[0];
                    debug!("� Using build directory: {}", build_dir.path.display());
                    
                    // The project root is typically the parent of the build directory
                    // or can be read from CMakeCache.txt
                    if let Some(project_root) = self.read_cmake_source_dir(&build_dir.path) {
                        debug!("� CMake source directory: {}", project_root.display());
                        return Some(project_root);
                    }
                    
                    // Fallback: assume build directory is a subdirectory of project root
                    if let Some(parent) = build_dir.path.parent() {
                        debug!("🏠 Fallback project root (build parent): {}", parent.display());
                        return Some(parent.to_path_buf());
                    }
                }
                debug!("❌ No build directories found");
                None
            }
            Err(e) => {
                debug!("❌ Failed to analyze CMake project: {}", e);
                None
            }
        }
    }

    fn read_cmake_source_dir(&self, build_path: &Path) -> Option<PathBuf> {
        let cache_file = build_path.join("CMakeCache.txt");
        debug!("📄 Looking for CMakeCache.txt at: {}", cache_file.display());
        
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            // Look for CMAKE_SOURCE_DIR entry in CMakeCache.txt
            for line in content.lines() {
                if let Some(source_dir) = line.strip_prefix("CMAKE_SOURCE_DIR:INTERNAL=") {
                    let source_path = PathBuf::from(source_dir);
                    debug!("✅ Found CMAKE_SOURCE_DIR: {}", source_path.display());
                    return Some(source_path);
                }
            }
            debug!("❌ CMAKE_SOURCE_DIR not found in CMakeCache.txt");
        } else {
            debug!("❌ Could not read CMakeCache.txt");
        }
        
        None
    }

    fn convert_symbol_kinds(&self, symbols: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        symbols
            .into_iter()
            .map(|mut symbol| {
                if let Some(kind_num) = symbol.get("kind").and_then(|k| k.as_u64()) {
                    let kind_name = Self::symbol_kind_to_string(kind_num);
                    symbol["kind"] = serde_json::Value::String(kind_name.to_string());
                }
                symbol
            })
            .collect()
    }

    fn limit_results(&self, symbols: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        let max_results = self.max_results.unwrap_or(100) as usize;
        symbols.into_iter().take(max_results).collect()
    }

    fn symbol_kind_to_string(kind: u64) -> &'static str {
        // LSP SymbolKind enumeration
        match kind {
            1 => "file",
            2 => "module",
            3 => "namespace",
            4 => "package",
            5 => "class",
            6 => "method",
            7 => "property",
            8 => "field",
            9 => "constructor",
            10 => "enum",
            11 => "interface",
            12 => "function",
            13 => "variable",
            14 => "constant",
            15 => "string",
            16 => "number",
            17 => "boolean",
            18 => "array",
            19 => "object",
            20 => "key",
            21 => "null",
            22 => "enum_member",
            23 => "struct",
            24 => "event",
            25 => "operator",
            26 => "type_parameter",
            _ => "unknown",
        }
    }
    fn format_indexing_status(
        indexing_state: &crate::lsp::types::IndexingState,
    ) -> serde_json::Value {
        json!({
            "status": match indexing_state.status {
                crate::lsp::types::IndexingStatus::NotStarted => "not_started",
                crate::lsp::types::IndexingStatus::InProgress => "in_progress",
                crate::lsp::types::IndexingStatus::Completed => "completed",
            },
            "is_indexing": indexing_state.is_indexing(),
            "files_processed": indexing_state.files_processed,
            "total_files": indexing_state.total_files,
            "percentage": indexing_state.percentage,
            "message": indexing_state.message,
            "estimated_completion_seconds": indexing_state.estimated_completion_seconds
        })
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

// Tool definitions using mcp_tool! macro for automatic schema generation
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "name")]
pub enum CppTools {
    #[serde(rename = "list_build_dirs")]
    ListBuildDirs(ListBuildDirsTool),
    #[serde(rename = "lsp_request")]
    LspRequest(LspRequestTool),
    #[serde(rename = "search_symbols")]
    SearchSymbols(SearchSymbolsTool),
}

impl CppTools {
    pub fn tools() -> Vec<rust_mcp_sdk::schema::Tool> {
        vec![
            ListBuildDirsTool::tool(),
            LspRequestTool::tool(),
            SearchSymbolsTool::tool(),
        ]
    }

    pub async fn handle_call(
        tool_name: &str,
        arguments: serde_json::Value,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Handling tool call: {}", tool_name);

        match tool_name {
            name if name == ListBuildDirsTool::tool_name() => {
                let tool: ListBuildDirsTool = serde_json::from_value(arguments).map_err(|e| {
                    CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to deserialize {} arguments: {}", ListBuildDirsTool::tool_name(), e)
                    ))
                })?;
                tool.call_tool()
            }
            name if name == LspRequestTool::tool_name() => {
                let tool: LspRequestTool = serde_json::from_value(arguments).map_err(|e| {
                    CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to deserialize {} arguments: {}", LspRequestTool::tool_name(), e)
                    ))
                })?;
                tool.call_tool(clangd_manager).await
            }
            name if name == SearchSymbolsTool::tool_name() => {
                let tool: SearchSymbolsTool = serde_json::from_value(arguments).map_err(|e| {
                    CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to deserialize {} arguments: {}", SearchSymbolsTool::tool_name(), e)
                    ))
                })?;
                tool.call_tool(clangd_manager).await
            }
            _ => Err(CallToolError::unknown_tool(format!(
                "Unknown tool: {}",
                tool_name
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_search_symbols_tool_deserialize() {
        let json_data = json!({
            "query": "vector",
            "kinds": ["class", "function"],
            "max_results": 50
        });
        let tool: SearchSymbolsTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.query, "vector");
        assert_eq!(tool.kinds, Some(vec!["class".to_string(), "function".to_string()]));
        assert_eq!(tool.max_results, Some(50));
        assert_eq!(tool.include_external, None);
        assert_eq!(tool.files, None);
    }

    #[test]
    fn test_search_symbols_tool_minimal() {
        let json_data = json!({
            "query": "main"
        });
        let tool: SearchSymbolsTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.query, "main");
        assert_eq!(tool.kinds, None);
        assert_eq!(tool.max_results, None);
        assert_eq!(tool.include_external, None);
        assert_eq!(tool.files, None);
    }

    #[test]
    fn test_symbol_kind_to_string() {
        assert_eq!(SearchSymbolsTool::symbol_kind_to_string(5), "class");
        assert_eq!(SearchSymbolsTool::symbol_kind_to_string(12), "function");
        assert_eq!(SearchSymbolsTool::symbol_kind_to_string(13), "variable");
        assert_eq!(SearchSymbolsTool::symbol_kind_to_string(999), "unknown");
    }
}
