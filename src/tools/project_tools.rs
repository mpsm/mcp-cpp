//! Project analysis tools for multi-provider build systems

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use serde_json::json;
use tracing::{info, instrument};

use super::serialize_result;
use crate::project::MetaProject;

#[mcp_tool(
    name = "list_project_components",
    description = "Comprehensive multi-provider project analysis tool that discovers and analyzes all build \
                   system configurations within a workspace. Works with CMake, Meson, and other supported \
                   build systems to provide unified project intelligence.

                   🏗️ MULTI-PROVIDER DISCOVERY:
                   • Automatic detection of CMake projects (CMakeLists.txt + build directories)
                   • Meson project support (meson.build + build configurations)
                   • Extensible architecture ready for Bazel, Buck, xmake, and other build systems
                   • Unified component representation across all providers

                   ⚙️ BUILD CONFIGURATION ANALYSIS:
                   • Generator type identification (Ninja, Unix Makefiles, Visual Studio, etc.)
                   • Build type classification (Debug, Release, RelWithDebInfo, MinSizeRel)
                   • Compiler toolchain detection and version information
                   • Build options and feature flags extraction

                   📋 COMPILATION DATABASE STATUS:
                   • compile_commands.json availability and validity for each component
                   • LSP server compatibility assessment across all build systems
                   • Cross-provider standardized metadata format

                   🎯 PROJECT COMPONENT STRUCTURE:
                   Each discovered component includes:
                   • Build directory path and source root location
                   • Provider type (cmake, meson, etc.) for build system identification
                   • Generator and build type information in standardized format
                   • Complete build options and configuration details
                   • Compilation database status for LSP integration

                   🚀 INTEGRATION BENEFITS:
                   • Single tool for all supported build systems
                   • Consistent component format enables universal tooling
                   • Perfect for polyglot projects using multiple build systems
                   • Foundation for LSP server initialization across providers

                   🎯 PRIMARY USE CASES:
                   Multi-provider project assessment • Build system inventory • LSP setup validation
                   • Polyglot project navigation • Development environment verification
                   • CI/CD build matrix generation

                   INPUT REQUIREMENTS:
                   • No parameters required - analyzes discovered project components
                   • Uses MetaProject workspace scanning results
                   • Returns all components regardless of provider type"
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct ListProjectComponentsTool {
    /// Optional project root path to scan. DEFAULT: uses server's initial scan root.
    ///
    /// FORMATS ACCEPTED:
    /// • Relative path: ".", "..", "subproject/"
    /// • Absolute path: "/home/project", "/path/to/workspace"
    ///
    /// BEHAVIOR: When specified and different from server's initial scan, performs
    /// a fresh scan without caching the results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Scan depth for project component discovery. DEFAULT: uses server's initial scan depth.
    ///
    /// RANGE: 0-10 levels (0 = only root directory, 3 = reasonable default)
    ///
    /// BEHAVIOR: When specified and different from server's initial scan, performs
    /// a fresh scan without caching the results. Higher depths may take longer
    /// but discover components in deeply nested directory structures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
}

impl ListProjectComponentsTool {
    #[instrument(name = "list_project_components", skip(self, meta_project))]
    pub fn call_tool(&self, meta_project: &MetaProject) -> Result<CallToolResult, CallToolError> {
        // Determine if we need to re-scan based on different parameters
        let requested_path = self.path.as_ref().map(std::path::PathBuf::from);
        let requested_depth = self.depth.unwrap_or(meta_project.scan_depth as u32) as usize;

        let needs_rescan = match &requested_path {
            Some(path) => path != &meta_project.project_root_path,
            None => false,
        } || requested_depth != meta_project.scan_depth;

        let effective_meta_project = if needs_rescan {
            // Perform fresh scan with user-specified parameters
            let scan_root = requested_path
                .as_ref()
                .unwrap_or(&meta_project.project_root_path);

            info!(
                "Re-scanning project: path={}, depth={} (differs from cached scan)",
                scan_root.display(),
                requested_depth
            );

            self.perform_fresh_scan(scan_root, requested_depth)?
        } else {
            // Use cached MetaProject
            info!("Using cached MetaProject scan results");
            meta_project.clone()
        };

        // Get project name from the effective meta project root
        let project_name = effective_meta_project
            .project_root_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Serialize all components directly - let ProjectComponent define the API!
        let components = effective_meta_project.components.clone();

        let content = json!({
            "project_name": project_name,
            "project_root": effective_meta_project.project_root_path,
            "component_count": effective_meta_project.component_count(),
            "provider_types": effective_meta_project.get_provider_types(),
            "scan_depth": effective_meta_project.scan_depth,
            "discovered_at": effective_meta_project.discovered_at,
            "rescanned": needs_rescan,
            "components": components
        });

        info!(
            "Successfully listed {} project components across {} providers: {:?}",
            effective_meta_project.component_count(),
            effective_meta_project.get_provider_types().len(),
            effective_meta_project.get_provider_types()
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(
            serialize_result(&content),
        )]))
    }

    /// Perform a fresh project scan without caching
    fn perform_fresh_scan(
        &self,
        scan_root: &std::path::Path,
        depth: usize,
    ) -> Result<crate::project::MetaProject, CallToolError> {
        use crate::project::ProjectScanner;

        // Create project scanner with default providers
        let scanner = ProjectScanner::with_default_providers();

        // Perform the scan
        scanner.scan_project(scan_root, depth, None).map_err(|e| {
            CallToolError::new(std::io::Error::other(format!(
                "Failed to scan project at {}: {}",
                scan_root.display(),
                e
            )))
        })
    }
}
