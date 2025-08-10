//! Project analysis tools for multi-provider build systems

use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::{CallToolResult, TextContent, schema_utils::CallToolError};
use tracing::{info, instrument};

use super::utils::serialize_result;
use crate::project::ProjectWorkspace;

#[mcp_tool(
    name = "get_project_details",
    description = "Get comprehensive project analysis including build configurations, components, \
                   and global compilation database information. Provides complete workspace \
                   intelligence for multi-provider build systems.

                   🏗️ PROJECT OVERVIEW:
                   • Project name and root directory information
                   • Global compilation database path (if configured)
                   • Component count and provider type summary
                   • Discovery timestamp and scan configuration

                   🔧 MULTI-PROVIDER DISCOVERY:
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
                   • Global compilation database path (overrides component-specific databases)
                   • Per-component compile_commands.json availability and validity
                   • LSP server compatibility assessment across all build systems

                   🎯 PROJECT STRUCTURE DETAILS:
                   Each discovered component includes:
                   • Build directory path and source root location
                   • Provider type (cmake, meson, etc.) for build system identification
                   • Generator and build type information in standardized format
                   • Complete build options and configuration details
                   • Compilation database status for LSP integration

                   🎯 PRIMARY USE CASES:
                   Project assessment • Build system inventory • LSP setup validation
                   • Development environment verification • CI/CD build matrix generation
                   • Global compilation database configuration analysis

                   INPUT REQUIREMENTS:
                   • No parameters required - returns complete project analysis
                   • Uses ProjectWorkspace scanning results
                   • Returns all discovered components and global configuration"
)]
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize, JsonSchema)]
pub struct GetProjectDetailsTool {
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

impl GetProjectDetailsTool {
    #[instrument(name = "get_project_details", skip(self, meta_project))]
    pub fn call_tool(
        &self,
        meta_project: &ProjectWorkspace,
    ) -> Result<CallToolResult, CallToolError> {
        // Determine if we need to re-scan based on different parameters
        let requested_path = self.path.as_ref().map(std::path::PathBuf::from);
        let requested_depth = self.depth.unwrap_or(meta_project.scan_depth as u32) as usize;

        let needs_rescan = match &requested_path {
            Some(path) => path != &meta_project.project_root_path,
            None => false,
        } || requested_depth != meta_project.scan_depth;

        let fresh_scan = if needs_rescan {
            // Perform fresh scan with user-specified parameters
            let scan_root = requested_path
                .as_ref()
                .unwrap_or(&meta_project.project_root_path);

            info!(
                "Re-scanning project: path={}, depth={} (differs from cached scan)",
                scan_root.display(),
                requested_depth
            );

            Some(self.perform_fresh_scan(scan_root, requested_depth)?)
        } else {
            // Use cached ProjectWorkspace
            info!("Using cached ProjectWorkspace scan results");
            None
        };

        let effective_meta_project = fresh_scan.as_ref().unwrap_or(meta_project);
        let rescanned_fresh = fresh_scan.is_some();

        // Serialize ProjectWorkspace directly
        let mut content = serde_json::to_value(effective_meta_project).map_err(|e| {
            CallToolError::new(std::io::Error::other(format!(
                "Failed to serialize project details: {e}"
            )))
        })?;

        // Add the rescanned flag which isn't part of the core ProjectWorkspace
        if let Some(obj) = content.as_object_mut() {
            obj.insert(
                "rescanned".to_string(),
                serde_json::Value::Bool(rescanned_fresh),
            );
        }

        info!(
            "Successfully analyzed project details: {} components across {} providers: {:?}",
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
    ) -> Result<crate::project::ProjectWorkspace, CallToolError> {
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
