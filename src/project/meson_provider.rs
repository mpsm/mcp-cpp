use crate::project::{ProjectComponent, ProjectComponentProvider, ProjectError};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Meson project component provider
///
/// This provider detects and parses Meson build directories by looking for
/// meson-info directory and extracting build configuration information.
pub struct MesonProvider;

impl MesonProvider {
    /// Create a new Meson provider
    pub fn new() -> Self {
        Self
    }

    /// Parse meson-info/intro-buildoptions.json and extract build options
    fn parse_meson_buildoptions(
        &self,
        meson_info_dir: &Path,
    ) -> Result<HashMap<String, String>, ProjectError> {
        let buildoptions_file = meson_info_dir.join("intro-buildoptions.json");

        if !buildoptions_file.exists() {
            // Return empty options if file doesn't exist
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&buildoptions_file).map_err(ProjectError::Io)?;

        let buildoptions: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| ProjectError::ParseError {
                reason: format!("Failed to parse intro-buildoptions.json: {e}"),
            })?;

        let mut options = HashMap::new();

        // Extract build options from the JSON array
        if let Some(options_array) = buildoptions.as_array() {
            for option in options_array {
                if let (Some(name), Some(value)) = (
                    option.get("name").and_then(|v| v.as_str()),
                    option.get("value").and_then(|v| v.as_str()),
                ) {
                    options.insert(name.to_string(), value.to_string());
                }
            }
        }

        Ok(options)
    }

    /// Parse meson-info/intro-projectinfo.json to get source directory
    fn parse_meson_projectinfo(
        &self,
        meson_info_dir: &Path,
    ) -> Result<Option<PathBuf>, ProjectError> {
        let projectinfo_file = meson_info_dir.join("intro-projectinfo.json");

        if !projectinfo_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&projectinfo_file).map_err(ProjectError::Io)?;

        let projectinfo: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| ProjectError::ParseError {
                reason: format!("Failed to parse intro-projectinfo.json: {e}"),
            })?;

        // Extract source directory from project info
        if let Some(source_dir) = projectinfo.get("source_dir").and_then(|v| v.as_str()) {
            Ok(Some(PathBuf::from(source_dir)))
        } else {
            Ok(None)
        }
    }

    /// Parse meson-info/intro-buildsystem_files.json to get source directory
    ///
    /// This extracts the source directory from the path to meson.build file,
    /// which is more reliable than using parent directory for out-of-source builds.
    fn parse_meson_buildsystem_files(
        &self,
        meson_info_dir: &Path,
    ) -> Result<Option<PathBuf>, ProjectError> {
        let buildsystem_files = meson_info_dir.join("intro-buildsystem_files.json");

        if !buildsystem_files.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&buildsystem_files).map_err(ProjectError::Io)?;

        let files: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| ProjectError::ParseError {
                reason: format!("Failed to parse intro-buildsystem_files.json: {e}"),
            })?;

        // Extract source directory from the first meson.build file path
        if let Some(files_array) = files.as_array() {
            for file_value in files_array {
                if let Some(file_path) = file_value.as_str() {
                    let path = PathBuf::from(file_path);
                    if path.file_name().and_then(|n| n.to_str()) == Some("meson.build")
                        && let Some(parent) = path.parent()
                    {
                        return Ok(Some(parent.to_path_buf()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Find compilation database path in build directory
    fn find_compilation_database(&self, build_dir: &Path) -> Option<PathBuf> {
        let compile_commands = build_dir.join("compile_commands.json");
        if compile_commands.exists() {
            Some(compile_commands)
        } else {
            None
        }
    }

    /// Extract build options from meson-info directory
    fn extract_build_options(
        &self,
        meson_info_dir: &Path,
    ) -> Result<HashMap<String, String>, ProjectError> {
        let mut build_options = self.parse_meson_buildoptions(meson_info_dir)?;

        // Add some standard Meson build information
        build_options.insert("BUILD_SYSTEM".to_string(), "meson".to_string());

        // Try to extract build type from buildoptions if available
        if let Some(buildtype) = build_options.get("buildtype") {
            build_options.insert("BUILD_TYPE".to_string(), buildtype.clone());
        }

        Ok(build_options)
    }
}

impl ProjectComponentProvider for MesonProvider {
    fn scan_path(&self, path: &Path) -> Result<Option<ProjectComponent>, ProjectError> {
        // Check if this looks like a Meson build directory
        let meson_info_dir = path.join("meson-info");

        if !meson_info_dir.exists() || !meson_info_dir.is_dir() {
            // Not a Meson build directory
            return Ok(None);
        }

        // Extract build options from meson-info
        let build_options = self.extract_build_options(&meson_info_dir)?;

        // Determine source root with multiple fallback strategies
        let source_root = if let Some(source_dir) = self.parse_meson_projectinfo(&meson_info_dir)? {
            // First try: source_dir from intro-projectinfo.json
            source_dir
        } else if let Some(source_dir) = self.parse_meson_buildsystem_files(&meson_info_dir)? {
            // Second try: extract source directory from meson.build path in intro-buildsystem_files.json
            source_dir
        } else {
            // Final fallback: assume source is parent of build directory
            path.parent()
                .ok_or_else(|| ProjectError::SourceRootNotFound {
                    path: path.to_string_lossy().to_string(),
                })?
                .to_path_buf()
        };

        // Find compilation database
        let compilation_database_path = self.find_compilation_database(path).ok_or_else(|| {
            ProjectError::CompilationDatabaseNotFound {
                path: path
                    .join("compile_commands.json")
                    .to_string_lossy()
                    .to_string(),
            }
        })?;

        // Extract generator and build type for structured fields
        let generator = build_options
            .get("backend")
            .unwrap_or(&"ninja".to_string())
            .clone();
        let build_type = build_options
            .get("buildtype")
            .unwrap_or(&"debug".to_string())
            .clone();

        // Create project component with validation
        let component = ProjectComponent::new(
            path.to_path_buf(),
            source_root,
            compilation_database_path,
            "meson".to_string(),
            generator,
            build_type,
            build_options,
        )?;

        Ok(Some(component))
    }
}

impl Default for MesonProvider {
    fn default() -> Self {
        Self::new()
    }
}
