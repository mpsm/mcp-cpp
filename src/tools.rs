use rust_mcp_sdk::schema::{schema_utils::CallToolError, CallToolResult, TextContent};
use rust_mcp_sdk::{
    macros::{mcp_tool, JsonSchema},
    tool_box,
};
use serde_json::json;
use tracing::{info, error, instrument};

use crate::cmake::{CmakeProjectStatus, CmakeError};

#[mcp_tool(
    name = "cpp_project_status",
    description = "Analyze C++ project status, including CMake configuration and build directories",
    title = "C++ Project Status Analyzer",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct CppProjectStatusTool {
    // No parameters needed for analyzing current directory
}

impl CppProjectStatusTool {
    #[instrument(name = "cpp_project_status", skip(self))]
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        info!("Executing C++ project status analysis");
        
        match CmakeProjectStatus::analyze_current_directory() {
            Ok(status) => {
                let content = json!({
                    "success": true,
                    "project_type": "cmake",
                    "is_configured": !status.build_directories.is_empty(),
                    "build_directories": status.build_directories.iter().map(|bd| {
                        json!({
                            "path": bd.relative_path,
                            "absolute_path": bd.path,
                            "generator": bd.generator,
                            "build_type": bd.build_type,
                            "cache_exists": bd.cache_exists,
                            "cache_readable": bd.cache_readable,
                            "configured_options": bd.configured_options.iter().map(|(k, v)| {
                                json!({ "name": k, "value": v })
                            }).collect::<Vec<_>>()
                        })
                    }).collect::<Vec<_>>(),
                    "issues": status.issues,
                    "summary": Self::generate_summary(&status)
                });
                
                info!("Successfully analyzed C++ project");
                
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serde_json::to_string_pretty(&content)
                        .unwrap_or_else(|e| format!("Error serializing result: {}", e))
                )]))
            }
            Err(CmakeError::NotCmakeProject) => {
                let content = json!({
                    "success": true,
                    "project_type": "unknown",
                    "is_configured": false,
                    "message": "Current directory is not a CMake project (no CMakeLists.txt found)",
                    "build_directories": [],
                    "issues": [],
                    "summary": "Not a CMake project"
                });
                
                info!("Directory is not a CMake project");
                
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serde_json::to_string_pretty(&content)
                        .unwrap_or_else(|e| format!("Error serializing result: {}", e))
                )]))
            }
            Err(CmakeError::MultipleIssues(issues)) => {
                let content = json!({
                    "success": false,
                    "project_type": "cmake",
                    "is_configured": false,
                    "error": "Multiple issues detected with build directories",
                    "build_directories": [],
                    "issues": issues,
                    "summary": format!("CMake project with {} issues", issues.len())
                });
                
                error!("Multiple issues detected: {:?}", issues);
                
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serde_json::to_string_pretty(&content)
                        .unwrap_or_else(|e| format!("Error serializing result: {}", e))
                )]))
            }
            Err(e) => {
                let error_msg = format!("Failed to analyze project: {}", e);
                let content = json!({
                    "success": false,
                    "project_type": "unknown",
                    "is_configured": false,
                    "error": error_msg,
                    "build_directories": [],
                    "issues": [error_msg.clone()],
                    "summary": "Analysis failed"
                });
                
                error!("Project analysis failed: {}", e);
                
                // Return error result with JSON content
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serde_json::to_string_pretty(&content)
                        .unwrap_or_else(|e| format!("Error serializing result: {}", e))
                )]))
            }
        }
    }

    fn generate_summary(status: &CmakeProjectStatus) -> String {
        if !status.is_cmake_project {
            return "Not a CMake project".to_string();
        }
        
        match status.build_directories.len() {
            0 => "CMake project (not configured)".to_string(),
            1 => {
                let bd = &status.build_directories[0];
                let generator = bd.generator.as_deref().unwrap_or("unknown generator");
                let build_type = bd.build_type.as_deref().unwrap_or("unspecified");
                format!("CMake project configured with {} ({})", generator, build_type)
            }
            n => {
                let generators: Vec<String> = status.build_directories
                    .iter()
                    .filter_map(|bd| bd.generator.as_ref())
                    .cloned()
                    .collect();
                let unique_generators: std::collections::HashSet<_> = generators.into_iter().collect();
                
                if unique_generators.len() == 1 {
                    format!("CMake project with {} build directories ({})", 
                           n, unique_generators.iter().next().unwrap())
                } else {
                    format!("CMake project with {} build directories (mixed generators)", n)
                }
            }
        }
    }
}

// Generate CppTools enum with CppProjectStatusTool variant
tool_box!(CppTools, [CppProjectStatusTool]);