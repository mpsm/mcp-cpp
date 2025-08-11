use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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

    /// Get the number of discovered components
    pub fn component_count(&self) -> usize {
        self.components.len()
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
}
