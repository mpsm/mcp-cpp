use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::project::{CompilationDatabase, ProjectComponent};

/// View of a project component with optional build options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectComponentView {
    /// Path to the build directory
    pub build_dir_path: PathBuf,

    /// Path to the source root directory
    pub source_root_path: PathBuf,

    /// Path to the compilation database (compile_commands.json)
    pub compilation_database_path: PathBuf,

    /// Build system provider type (e.g., "cmake", "meson")
    pub provider_type: String,

    /// Generator used by the build system (e.g., "Ninja", "Unix Makefiles", "Visual Studio 16 2019")
    pub generator: String,

    /// Build configuration type (e.g., "Debug", "Release", "RelWithDebInfo", "MinSizeRel")
    pub build_type: String,

    /// Raw build options and configuration (provider-specific key-value pairs)
    /// In short view, this contains a summary instead of full options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_options: Option<HashMap<String, String>>,

    /// Count of build options (present in short view when build_options is None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_options_count: Option<usize>,
}

/// View of a project workspace with optional detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectWorkspaceView {
    /// Root directory that was scanned to discover components
    pub project_root_path: PathBuf,

    /// Collection of discovered project components (as views)
    pub components: Vec<ProjectComponentView>,

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

    /// Add a new component to this workspace
    ///
    /// This method is used for dynamic component discovery when a build directory
    /// is requested that wasn't found during initial workspace scanning.
    ///
    /// # Arguments
    /// * `component` - The project component to add to the workspace
    pub fn add_component(&mut self, component: ProjectComponent) {
        // Verify we don't already have this component (by build directory)
        if self
            .get_component_by_build_dir(&component.build_dir_path)
            .is_none()
        {
            self.components.push(component);
        }
    }

    /// Get a short view of the workspace without detailed build options
    ///
    /// This method creates a view that includes essential information but excludes
    /// verbose build_options to prevent context window exhaustion. Instead, it
    /// provides a count of build options for each component.
    pub fn get_short_view(&self) -> ProjectWorkspaceView {
        let component_views: Vec<ProjectComponentView> = self
            .components
            .iter()
            .map(|component| ProjectComponentView {
                build_dir_path: component.build_dir_path.clone(),
                source_root_path: component.source_root_path.clone(),
                compilation_database_path: component.compilation_database_path.clone(),
                provider_type: component.provider_type.clone(),
                generator: component.generator.clone(),
                build_type: component.build_type.clone(),
                build_options: None, // Excluded in short view
                build_options_count: Some(component.build_options.len()),
            })
            .collect();

        ProjectWorkspaceView {
            project_root_path: self.project_root_path.clone(),
            components: component_views,
            scan_depth: self.scan_depth,
            discovered_at: self.discovered_at,
            global_compilation_database: self.global_compilation_database.clone(),
        }
    }

    /// Get a full view of the workspace with all build options included
    ///
    /// This method creates a complete view that includes all build_options
    /// for debugging and detailed analysis purposes.
    pub fn get_full_view(&self) -> ProjectWorkspaceView {
        let component_views: Vec<ProjectComponentView> = self
            .components
            .iter()
            .map(|component| ProjectComponentView {
                build_dir_path: component.build_dir_path.clone(),
                source_root_path: component.source_root_path.clone(),
                compilation_database_path: component.compilation_database_path.clone(),
                provider_type: component.provider_type.clone(),
                generator: component.generator.clone(),
                build_type: component.build_type.clone(),
                build_options: Some(component.build_options.clone()), // Included in full view
                build_options_count: Some(component.build_options.len()),
            })
            .collect();

        ProjectWorkspaceView {
            project_root_path: self.project_root_path.clone(),
            components: component_views,
            scan_depth: self.scan_depth,
            discovered_at: self.discovered_at,
            global_compilation_database: self.global_compilation_database.clone(),
        }
    }
}
