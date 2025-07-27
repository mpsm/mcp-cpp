use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::project::ProjectComponent;

/// Meta project representing a workspace with multiple build configurations
///
/// A MetaProject contains the root directory that was scanned and all discovered
/// ProjectComponents within that workspace. This allows managing complex projects
/// that may have multiple build systems or configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaProject {
    /// Root directory that was scanned to discover components
    pub project_root_path: PathBuf,

    /// Collection of discovered project components
    pub components: Vec<ProjectComponent>,

    /// Depth used during the scan that discovered these components
    pub scan_depth: usize,

    /// Timestamp when this meta project was discovered
    pub discovered_at: DateTime<Utc>,
}

#[allow(dead_code)]
impl MetaProject {
    /// Create a new meta project
    pub fn new(
        project_root_path: PathBuf,
        components: Vec<ProjectComponent>,
        scan_depth: usize,
    ) -> Self {
        Self {
            project_root_path,
            components,
            scan_depth,
            discovered_at: Utc::now(),
        }
    }

    /// Get all components grouped by provider type
    pub fn get_components_by_provider(&self) -> HashMap<String, Vec<&ProjectComponent>> {
        let mut grouped = HashMap::new();

        for component in &self.components {
            grouped
                .entry(component.provider_type.clone())
                .or_insert_with(Vec::new)
                .push(component);
        }

        grouped
    }

    /// Get unique source root directories from all components
    pub fn get_source_roots(&self) -> Vec<&PathBuf> {
        let mut roots: Vec<&PathBuf> = self
            .components
            .iter()
            .map(|c| &c.source_root_path)
            .collect();

        // Remove duplicates while preserving order
        roots.sort();
        roots.dedup();
        roots
    }

    /// Get all components for a specific provider type
    pub fn get_components_for_provider(&self, provider_type: &str) -> Vec<&ProjectComponent> {
        self.components
            .iter()
            .filter(|c| c.provider_type == provider_type)
            .collect()
    }

    /// Check if any components use a specific provider type
    pub fn has_provider_type(&self, provider_type: &str) -> bool {
        self.components
            .iter()
            .any(|c| c.provider_type == provider_type)
    }

    /// Get the number of discovered components
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Get all provider types present in this meta project
    pub fn get_provider_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self
            .components
            .iter()
            .map(|c| c.provider_type.clone())
            .collect();

        types.sort();
        types.dedup();
        types
    }

    /// Validate all components in this meta project
    ///
    /// Note: Since ProjectComponent constructor already validates paths,
    /// this mainly serves as a health check for existing components.
    pub fn validate_all(&self) -> Result<(), Vec<crate::project::ProjectError>> {
        let mut errors = Vec::new();

        for (index, component) in self.components.iter().enumerate() {
            // Check if paths still exist (they might have been deleted since discovery)
            if !component.build_dir_path.exists() {
                errors.push(crate::project::ProjectError::BuildDirectoryNotReadable {
                    path: format!(
                        "Component[{}]: {}",
                        index,
                        component.build_dir_path.display()
                    ),
                });
            }

            if !component.source_root_path.exists() {
                errors.push(crate::project::ProjectError::SourceRootNotFound {
                    path: format!(
                        "Component[{}]: {}",
                        index,
                        component.source_root_path.display()
                    ),
                });
            }

            if !component.compilation_database_path.exists() {
                errors.push(crate::project::ProjectError::CompilationDatabaseNotFound {
                    path: format!(
                        "Component[{}]: {}",
                        index,
                        component.compilation_database_path.display()
                    ),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
