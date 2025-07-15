//! LSP request proxy tools

use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument};

use crate::cmake::CmakeProjectStatus;
use crate::lsp::{ClangdManager, LspError};
use super::serialize_result;

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
                        match serde_json::from_str::<serde_json::Value>(&s) {
                            Ok(parsed) => Some(parsed),
                            Err(_) => {
                                // If parsing fails, treat as a string literal
                                Some(serde_json::Value::String(s))
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
                            "error": "build_directory_required",
                            "message": "No build directory found or multiple directories available. Use list_build_dirs tool to see options, then specify buildDirectory parameter.",
                            "method": self.method,
                            "indexing_status": Self::format_indexing_status(&indexing_state)
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
                        "indexing_status": Self::format_indexing_status(&indexing_state)
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
                        "indexing_status": Self::format_indexing_status(&indexing_state)
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
                        "indexing_status": Self::format_indexing_status(&indexing_state)
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
                        "indexing_status": Self::format_indexing_status(&indexing_state)
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
                        "indexing_status": Self::format_indexing_status(&indexing_state)
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
                        "indexing_status": Self::format_indexing_status(&indexing_state)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_lsp_request_tool_deserialize() {
        let json_data = json!({
            "method": "textDocument/hover",
            "params": {
                "textDocument": {"uri": "file:///test.cpp"},
                "position": {"line": 10, "character": 5}
            }
        });
        let tool: LspRequestTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.method, "textDocument/hover");
        assert!(tool.params.is_some());
        assert_eq!(tool.build_directory, None);
    }

    #[test]
    fn test_lsp_request_tool_minimal() {
        let json_data = json!({
            "method": "workspace/symbol"
        });
        let tool: LspRequestTool = serde_json::from_value(json_data).unwrap();
        assert_eq!(tool.method, "workspace/symbol");
        assert_eq!(tool.params, None);
        assert_eq!(tool.build_directory, None);
    }
}
