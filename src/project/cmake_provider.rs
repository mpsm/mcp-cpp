use crate::project::{ProjectComponent, ProjectComponentProvider, ProjectError};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// CMake project component provider
///
/// This provider detects and parses CMake build directories by looking for
/// CMakeCache.txt files and extracting build configuration information.
#[allow(dead_code)]
pub struct CmakeProvider;

#[allow(dead_code)]
impl CmakeProvider {
    /// Create a new CMake provider
    pub fn new() -> Self {
        Self
    }

    /// Parse CMakeCache.txt and extract relevant information
    fn parse_cmake_cache(&self, cache_file: &Path) -> Result<CmakeProjectInfo, ProjectError> {
        let content = fs::read_to_string(cache_file).map_err(ProjectError::Io)?;

        let mut generator = None;
        let mut build_type = None;
        let mut build_options = HashMap::new();
        let mut source_dir = None;
        let mut project_name = None;

        // Parse cache file line by line
        for line in content.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            if let Some(eq_pos) = line.find('=') {
                let (key_part, value) = line.split_at(eq_pos);
                let value = &value[1..]; // Skip the '=' character

                // Parse key:type format
                let key = if let Some(colon_pos) = key_part.find(':') {
                    &key_part[..colon_pos]
                } else {
                    key_part
                };

                match key {
                    "CMAKE_GENERATOR" => generator = Some(value.to_string()),
                    "CMAKE_BUILD_TYPE" => build_type = Some(value.to_string()),
                    "CMAKE_SOURCE_DIR" => source_dir = Some(PathBuf::from(value)),
                    "CMAKE_PROJECT_NAME" => project_name = Some(value.to_string()),
                    _ if key.starts_with("CMAKE_") => {
                        // Store CMAKE_ variables as build options
                        build_options.insert(key.to_string(), value.to_string());
                    }
                    _ => {
                        // Store user-defined options
                        build_options.insert(key.to_string(), value.to_string());
                    }
                }
            }
        }

        // If CMAKE_SOURCE_DIR not found, try project-specific SOURCE_DIR
        if source_dir.is_none() && project_name.is_some() {
            let project_source_dir_key = format!("{}_SOURCE_DIR", project_name.as_ref().unwrap());

            for line in content.lines() {
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }

                if let Some(eq_pos) = line.find('=') {
                    let (key_part, value) = line.split_at(eq_pos);
                    let value = &value[1..];

                    let key = if let Some(colon_pos) = key_part.find(':') {
                        &key_part[..colon_pos]
                    } else {
                        key_part
                    };

                    if key == project_source_dir_key {
                        source_dir = Some(PathBuf::from(value));
                        break;
                    }
                }
            }
        }

        // Add generator and build type to build options if they exist
        if let Some(ref generator_val) = generator {
            build_options.insert("CMAKE_GENERATOR".to_string(), generator_val.clone());
        }
        if let Some(ref bt) = build_type {
            build_options.insert("CMAKE_BUILD_TYPE".to_string(), bt.clone());
        }

        Ok(CmakeProjectInfo {
            generator,
            build_type,
            build_options,
            source_dir,
        })
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
}

#[allow(dead_code)]
impl ProjectComponentProvider for CmakeProvider {
    fn name(&self) -> &str {
        "cmake"
    }

    fn scan_path(&self, path: &Path) -> Result<Option<ProjectComponent>, ProjectError> {
        // Check if this looks like a CMake build directory
        let cmake_cache = path.join("CMakeCache.txt");

        if !cmake_cache.exists() {
            // Not a CMake build directory
            return Ok(None);
        }

        // Parse the CMake cache
        let cmake_info = self.parse_cmake_cache(&cmake_cache)?;

        // Determine source root
        let source_root = if let Some(source_dir) = cmake_info.source_dir {
            source_dir
        } else {
            // Fallback: assume source is parent of build directory
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

        // Create project component with validation
        let component = ProjectComponent::new(
            path.to_path_buf(),
            source_root,
            compilation_database_path,
            "cmake".to_string(),
            cmake_info
                .generator
                .unwrap_or_else(|| "Unknown".to_string()),
            cmake_info
                .build_type
                .unwrap_or_else(|| "Unknown".to_string()),
            cmake_info.build_options,
        )?;

        Ok(Some(component))
    }
}

impl Default for CmakeProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal struct to hold parsed CMake information
#[derive(Debug)]
#[allow(dead_code)]
struct CmakeProjectInfo {
    generator: Option<String>,
    build_type: Option<String>,
    build_options: HashMap<String, String>,
    source_dir: Option<PathBuf>,
}
