use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::path::PathBuf;
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
            description: Some("Send LSP request to clangd. Automatically sets up clangd if not already running. Optional build_directory parameter - if not provided, requires single build directory in workspace. Supports all LSP methods like textDocument/definition, textDocument/hover, textDocument/completion, etc.".to_string()),
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
                match Self::resolve_build_directory()? {
                    Some(path) => path,
                    None => {
                        let content = json!({
                            "success": false,
                            "error": "build_directory parameter required",
                            "message": "Multiple or no build directories detected. Use list_build_dirs tool to see available options, or specify any build directory path you believe is correct - the tool will attempt to use it.",
                            "suggested_workflow": [
                                "Call list_build_dirs to see discovered build directories",
                                "Or provide any build_directory path (tool will validate and attempt to use)"
                            ],
                            "flexibility_note": "build_directory parameter accepts any path - not restricted to discovered directories"
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
                    let content = json!({
                        "success": false,
                        "error": format!("Failed to setup clangd: {}", e),
                        "build_directory_attempted": build_path.display().to_string(),
                        "suggestion": "Verify build directory exists and contains compile_commands.json"
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

                    let content = json!({
                        "success": true,
                        "method": self.method,
                        "message": "Notification sent successfully",
                        "build_directory_used": build_directory
                    });

                    info!("LSP notification sent successfully: {}", self.method);

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let content = json!({
                        "success": false,
                        "error": error_msg,
                        "method": self.method,
                        "params": self.params
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

                    let content = json!({
                        "success": true,
                        "method": self.method,
                        "result": result,
                        "build_directory_used": build_directory
                    });

                    info!("LSP request successful: {}", self.method);

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
                Err(LspError::NotSetup) => {
                    let content = json!({
                        "success": false,
                        "error": "clangd setup failed",
                        "message": "Unable to automatically setup clangd. This should not happen - clangd setup is now automatic.",
                        "workflow": "1. Use list_build_dirs to verify build directories, 2. Ensure build directory contains compile_commands.json",
                        "method": self.method
                    });

                    error!("LSP request failed - clangd setup failed");

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let content = json!({
                        "success": false,
                        "error": error_msg,
                        "method": self.method,
                        "params": self.params
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

// Tool definitions using mcp_tool! macro for automatic schema generation
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "name")]
pub enum CppTools {
    #[serde(rename = "list_build_dirs")]
    ListBuildDirs(ListBuildDirsTool),
    #[serde(rename = "lsp_request")]
    LspRequest(LspRequestTool),
}

impl CppTools {
    pub fn tools() -> Vec<rust_mcp_sdk::schema::Tool> {
        vec![ListBuildDirsTool::tool(), LspRequestTool::tool()]
    }
}

impl TryFrom<rust_mcp_sdk::schema::CallToolRequest> for CppTools {
    type Error = String;

    fn try_from(request: rust_mcp_sdk::schema::CallToolRequest) -> Result<Self, Self::Error> {
        match request.params.name.as_str() {
            name if name == ListBuildDirsTool::tool_name() => {
                let args_value = match request.params.arguments {
                    Some(args) => serde_json::Value::Object(args),
                    None => serde_json::json!({}),
                };
                let tool: ListBuildDirsTool = serde_json::from_value(args_value).map_err(|e| {
                    format!(
                        "Failed to parse {} params: {}",
                        ListBuildDirsTool::tool_name(),
                        e
                    )
                })?;
                Ok(CppTools::ListBuildDirs(tool))
            }
            name if name == LspRequestTool::tool_name() => {
                let args_value = match request.params.arguments {
                    Some(args) => serde_json::Value::Object(args),
                    None => serde_json::json!({}),
                };
                let tool: LspRequestTool = serde_json::from_value(args_value).map_err(|e| {
                    format!(
                        "Failed to parse {} params: {}",
                        LspRequestTool::tool_name(),
                        e
                    )
                })?;
                Ok(CppTools::LspRequest(tool))
            }
            _ => Err(format!("Unknown tool: {}", request.params.name)),
        }
    }
}
