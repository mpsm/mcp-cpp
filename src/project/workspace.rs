use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::project::{CompilationDatabase, ProjectComponent};

/// Project workspace representing a workspace with multiple build configurations
///
/// A ProjectWorkspace contains the root directory that was scanned and all discovered
/// ProjectComponents within that workspace. This allows managing complex projects
/// that may have multiple build systems or configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectWorkspace {
    /// Root directory that was scanned to discover components
    pub project_root_path: PathBuf,

    /// Collection of discovered project components
    pub components: Vec<ProjectComponent>,

    /// Depth used during the scan that discovered these components
    pub scan_depth: usize,

    /// Timestamp when this project workspace was discovered
    pub discovered_at: DateTime<Utc>,

    /// Optional global compilation database that overrides component-specific databases
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "global_compilation_database_path"
    )]
    pub global_compilation_database: Option<CompilationDatabase>,
}

#[allow(dead_code)]
impl ProjectWorkspace {
    /// Create a new project workspace
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
            global_compilation_database: None,
        }
    }

    /// Create a new project workspace with optional global compilation database
    pub fn with_global_compilation_database(
        project_root_path: PathBuf,
        components: Vec<ProjectComponent>,
        scan_depth: usize,
        global_compilation_database: Option<CompilationDatabase>,
    ) -> Self {
        Self {
            project_root_path,
            components,
            scan_depth,
            discovered_at: Utc::now(),
            global_compilation_database,
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

    /// Get a component by its build directory path
    pub fn get_component_by_build_dir(&self, build_dir: &PathBuf) -> Option<&ProjectComponent> {
        self.components
            .iter()
            .find(|c| c.build_dir_path == *build_dir)
    }

    /// Get all build directories from components
    pub fn get_build_dirs(&self) -> Vec<PathBuf> {
        self.components
            .iter()
            .map(|c| c.build_dir_path.clone())
            .collect()
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

    /// Get the project name derived from the root directory
    pub fn project_name(&self) -> String {
        self.project_root_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Get all provider types present in this project workspace
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

    /// Validate all components in this project workspace
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

            if !component.compilation_database.path().exists() {
                errors.push(crate::project::ProjectError::CompilationDatabaseNotFound {
                    path: format!(
                        "Component[{}]: {}",
                        index,
                        component.compilation_database.path().display()
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
