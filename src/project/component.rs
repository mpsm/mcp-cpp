use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Project component representing a build system configuration
///
/// This struct contains all essential information about a project's build configuration,
/// including paths to key directories and files, as well as build-specific options.
/// Providers should populate the structured fields (generator, build_type) from their
/// specific configuration formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectComponent {
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
    pub build_options: HashMap<String, String>,
}

impl ProjectComponent {
    /// Create a new project component with validation
    ///
    /// Returns an error if any of the required paths are not accessible
    pub fn new(
        build_dir_path: PathBuf,
        source_root_path: PathBuf,
        compilation_database_path: PathBuf,
        provider_type: String,
        generator: String,
        build_type: String,
        build_options: HashMap<String, String>,
    ) -> Result<Self, crate::project::ProjectError> {
        use crate::project::ProjectError;

        // Validate build directory
        if !build_dir_path.exists() || !build_dir_path.is_dir() {
            return Err(ProjectError::BuildDirectoryNotReadable {
                path: build_dir_path.to_string_lossy().to_string(),
            });
        }

        // Validate source root
        if !source_root_path.exists() || !source_root_path.is_dir() {
            return Err(ProjectError::SourceRootNotFound {
                path: source_root_path.to_string_lossy().to_string(),
            });
        }

        // Validate compilation database
        if !compilation_database_path.exists() {
            return Err(ProjectError::CompilationDatabaseNotFound {
                path: compilation_database_path.to_string_lossy().to_string(),
            });
        }

        Ok(Self {
            build_dir_path,
            source_root_path,
            compilation_database_path,
            provider_type,
            generator,
            build_type,
            build_options,
        })
    }
}
