//! Common utilities for MCP tools

use rust_mcp_sdk::schema::{CallToolResult, schema_utils::CallToolError};
use serde::de::DeserializeOwned;
#[cfg(feature = "tools-v2")]
use tracing::{info, warn};

/// Default timeout for waiting for clangd indexing to complete
#[cfg(feature = "tools-v2")]
pub const INDEXING_WAIT_TIMEOUT_SECS: u64 = 30;

/// Wait for clangd indexing to complete with timeout
///
/// This helper function waits for clangd to finish indexing the codebase,
/// with a configurable timeout. It logs the result and continues execution
/// regardless of whether indexing completes successfully, fails, or times out.
///
/// # Arguments
/// * `index_monitor` - The index monitor from a ClangdSession
/// * `timeout_secs` - Optional timeout in seconds (defaults to INDEXING_WAIT_TIMEOUT_SECS)
#[cfg(feature = "tools-v2")]
pub async fn wait_for_indexing(
    index_monitor: &crate::clangd::index::IndexMonitor,
    timeout_secs: Option<u64>,
) {
    let timeout =
        std::time::Duration::from_secs(timeout_secs.unwrap_or(INDEXING_WAIT_TIMEOUT_SECS));

    info!("Waiting for clangd indexing to complete...");
    match tokio::time::timeout(timeout, index_monitor.wait_for_indexing_completion()).await {
        Ok(Ok(())) => info!("Indexing completed successfully"),
        Ok(Err(e)) => {
            warn!("Indexing wait failed (continuing anyway): {}", e);
        }
        Err(_) => {
            warn!(
                "Indexing wait timed out after {} seconds (continuing anyway)",
                timeout.as_secs()
            );
        }
    }
}

/// Helper function to serialize JSON content and handle errors gracefully
pub fn serialize_result(content: &serde_json::Value) -> String {
    serde_json::to_string_pretty(content)
        .unwrap_or_else(|e| format!("Error serializing result: {e}"))
}

/// Extension trait for cleaner tool argument deserialization
pub trait ToolArguments {
    /// Deserialize MCP tool arguments to a concrete tool type
    fn deserialize_tool<T: DeserializeOwned>(self, tool_name: &str) -> Result<T, CallToolError>;
}

impl ToolArguments for Option<serde_json::Map<String, serde_json::Value>> {
    fn deserialize_tool<T: DeserializeOwned>(self, tool_name: &str) -> Result<T, CallToolError> {
        serde_json::from_value(
            self.map(serde_json::Value::Object)
                .unwrap_or(serde_json::Value::Null),
        )
        .map_err(|e| {
            CallToolError::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Failed to deserialize {tool_name} arguments: {e}"),
            ))
        })
    }
}

/// Trait for unified MCP tool handling with compile-time safety
pub trait McpToolHandler<T> {
    /// The tool name (must match the #[mcp_tool] name attribute)
    const TOOL_NAME: &'static str;

    /// Handle sync tools (default implementation panics - override for sync tools)
    fn call_tool_sync(&self, _tool: T) -> Result<CallToolResult, CallToolError> {
        panic!("call_tool_sync not implemented - this tool should use call_tool_async")
    }

    /// Handle async tools (default implementation panics - override for async tools)  
    async fn call_tool_async(&self, _tool: T) -> Result<CallToolResult, CallToolError> {
        panic!("call_tool_async not implemented - this tool should use call_tool_sync")
    }
}

/// Macro for registering MCP tools with compile-time safety
///
/// Usage:
/// ```
/// register_tools! {
///     HandlerType {
///         ToolStruct => handler_method (sync),
///         AnotherTool => another_handler (async),
///     }
/// }
/// ```
#[macro_export]
macro_rules! register_tools {
    ($handler_type:ty {
        $($tool_type:ty => $handler_method:ident ($tool_mode:ident)),+ $(,)?
    }) => {
        impl $handler_type {
            /// Generate the dispatch function with compile-time safety
            pub async fn dispatch_tool(
                &self,
                tool_name: &str,
                arguments: Option<serde_json::Map<String, serde_json::Value>>,
            ) -> Result<rust_mcp_sdk::schema::CallToolResult, rust_mcp_sdk::schema::schema_utils::CallToolError> {
                use $crate::tools::utils::{McpToolHandler, ToolArguments};

                match tool_name {
                    $(
                        <Self as McpToolHandler<$tool_type>>::TOOL_NAME => {
                            let tool: $tool_type = arguments.deserialize_tool(tool_name)?;
                            register_tools!(@dispatch_call self, tool, $tool_mode)
                        }
                    )+
                    _ => Err(rust_mcp_sdk::schema::schema_utils::CallToolError::unknown_tool(
                        format!("Unknown tool: {}", tool_name)
                    ))
                }
            }

            /// Generate the tool registration list
            pub fn registered_tools() -> Vec<rust_mcp_sdk::schema::Tool> {
                vec![
                    $(
                        <$tool_type>::tool(),
                    )+
                ]
            }
        }
    };

    // Helper macro for sync vs async dispatch
    (@dispatch_call $self:expr, $tool:expr, sync) => {
        $self.call_tool_sync($tool)
    };
    (@dispatch_call $self:expr, $tool:expr, async) => {
        $self.call_tool_async($tool).await
    };
}
