//! Project analysis tools for multi-provider build systems

use rmcp::{
    ErrorData,
    model::{CallToolResult, Content},
};
use tracing::{info, instrument};

use super::utils::serialize_result;
use crate::project::ProjectWorkspace;

/// Tool parameters for get_project_details
#[derive(Debug, ::serde::Deserialize, ::serde::Serialize)]
pub struct GetProjectDetailsTool {
    /// Optional project root path to scan. DEFAULT: uses server's cached scan results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Scan depth for project component discovery. DEFAULT: uses server's initial scan depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,

    /// Include detailed build options and configuration variables. DEFAULT: false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_details: Option<bool>,
}

impl GetProjectDetailsTool {
    #[instrument(name = "get_project_details", skip(self, meta_project))]
    pub fn call_tool(&self, meta_project: &ProjectWorkspace) -> Result<CallToolResult, ErrorData> {
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
        let include_details = self.include_details.unwrap_or(false);

        // Create appropriate view based on include_details flag
        let view = if include_details {
            effective_meta_project.get_full_view()
        } else {
            effective_meta_project.get_short_view()
        };

        // Serialize the view
        let mut content = serde_json::to_value(&view).map_err(|e| {
            ErrorData::internal_error(format!("Failed to serialize project view: {}", e), None)
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

        Ok(CallToolResult::success(vec![Content::text(
            serialize_result(&content),
        )]))
    }

    /// Perform a fresh project scan without caching
    fn perform_fresh_scan(
        &self,
        scan_root: &std::path::Path,
        depth: usize,
    ) -> Result<crate::project::ProjectWorkspace, ErrorData> {
        use crate::project::ProjectScanner;

        // Create project scanner with default providers
        let scanner = ProjectScanner::with_default_providers();

        // Perform the scan
        scanner.scan_project(scan_root, depth, None).map_err(|e| {
            ErrorData::internal_error(
                format!("Failed to scan project at {}: {}", scan_root.display(), e),
                None,
            )
        })
    }
}
