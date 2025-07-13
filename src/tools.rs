use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, instrument};

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
/// - `CppProjectStatusTool` - **sync** (file system analysis, no process interaction)
/// - `SetupClangdTool` - **async** (starts LSP process, initializes manager)
/// - `LspRequestTool` - **async** (sends requests to LSP process)
///
/// **Error Handling Pattern:**
/// - All tools use `CallToolResult::text_content()` for responses
/// - All tools use `serialize_result()` helper for consistent JSON formatting
/// - Errors are logged with appropriate level (error, warn, info) before returning

#[mcp_tool(
    name = "cpp_project_status",
    description = "Analyze C++ project status, including CMake configuration and build directories"
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct CppProjectStatusTool {
    // No parameters needed for analyzing current directory
}

impl CppProjectStatusTool {
    #[instrument(name = "cpp_project_status", skip(self))]
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        info!("Executing C++ project status analysis");

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

                info!("Successfully analyzed C++ project");

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

#[mcp_tool(
    name = "setup_clangd",
    description = "Setup clangd LSP server for a build directory with compile_commands.json (required before using lsp_request). Use cpp_project_status tool first to discover build directories."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct SetupClangdTool {
    #[serde(rename = "buildDirectory")]
    pub build_directory: String,
}

impl SetupClangdTool {
    #[instrument(name = "setup_clangd", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!(
            "Setting up clangd for build directory: {}",
            self.build_directory
        );

        let build_path = PathBuf::from(&self.build_directory);

        // Validate the build directory is absolute or make it relative to current dir
        let build_path = if build_path.is_absolute() {
            build_path
        } else {
            std::env::current_dir().unwrap_or_default().join(build_path)
        };

        let manager_guard = clangd_manager.lock().await;

        match manager_guard.setup_clangd(build_path.clone()).await {
            Ok(message) => {
                let content = json!({
                    "success": true,
                    "message": message,
                    "build_directory": build_path.display().to_string(),
                    "compile_commands": build_path.join("compile_commands.json").display().to_string(),
                    "next_step": "Use lsp_request tool to send LSP requests to clangd"
                });

                info!("Clangd setup successful for: {}", build_path.display());

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(e) => {
                let error_msg = e.to_string();
                let content = json!({
                    "success": false,
                    "error": error_msg,
                    "build_directory": build_path.display().to_string(),
                    "workflow_reminder": "1. Optional: cpp_project_status to find build dirs, 2. Required: setup_clangd, 3. Use: lsp_request"
                });

                error!("Clangd setup failed: {}", error_msg);

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
        }
    }
}

#[mcp_tool(
    name = "lsp_request",
    description = "Send LSP request to clangd (requires setup_clangd first). Supports all LSP methods like textDocument/definition, textDocument/hover, textDocument/completion, etc. See lsp://workflow resource for complete usage guide."
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct LspRequestTool {
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

impl LspRequestTool {
    #[instrument(name = "lsp_request", skip(self, clangd_manager))]
    pub async fn call_tool(
        &self,
        clangd_manager: &Arc<Mutex<ClangdManager>>,
    ) -> Result<CallToolResult, CallToolError> {
        info!("Sending LSP request: {}", self.method);

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
                    let content = json!({
                        "success": true,
                        "method": self.method,
                        "message": "Notification sent successfully"
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
                    let content = json!({
                        "success": true,
                        "method": self.method,
                        "result": result
                    });

                    info!("LSP request successful: {}", self.method);

                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        serialize_result(&content),
                    )]))
                }
                Err(LspError::NotSetup) => {
                    let content = json!({
                        "success": false,
                        "error": "clangd not setup",
                        "workflow": "1. Optional: cpp_project_status, 2. Required: setup_clangd, 3. Use: lsp_request",
                        "resource": "See lsp://workflow for complete guide",
                        "method": self.method
                    });

                    error!("LSP request failed - clangd not setup");

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
}

// Tool definitions using mcp_tool! macro for automatic schema generation
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "name")]
pub enum CppTools {
    #[serde(rename = "cpp_project_status")]
    CppProjectStatus(CppProjectStatusTool),
    #[serde(rename = "setup_clangd")]
    SetupClangd(SetupClangdTool),
    #[serde(rename = "lsp_request")]
    LspRequest(LspRequestTool),
}

impl CppTools {
    pub fn tools() -> Vec<rust_mcp_sdk::schema::Tool> {
        vec![
            CppProjectStatusTool::tool(),
            SetupClangdTool::tool(),
            LspRequestTool::tool(),
        ]
    }
}

impl TryFrom<rust_mcp_sdk::schema::CallToolRequest> for CppTools {
    type Error = String;

    fn try_from(request: rust_mcp_sdk::schema::CallToolRequest) -> Result<Self, Self::Error> {
        match request.params.name.as_str() {
            name if name == CppProjectStatusTool::tool_name() => {
                let args_value = match request.params.arguments {
                    Some(args) => serde_json::Value::Object(args),
                    None => serde_json::json!({}),
                };
                let tool: CppProjectStatusTool =
                    serde_json::from_value(args_value).map_err(|e| {
                        format!(
                            "Failed to parse {} params: {}",
                            CppProjectStatusTool::tool_name(),
                            e
                        )
                    })?;
                Ok(CppTools::CppProjectStatus(tool))
            }
            name if name == SetupClangdTool::tool_name() => {
                let args_value = match request.params.arguments {
                    Some(args) => serde_json::Value::Object(args),
                    None => serde_json::json!({}),
                };
                let tool: SetupClangdTool = serde_json::from_value(args_value).map_err(|e| {
                    format!(
                        "Failed to parse {} params: {}",
                        SetupClangdTool::tool_name(),
                        e
                    )
                })?;
                Ok(CppTools::SetupClangd(tool))
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
