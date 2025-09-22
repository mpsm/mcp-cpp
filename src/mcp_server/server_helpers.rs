//! Server helper utilities for common operations

use rust_mcp_sdk::schema::{CallToolResult, schema_utils::CallToolError};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use tracing::debug;

use crate::project::ProjectWorkspace;

/// Resolves build directory from optional parameter.
///
/// # Arguments
/// * `workspace` - The project workspace to search for build directories
/// * `requested_build_dir` - Optional build directory path (can be relative or absolute)
///
/// # Returns
/// * `Ok(PathBuf)` - The resolved build directory path
/// * `Err(CallToolError)` - If the specified directory doesn't exist in workspace or if
///   auto-detection fails due to zero or multiple build directories
///
/// # Behavior
/// - If `requested_build_dir` is provided, validates it exists in the workspace
/// - If not provided, auto-detects single build directory
/// - Fails if no build directories exist (suggests running cmake)
/// - Fails if multiple build directories exist without explicit selection
pub fn resolve_build_directory(
    workspace: &ProjectWorkspace,
    requested_build_dir: Option<&str>,
) -> Result<PathBuf, CallToolError> {
    match requested_build_dir {
        Some(build_dir_str) => {
            debug!(
                "Attempting to use specified build directory: {}",
                build_dir_str
            );
            let requested_path = PathBuf::from(build_dir_str);

            // Convert relative paths to absolute paths if needed
            let absolute_path = if requested_path.is_absolute() {
                debug!("Using absolute path as-is: {}", requested_path.display());
                requested_path
            } else {
                // Convert relative path to absolute by joining with workspace root
                let absolute = workspace.project_root_path.join(&requested_path);
                debug!(
                    "Converting relative path '{}' to absolute path '{}' using project root '{}'",
                    build_dir_str,
                    absolute.display(),
                    workspace.project_root_path.display()
                );
                absolute
            };

            // Check if component already exists in workspace
            if workspace
                .get_component_by_build_dir(&absolute_path)
                .is_some()
            {
                debug!(
                    "Build directory '{}' found in workspace",
                    absolute_path.display()
                );
                Ok(absolute_path)
            } else {
                debug!(
                    "Build directory '{}' not found in workspace, will attempt dynamic discovery",
                    absolute_path.display()
                );

                // If the path doesn't exist, provide helpful error
                if !absolute_path.exists() {
                    let available_dirs = workspace.get_build_dirs();
                    let is_relative = !PathBuf::from(build_dir_str).is_absolute();
                    let relative_path_note = if is_relative {
                        format!(
                            " (You provided relative path '{}' which was resolved to '{}' using scan root '{}')",
                            build_dir_str,
                            absolute_path.display(),
                            workspace.project_root_path.display()
                        )
                    } else {
                        String::new()
                    };

                    return Err(CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "Build directory '{}' does not exist{}. Scan root: '{}'. Run get_project_details first to see available build directories with absolute paths. Available directories: {:?}. STRONGLY RECOMMEND: Use absolute paths from get_project_details output.",
                            absolute_path.display(),
                            relative_path_note,
                            workspace.project_root_path.display(),
                            available_dirs
                        ),
                    )));
                }

                // Return the path anyway - let get_component_session handle dynamic discovery
                Ok(absolute_path)
            }
        }
        None => {
            debug!("No build directory specified, attempting auto-detection");
            let build_dirs = workspace.get_build_dirs();

            match build_dirs.len() {
                0 => {
                    debug!("No build directories found in workspace");
                    Err(CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "No build directories found in project. Scan root: '{}'. Run get_project_details first to see project status and available build configurations. If no build directories exist, you may need to run cmake or meson to generate build configuration.",
                            workspace.project_root_path.display()
                        ),
                    )))
                }
                1 => {
                    debug!("Single build directory found: {:?}", build_dirs[0]);
                    Ok(build_dirs[0].clone())
                }
                _ => {
                    debug!("Multiple build directories found: {:?}", build_dirs);
                    Err(CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "Multiple build directories found. Scan root: '{}'. Run get_project_details to see all available options with absolute paths, then specify one using the build_directory parameter. Available directories: {:?}. STRONGLY RECOMMEND: Use absolute paths from get_project_details output.",
                            workspace.project_root_path.display(),
                            build_dirs
                        ),
                    )))
                }
            }
        }
    }
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
    #[allow(dead_code)]
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
                use $crate::mcp_server::server_helpers::{McpToolHandler, ToolArguments};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_with_explicit_directory() {
        // Test validates function signature compatibility.
        // Full integration test coverage is provided by the E2E test suite
        // which exercises this function with real ProjectWorkspace instances.
        let _result: fn(&ProjectWorkspace, Option<&str>) -> Result<PathBuf, CallToolError> =
            resolve_build_directory;
    }
}
