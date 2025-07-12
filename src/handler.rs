use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    schema_utils::CallToolError, CallToolRequest, CallToolResult, ListToolsRequest,
    ListToolsResult, ListResourcesRequest, ListResourcesResult, ReadResourceRequest,
    ReadResourceResult, RpcError,
};
use rust_mcp_sdk::{mcp_server::ServerHandler, McpServer};
use tracing::info;

use crate::tools::CppTools;
use crate::resources::LspResources;

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
            CppTools::try_from(request).map_err(|e| CallToolError::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))?;

        // Match the tool variant and execute its corresponding logic
        match tool_params {
            CppTools::CppProjectStatus(cpp_status_tool) => cpp_status_tool.call_tool(),
            CppTools::SetupClangd(setup_tool) => setup_tool.call_tool().await,
            CppTools::LspRequest(lsp_tool) => lsp_tool.call_tool().await,
        }
    }

    async fn handle_list_resources_request(
        &self,
        request: ListResourcesRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ListResourcesResult, RpcError> {
        info!("Listing available resources");
        
        LspResources::list_resources(request)
            .map_err(|_e| RpcError::internal_error())
    }

    async fn handle_read_resource_request(
        &self,
        request: ReadResourceRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ReadResourceResult, RpcError> {
        info!("Reading resource: {}", request.params.uri);
        
        LspResources::read_resource(request)
            .map_err(|_e| RpcError::internal_error())
    }
}