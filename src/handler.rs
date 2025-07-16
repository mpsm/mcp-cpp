use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    CallToolRequest, CallToolResult, ListToolsRequest,
    ListToolsResult, RpcError,
    schema_utils::CallToolError,
};
use rust_mcp_sdk::{McpServer, mcp_server::ServerHandler};
use tracing::{Level, info};

use crate::lsp::manager::ClangdManager;
use crate::tools::CppTools;
use crate::{log_mcp_message, log_timing};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

pub struct CppServerHandler {
    clangd_manager: Arc<Mutex<ClangdManager>>,
}

impl CppServerHandler {
    pub fn new() -> Self {
        Self {
            clangd_manager: Arc::new(Mutex::new(ClangdManager::new())),
        }
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
            tools: CppTools::tools(),
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

        let result = CppTools::handle_call(
            &tool_name,
            request.params.arguments.map(serde_json::Value::Object).unwrap_or(serde_json::Value::Null),
            &self.clangd_manager,
        ).await;

        log_mcp_message!(Level::INFO, "outgoing", "call_tool", &result);
        log_timing!(
            Level::DEBUG,
            &format!("call_tool_{}", tool_name),
            start.elapsed()
        );

        result
    }

}
