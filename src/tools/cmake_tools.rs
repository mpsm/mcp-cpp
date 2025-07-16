//! CMake project analysis tools

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use tracing::{error, info, instrument};

use super::serialize_result;
use crate::cmake::{CmakeError, CmakeProjectStatus};

#[mcp_tool(
    name = "list_build_dirs",
    description = "Comprehensive CMake build environment analyzer providing detailed discovery and analysis \
                   of all build directories in the current workspace. Essential prerequisite tool for any \
                   compilation-dependent operations including symbol analysis and LSP server initialization.

                   🏗️ BUILD DIRECTORY DISCOVERY:
                   • Deep filesystem scanning for configured build directories
                   • Detection of both active and potential build locations
                   • Common build patterns: build/, Debug/, Release/, out/
                   • Custom build directory identification via CMake cache analysis

                   ⚙️ CONFIGURATION ANALYSIS:
                   • CMake generator type: Ninja, Unix Makefiles, Visual Studio, Xcode
                   • Build type classification: Debug, Release, RelWithDebInfo, MinSizeRel
                   • Compiler toolchain detection: GCC, Clang, MSVC versions
                   • CMake version and configuration timestamp

                   📋 COMPILATION DATABASE STATUS:
                   • compile_commands.json availability and validity
                   • LSP server compatibility assessment  
                   • Clangd integration readiness verification
                   • Source file coverage analysis

                   🎯 BUILD TARGETS & OPTIONS:
                   • Configured CMake targets and executables
                   • Build options and feature flags (CMAKE_BUILD_TYPE, etc.)
                   • Dependency library detection
                   • Installation prefix and output paths

                   🚀 INTEGRATION BENEFITS:
                   • Automatic build directory selection for single-config projects
                   • Multi-configuration project guidance and selection prompts
                   • LSP server initialization with optimal build context
                   • Symbol analysis prerequisite validation

                   🎯 PRIMARY USE CASES:
                   Build environment assessment • LSP setup validation • Multi-config project navigation
                   • Compilation troubleshooting • Development environment verification

                   INPUT REQUIREMENTS:
                   • No parameters required - analyzes current workspace automatically
                   • Operates on current working directory and subdirectories
                   • Results include actionable recommendations for next steps"
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct ListBuildDirsTool {
    // No parameters needed for analyzing current directory
}

impl ListBuildDirsTool {
    #[instrument(name = "list_build_dirs", skip(self))]
    pub fn call_tool(&self) -> Result<CallToolResult, CallToolError> {
        info!("Listing available CMake build directories");

        match CmakeProjectStatus::analyze_current_directory() {
            Ok(status) => {
                let content = json!({
                    "summary": Self::generate_summary(&status)
                });

                info!("Successfully listed CMake build directories");

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(CmakeError::NotCmakeProject) => {
                let content = json!({
                    "summary": "Not a CMake project"
                });

                info!("Directory is not a CMake project");

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(CmakeError::MultipleIssues(issues)) => {
                let content = json!({
                    "summary": format!("CMake project with {} issues", issues.len())
                });

                error!("Multiple issues detected: {:?}", issues);

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
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
                    serialize_result(&content),
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
                format!(
                    "CMake project configured with {} ({})",
                    generator, build_type
                )
            }
            n => {
                let generators: Vec<String> = status
                    .build_directories
                    .iter()
                    .filter_map(|bd| bd.generator.as_ref())
                    .cloned()
                    .collect();
                let unique_generators: std::collections::HashSet<_> =
                    generators.into_iter().collect();

                if unique_generators.len() == 1 {
                    let generator = unique_generators
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| "unknown generator".to_string());
                    format!("CMake project with {} build directories ({})", n, generator)
                } else {
                    format!(
                        "CMake project with {} build directories (multiple generators)",
                        n
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_build_dirs_tool_creation() {
        let _tool = ListBuildDirsTool {};
        // Tool name is generated by the mcp_tool macro
        assert_eq!(ListBuildDirsTool::tool().name, "list_build_dirs");
    }
}
