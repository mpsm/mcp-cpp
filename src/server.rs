use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    CallToolRequest, CallToolResult, ListToolsRequest, ListToolsResult, RpcError,
    schema_utils::CallToolError,
};
use rust_mcp_sdk::{McpServer, mcp_server::ServerHandler};
use tracing::{Level, info};

use crate::legacy_lsp::manager::ClangdManager;
use crate::project::MetaProject;
use crate::register_tools;
use crate::tools::analyze_symbols::AnalyzeSymbolContextTool;
use crate::tools::project_tools::GetProjectDetailsTool;
use crate::tools::search_symbols::SearchSymbolsTool;
use crate::tools::utils::McpToolHandler;
use crate::{log_mcp_message, log_timing};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

pub struct CppServerHandler {
    clangd_manager: Arc<Mutex<ClangdManager>>,
    meta_project: MetaProject,
}

impl CppServerHandler {
    pub fn new(meta_project: MetaProject) -> Self {
        Self {
            clangd_manager: Arc::new(Mutex::new(ClangdManager::new())),
            meta_project,
        }
    }
}

// Implement McpToolHandler trait for each tool type
impl McpToolHandler<GetProjectDetailsTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "get_project_details";

    fn call_tool_sync(&self, tool: GetProjectDetailsTool) -> Result<CallToolResult, CallToolError> {
        tool.call_tool(&self.meta_project)
    }
}

impl McpToolHandler<SearchSymbolsTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "search_symbols";

    async fn call_tool_async(
        &self,
        tool: SearchSymbolsTool,
    ) -> Result<CallToolResult, CallToolError> {
        tool.call_tool(&self.clangd_manager).await
    }
}

impl McpToolHandler<AnalyzeSymbolContextTool> for CppServerHandler {
    const TOOL_NAME: &'static str = "analyze_symbol_context";

    async fn call_tool_async(
        &self,
        tool: AnalyzeSymbolContextTool,
    ) -> Result<CallToolResult, CallToolError> {
        tool.call_tool(&self.clangd_manager).await
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
