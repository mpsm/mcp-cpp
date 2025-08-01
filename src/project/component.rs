use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::project::{CompilationDatabase, CompilationDatabaseError};

/// Project component representing a build system configuration
///
/// This struct contains all essential information about a project's build configuration,
/// including paths to key directories and files, as well as build-specific options.
/// Providers should populate the structured fields (generator, build_type) from their
/// specific configuration formats.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectComponent {
    /// Path to the build directory
    pub build_dir_path: PathBuf,

    /// Path to the source root directory
    pub source_root_path: PathBuf,

    /// Compilation database (compile_commands.json) with parsed entries
    #[serde(rename = "compilation_database_path")]
    pub compilation_database: CompilationDatabase,

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
    /// Returns an error if any of the required paths are not accessible or if the compilation database cannot be loaded
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

        // Create and validate compilation database
        let compilation_database =
            CompilationDatabase::new(compilation_database_path).map_err(|e| match e {
                CompilationDatabaseError::FileNotFound { path } => {
                    ProjectError::CompilationDatabaseNotFound { path }
                }
                CompilationDatabaseError::ReadError { error } => {
                    ProjectError::CompilationDatabaseNotReadable { error }
                }
                CompilationDatabaseError::ParseError { error } => {
                    ProjectError::CompilationDatabaseInvalid { error }
                }
                CompilationDatabaseError::EmptyDatabase => ProjectError::CompilationDatabaseEmpty,
            })?;

        Ok(Self {
            build_dir_path,
            source_root_path,
            compilation_database,
            provider_type,
            generator,
            build_type,
            build_options,
        })
    }
}
