use async_trait::async_trait;
use rust_mcp_sdk::schema::{
    CallToolRequest, CallToolResult, ListToolsRequest, ListToolsResult, RpcError,
    schema_utils::CallToolError,
};
use rust_mcp_sdk::{McpServer, mcp_server::ServerHandler};
use tracing::{Level, info};

use crate::lsp::manager::ClangdManager;
use crate::project::{MetaProject, ProjectScanner};
use crate::tools::CppTools;
use crate::{log_mcp_message, log_timing};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::warn;

pub struct CppServerHandler {
    clangd_manager: Arc<Mutex<ClangdManager>>,
    meta_project: MetaProject,
}

impl CppServerHandler {
    pub fn new(project_root: PathBuf) -> Self {
        let meta_project = Self::scan_project_root(&project_root, 3);

        Self {
            clangd_manager: Arc::new(Mutex::new(ClangdManager::new())),
            meta_project,
        }
    }

    /// Scan project root with specified depth
    fn scan_project_root(project_root: &Path, depth: usize) -> MetaProject {
        info!(
            "Scanning project root for build configurations: {} (depth: {})",
            project_root.display(),
            depth
        );

        // Create project scanner with default providers
        let scanner = ProjectScanner::with_default_providers();

        // Scan the project root with specified depth
        match scanner.scan_project(project_root, depth, None) {
            Ok(meta_project) => {
                info!(
                    "Successfully scanned project: found {} components with providers: {:?}",
                    meta_project.component_count(),
                    meta_project.get_provider_types()
                );
                meta_project
            }
            Err(e) => {
                warn!(
                    "Failed to scan project root {}: {}. Creating empty MetaProject.",
                    project_root.display(),
                    e
                );
                // Create empty MetaProject as fallback
                MetaProject::new(project_root.to_path_buf(), Vec::new(), depth)
            }
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
            request
                .params
                .arguments
                .map(serde_json::Value::Object)
                .unwrap_or(serde_json::Value::Null),
            &self.clangd_manager,
            &self.meta_project,
        )
        .await;

        log_mcp_message!(Level::INFO, "outgoing", "call_tool", &result);
        log_timing!(
            Level::DEBUG,
            &format!("call_tool_{tool_name}"),
            start.elapsed()
        );

        result
    }
}
