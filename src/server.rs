use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    CallToolRequest, CallToolResult, ListToolsRequest, ListToolsResult, RpcError,
    schema_utils::CallToolError,
};
use rust_mcp_sdk::{McpServer, mcp_server::ServerHandler};
use tracing::{Level, info};

#[cfg(not(feature = "tools-v2"))]
use crate::legacy_lsp::manager::ClangdManager;
use crate::project::ProjectWorkspace;
#[cfg(feature = "tools-v2")]
use crate::project::WorkspaceSession;
use crate::register_tools;
#[cfg(feature = "tools-v2")]
use crate::server_helpers;
#[cfg(not(feature = "tools-v2"))]
use crate::tools::analyze_symbols::AnalyzeSymbolContextTool;
#[cfg(feature = "tools-v2")]
use crate::tools::analyze_symbols_v2::AnalyzeSymbolContextTool;
use crate::tools::project_tools::GetProjectDetailsTool;
#[cfg(not(feature = "tools-v2"))]
use crate::tools::search_symbols::SearchSymbolsTool;
#[cfg(feature = "tools-v2")]
use crate::tools::search_symbols_v2::SearchSymbolsTool;
use crate::tools::utils::McpToolHandler;
use crate::{log_mcp_message, log_timing};
#[cfg(feature = "tools-v2")]
use std::path::PathBuf;
#[cfg(not(feature = "tools-v2"))]
use std::sync::Arc;
use std::time::Instant;
#[cfg(not(feature = "tools-v2"))]
use tokio::sync::Mutex;

pub struct CppServerHandler {
    #[cfg(not(feature = "tools-v2"))]
    clangd_manager: Arc<Mutex<ClangdManager>>,
    project_workspace: ProjectWorkspace,
    #[cfg(feature = "tools-v2")]
    workspace_session: WorkspaceSession,
}

impl CppServerHandler {
    pub fn new(project_workspace: ProjectWorkspace) -> Self {
        #[cfg(feature = "tools-v2")]
        let workspace_session = WorkspaceSession::new(project_workspace.clone());
        Self {
            #[cfg(not(feature = "tools-v2"))]
            clangd_manager: Arc::new(Mutex::new(ClangdManager::new())),
            project_workspace,
            #[cfg(feature = "tools-v2")]
            workspace_session,
        }
    }

    /// Resolves build directory from optional parameter using the helper function.
    #[cfg(feature = "tools-v2")]
    fn resolve_build_directory(
        &self,
        requested_build_dir: Option<&str>,
    ) -> Result<PathBuf, CallToolError> {
        server_helpers::resolve_build_directory(&self.project_workspace, requested_build_dir)
    }
}

// Implement McpToolHandler trait for each tool type
impl McpToolHandler<GetProjectDetailsTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "get_project_details";

    fn call_tool_sync(&self, tool: GetProjectDetailsTool) -> Result<CallToolResult, CallToolError> {
        tool.call_tool(&self.project_workspace)
    }
}

#[cfg(not(feature = "tools-v2"))]
impl McpToolHandler<SearchSymbolsTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "search_symbols";

    async fn call_tool_async(
        &self,
        tool: SearchSymbolsTool,
    ) -> Result<CallToolResult, CallToolError> {
        tool.call_tool(&self.clangd_manager).await
    }
}

#[cfg(feature = "tools-v2")]
impl McpToolHandler<SearchSymbolsTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "search_symbols";

    async fn call_tool_async(
        &self,
        tool: SearchSymbolsTool,
    ) -> Result<CallToolResult, CallToolError> {
        let build_dir = self.resolve_build_directory(tool.build_directory.as_deref())?;

        let clangd_session = self
            .workspace_session
            .get_or_create_session(build_dir)
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!(
                    "Session creation failed: {}",
                    e
                )))
            })?;

        tool.call_tool_v2(clangd_session, &self.project_workspace)
            .await
    }
}

#[cfg(not(feature = "tools-v2"))]
impl McpToolHandler<AnalyzeSymbolContextTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "analyze_symbol_context";

    async fn call_tool_async(
        &self,
        tool: AnalyzeSymbolContextTool,
    ) -> Result<CallToolResult, CallToolError> {
        tool.call_tool(&self.clangd_manager).await
    }
}

#[cfg(feature = "tools-v2")]
impl McpToolHandler<AnalyzeSymbolContextTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "analyze_symbol_context";

    async fn call_tool_async(
        &self,
        tool: AnalyzeSymbolContextTool,
    ) -> Result<CallToolResult, CallToolError> {
        let build_dir = self.resolve_build_directory(tool.build_directory.as_deref())?;

        let clangd_session = self
            .workspace_session
            .get_or_create_session(build_dir)
            .await
            .map_err(|e| {
                CallToolError::new(std::io::Error::other(format!(
                    "Session creation failed: {}",
                    e
                )))
            })?;

        tool.call_tool_v2(clangd_session, &self.project_workspace)
            .await
    }
}

// Register all tools with compile-time safety - this generates dispatch_tool() and registered_tools()
register_tools! {
    CppServerHandler {
        GetProjectDetailsTool => call_tool_sync (sync),
        SearchSymbolsTool => call_tool_async (async),
        AnalyzeSymbolContextTool => call_tool_async (async),
    }
}

#[async_trait]
impl ServerHandler for CppServerHandler {
    async fn handle_list_tools_request(
        &self,
        request: ListToolsRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        let start = Instant::now();

        log_mcp_message!(Level::INFO, "incoming", "list_tools", &request);
        info!("Listing available tools");

        let result = ListToolsResult {
            meta: None,
            next_cursor: None,
            tools: Self::registered_tools(),
        };

        log_mcp_message!(Level::INFO, "outgoing", "list_tools", &result);
        log_timing!(Level::DEBUG, "list_tools", start.elapsed());

        Ok(result)
    }

    async fn handle_call_tool_request(
        &self,
        request: CallToolRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let start = Instant::now();
        let tool_name = request.params.name.clone();

        log_mcp_message!(Level::INFO, "incoming", "call_tool", &request);
        info!("Executing tool: {}", tool_name);

        // Generated dispatch with compile-time safety
        let result = self
            .dispatch_tool(&tool_name, request.params.arguments)
            .await?;

        log_mcp_message!(Level::INFO, "outgoing", "call_tool", &result);
        log_timing!(
            Level::DEBUG,
            &format!("call_tool_{tool_name}"),
            start.elapsed()
        );

        Ok(result)
    }
}
