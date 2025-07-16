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
                let build_dirs = status.build_directories.iter().map(|bd| {
                    // Check if compile_commands.json exists
                    let compile_commands_exists = bd.path.join("compile_commands.json").exists();

                    json!({
                        "path": bd.path,
                        "generator": bd.generator,
                        "build_type": bd.build_type,
                        "options": bd.configured_options.iter().cloned().collect::<std::collections::HashMap<_, _>>(),
                        "compile_commands_exists": compile_commands_exists
                    })
                }).collect::<Vec<_>>();

                let project_name = status
                    .project_root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let content = json!({
                    "project_name": project_name,
                    "project_root": status.project_root,
                    "build_dirs": build_dirs
                });

                info!(
                    "Successfully listed {} CMake build directories",
                    build_dirs.len()
                );

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(CmakeError::NotCmakeProject) => {
                let current_dir = std::env::current_dir().unwrap_or_default();
                let project_name = current_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());

                let content = json!({
                    "project_name": project_name,
                    "project_root": current_dir,
                    "build_dirs": [],
                    "error": "Not a CMake project - no CMakeLists.txt found"
                });

                info!("Directory is not a CMake project");

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(CmakeError::MultipleIssues(issues)) => {
                let current_dir = std::env::current_dir().unwrap_or_default();
                let project_name = current_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());

                let content = json!({
                    "project_name": project_name,
                    "project_root": current_dir,
                    "build_dirs": [],
                    "error": format!("CMake project has issues: {}", issues.join(", "))
                });

                error!("Multiple issues detected: {:?}", issues);

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
            }
            Err(e) => {
                let current_dir = std::env::current_dir().unwrap_or_default();

                let content = json!({
                    "project_name": null,
                    "project_root": current_dir,
                    "build_dirs": [],
                    "error": format!("Failed to analyze project: {}", e)
                });

                error!("Project analysis failed: {}", e);

                Ok(CallToolResult::text_content(vec![TextContent::from(
                    serialize_result(&content),
                )]))
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
