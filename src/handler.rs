use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    schema_utils::CallToolError, CallToolRequest, CallToolResult, ListToolsRequest,
    ListToolsResult, RpcError,
};
use rust_mcp_sdk::{mcp_server::ServerHandler, McpServer};
use tracing::info;

use crate::tools::CppTools;

pub struct CppServerHandler;

#[async_trait]
impl ServerHandler for CppServerHandler {
    async fn handle_list_tools_request(
        &self,
        _request: ListToolsRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        info!("Listing available tools");

        Ok(ListToolsResult {
            meta: None,
            next_cursor: None,
            tools: CppTools::tools(),
        })
    }

    async fn handle_call_tool_request(
        &self,
        request: CallToolRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        info!("Executing tool: {}", request.params.name);

        // Convert request parameters into CppTools enum
        let tool_params: CppTools =
            CppTools::try_from(request.params).map_err(|e| CallToolError::new(e))?;

        // Match the tool variant and execute its corresponding logic
        match tool_params {
            CppTools::CppProjectStatusTool(cpp_status_tool) => cpp_status_tool.call_tool(),
        }
    }
}