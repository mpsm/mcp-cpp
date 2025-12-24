//! C++ MCP Server Handler
//!
//! This module implements the ServerHandler trait for rmcp 0.12 manually,
//! providing tool routing without relying on the `#[tool_router]` macro.
//! The manual implementation provides better control over routing logic
//! and avoids macro expansion issues with complex handler architectures.

use rmcp::{
    ErrorData,
    handler::server::ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, ListToolsResult, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    service::RoleServer,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::Level;

use super::server_helpers::resolve_build_directory;
use super::tools::analyze_symbols::AnalyzeSymbolContextTool;
use super::tools::project_tools::GetProjectDetailsTool;
use super::tools::search_symbols::SearchSymbolsTool;
use crate::log_mcp_message;
use crate::log_timing;
use crate::project::{ProjectError, ProjectWorkspace, WorkspaceSession};
use std::time::Instant;

type JsonObject = serde_json::Map<String, serde_json::Value>;

/// C++ MCP Server Handler
///
/// This handler implements the ServerHandler trait manually for rmcp 0.12.
/// It routes tool calls to the appropriate tool implementations while
/// maintaining separation between protocol handling and business logic.
#[derive(Clone)]
pub struct CppServerHandler {
    workspace_session: WorkspaceSession,
}

impl CppServerHandler {
    /// Create a new CppServerHandler with the given workspace and clangd path.
    pub fn new(
        project_workspace: ProjectWorkspace,
        clangd_path: String,
    ) -> Result<Self, ProjectError> {
        let workspace_session = WorkspaceSession::new(project_workspace, clangd_path)?;
        Ok(Self { workspace_session })
    }

    /// Resolves build directory from optional parameter using the helper function.
    async fn resolve_build_directory(
        &self,
        requested_build_dir: Option<&str>,
    ) -> Result<std::path::PathBuf, ErrorData> {
        let workspace = self.workspace_session.get_workspace().lock().await;
        resolve_build_directory(&workspace, requested_build_dir)
    }

    /// Handle get_project_details tool call
    async fn handle_get_project_details(
        &self,
        arguments: String,
    ) -> Result<CallToolResult, ErrorData> {
        let start = Instant::now();
        log_mcp_message!(Level::INFO, "incoming", "get_project_details", &arguments);

        let params: GetProjectDetailsTool = serde_json::from_str(&arguments).map_err(|e| {
            ErrorData::invalid_params(format!("Failed to parse arguments: {}", e), None)
        })?;

        let workspace = self.workspace_session.get_workspace().lock().await;
        let result = params.call_tool(&workspace)?;

        log_mcp_message!(Level::INFO, "outgoing", "get_project_details", &result);
        log_timing!(Level::DEBUG, "get_project_details", start.elapsed());

        Ok(result)
    }

    /// Handle search_symbols tool call
    async fn handle_search_symbols(&self, arguments: String) -> Result<CallToolResult, ErrorData> {
        let start = Instant::now();
        log_mcp_message!(Level::INFO, "incoming", "search_symbols", &arguments);

        let params: SearchSymbolsTool = serde_json::from_str(&arguments).map_err(|e| {
            ErrorData::invalid_params(format!("Failed to parse arguments: {}", e), None)
        })?;

        let build_dir = self
            .resolve_build_directory(params.build_directory.as_deref())
            .await?;

        let component_session = self
            .workspace_session
            .get_component_session(build_dir)
            .await
            .map_err(|e| {
                ErrorData::invalid_params(format!("ComponentSession creation failed: {}", e), None)
            })?;

        let workspace = self.workspace_session.get_workspace().lock().await;
        let result = params.call_tool(component_session, &workspace).await?;

        log_mcp_message!(Level::INFO, "outgoing", "search_symbols", &result);
        log_timing!(Level::DEBUG, "search_symbols", start.elapsed());

        Ok(result)
    }

    /// Handle analyze_symbol_context tool call
    async fn handle_analyze_symbol_context(
        &self,
        arguments: String,
    ) -> Result<CallToolResult, ErrorData> {
        let start = Instant::now();
        log_mcp_message!(
            Level::INFO,
            "incoming",
            "analyze_symbol_context",
            &arguments
        );

        let params: AnalyzeSymbolContextTool = serde_json::from_str(&arguments).map_err(|e| {
            ErrorData::invalid_params(format!("Failed to parse arguments: {}", e), None)
        })?;

        let build_dir = self
            .resolve_build_directory(params.build_directory.as_deref())
            .await?;

        let component_session = self
            .workspace_session
            .get_component_session(build_dir)
            .await
            .map_err(|e| {
                ErrorData::invalid_params(format!("ComponentSession creation failed: {}", e), None)
            })?;

        let workspace = self.workspace_session.get_workspace().lock().await;
        let result = params.call_tool(component_session, &workspace).await?;

        log_mcp_message!(Level::INFO, "outgoing", "analyze_symbol_context", &result);
        log_timing!(Level::DEBUG, "analyze_symbol_context", start.elapsed());

        Ok(result)
    }
}

impl ServerHandler for CppServerHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "C++ MCP Server - Provides semantic code analysis for C++ codebases using clangd LSP integration. Use get_project_details to discover build configurations first.".into()
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }

    #[allow(refining_impl_trait)]
    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Pin<Box<dyn Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_>> {
        // Helper function to convert json! output to Arc<JsonObject>
        fn to_json_object(value: serde_json::Value) -> Arc<JsonObject> {
            match value {
                serde_json::Value::Object(map) => Arc::new(map),
                _ => panic!("input_schema must be a JSON object"),
            }
        }

        let tools = vec![
            Tool::new(
                "get_project_details",
                "Get comprehensive project analysis including build configurations, components, and global compilation database information.",
                to_json_object(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Optional project root path to scan. DEFAULT: uses server's cached scan results."
                        },
                        "depth": {
                            "type": "integer",
                            "description": "Scan depth for project component discovery. DEFAULT: uses server's initial scan depth."
                        },
                        "include_details": {
                            "type": "boolean",
                            "description": "Include detailed build options and configuration variables. DEFAULT: false."
                        }
                    }
                })),
            ),
            Tool::new(
                "search_symbols",
                "Advanced C++ symbol search engine with intelligent dual-mode operation.",
                to_json_object(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query to match C++ symbol names."
                        },
                        "kinds": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Optional symbol kinds to filter results."
                        },
                        "files": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Optional file paths to limit search scope."
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of results."
                        },
                        "include_external": {
                            "type": "boolean",
                            "description": "Include external symbols from system libraries."
                        },
                        "build_directory": {
                            "type": "string",
                            "description": "Build directory path containing compile_commands.json."
                        },
                        "wait_timeout": {
                            "type": "integer",
                            "description": "Timeout in seconds to wait for indexing completion."
                        }
                    }
                })),
            ),
            Tool::new(
                "analyze_symbol_context",
                "Comprehensive C++ symbol analysis with automatic multi-dimensional context extraction.",
                to_json_object(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "symbol": {
                            "type": "string",
                            "description": "The C++ symbol name to analyze."
                        },
                        "build_directory": {
                            "type": "string",
                            "description": "Build directory path containing compile_commands.json."
                        },
                        "location_hint": {
                            "type": "string",
                            "description": "Location hint for overloaded symbols (format: /path/file.cpp:line:column)."
                        },
                        "max_examples": {
                            "type": "integer",
                            "description": "Maximum number of usage examples to include."
                        },
                        "wait_timeout": {
                            "type": "integer",
                            "description": "Timeout in seconds to wait for indexing completion."
                        }
                    }
                })),
            ),
        ];

        Box::pin(async move {
            Ok(ListToolsResult {
                tools,
                ..Default::default()
            })
        })
    }

    #[allow(refining_impl_trait)]
    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Pin<Box<dyn Future<Output = Result<CallToolResult, ErrorData>> + Send + '_>> {
        let name = request.name.clone();
        // Convert Option<JsonObject> to String for JSON parsing
        let arguments = match request.arguments {
            Some(obj) => serde_json::to_string(&obj).unwrap_or_else(|_| "{}".to_string()),
            None => "{}".to_string(),
        };
        let handler = self.clone();

        Box::pin(async move {
            match name.as_ref() {
                "get_project_details" => handler.handle_get_project_details(arguments).await,
                "search_symbols" => handler.handle_search_symbols(arguments).await,
                "analyze_symbol_context" => handler.handle_analyze_symbol_context(arguments).await,
                _ => Err(ErrorData::invalid_params(
                    format!("Unknown tool: {}", name),
                    None,
                )),
            }
        })
    }
}
