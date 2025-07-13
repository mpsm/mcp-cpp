use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    CallToolRequest, CallToolResult, ListResourcesRequest, ListResourcesResult, ListToolsRequest,
    ListToolsResult, ReadResourceRequest, ReadResourceResult, RpcError,
    schema_utils::CallToolError,
};
use rust_mcp_sdk::{McpServer, mcp_server::ServerHandler};
use tracing::{info, Level};

use crate::lsp::manager::ClangdManager;
use crate::resources::LspResources;
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

        // Convert request parameters into CppTools enum
        let tool_params: CppTools = CppTools::try_from(request).map_err(|e| {
            CallToolError::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;

        // Match the tool variant and execute its corresponding logic
        let result = match tool_params {
            CppTools::CppProjectStatus(cpp_status_tool) => cpp_status_tool.call_tool(),
            CppTools::SetupClangd(setup_tool) => setup_tool.call_tool(&self.clangd_manager).await,
            CppTools::LspRequest(lsp_tool) => lsp_tool.call_tool(&self.clangd_manager).await,
        };
        
        log_mcp_message!(Level::INFO, "outgoing", "call_tool", &result);
        log_timing!(Level::DEBUG, &format!("call_tool_{}", tool_name), start.elapsed());
        
        result
    }

    async fn handle_list_resources_request(
        &self,
        request: ListResourcesRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ListResourcesResult, RpcError> {
        let start = Instant::now();
        
        log_mcp_message!(Level::INFO, "incoming", "list_resources", &request);
        info!("Listing available resources");

        let result = LspResources::list_resources(request).map_err(|_e| RpcError::internal_error())?;
        
        log_mcp_message!(Level::INFO, "outgoing", "list_resources", &result);
        log_timing!(Level::DEBUG, "list_resources", start.elapsed());
        
        Ok(result)
    }

    async fn handle_read_resource_request(
        &self,
        request: ReadResourceRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ReadResourceResult, RpcError> {
        let start = Instant::now();
        let uri = request.params.uri.clone();
        
        log_mcp_message!(Level::INFO, "incoming", "read_resource", &request);
        info!("Reading resource: {}", uri);

        let result = LspResources::read_resource(request).map_err(|_e| RpcError::internal_error())?;
        
        log_mcp_message!(Level::INFO, "outgoing", "read_resource", &result);
        log_timing!(Level::DEBUG, &format!("read_resource_{}", uri), start.elapsed());
        
        Ok(result)
    }
}
