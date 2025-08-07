use std::path::Path;
use walkdir::WalkDir;

use crate::project::{MetaProject, ProjectError, ProjectProviderRegistry};

/// Options for configuring project scanning behavior
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Skip hidden directories (starting with '.')
    pub skip_hidden: bool,

    /// Follow symbolic links during traversal
    pub follow_symlinks: bool,

    /// Maximum number of components to discover (None = unlimited)
    pub max_components: Option<usize>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            skip_hidden: true,
            follow_symlinks: false,
            max_components: None,
        }
    }
}

/// Project scanner for discovering multiple build configurations in a workspace
///
/// The scanner uses a provider registry to detect different build systems
/// and creates a MetaProject containing all discovered components.
#[allow(dead_code)]
pub struct ProjectScanner {
    provider_registry: ProjectProviderRegistry,
}

#[allow(dead_code)]
impl ProjectScanner {
    /// Create a new project scanner with the given provider registry
    pub fn new(provider_registry: ProjectProviderRegistry) -> Self {
        Self { provider_registry }
    }

    /// Create a scanner with default providers (CMake and Meson)
    pub fn with_default_providers() -> Self {
        use crate::project::{CmakeProvider, MesonProvider};

        let registry = ProjectProviderRegistry::new()
            .with_provider(Box::new(CmakeProvider::new()))
            .with_provider(Box::new(MesonProvider::new()));

        Self::new(registry)
    }

    /// Scan a directory tree for project components
    ///
    /// # Arguments
    /// * `root_path` - Root directory to start scanning from
    /// * `depth` - Maximum depth to traverse (0 = only root, 1 = root + immediate children, etc.)
    /// * `options` - Optional scanning configuration
    ///
    /// # Returns
    /// A MetaProject containing all discovered components
    pub fn scan_project(
        &self,
        root_path: &Path,
        depth: usize,
        options: Option<ScanOptions>,
    ) -> Result<MetaProject, ProjectError> {
        let options = options.unwrap_or_default();

        // Validate root path
        if !root_path.exists() {
            return Err(ProjectError::PathNotFound {
                path: root_path.to_string_lossy().to_string(),
            });
        }

        if !root_path.is_dir() {
            return Err(ProjectError::InvalidBuildDirectory {
                reason: format!("Root path is not a directory: {}", root_path.display()),
            });
        }

        let mut components = Vec::new();
        let mut scanned_paths = std::collections::HashSet::new();

        // Configure walkdir based on options
        let mut walk_builder = WalkDir::new(root_path).max_depth(depth + 1); // +1 because walkdir counts root as depth 0

        if options.follow_symlinks {
            walk_builder = walk_builder.follow_links(true);
        }

        // Traverse directory tree
        for entry in walk_builder.into_iter() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    // Log the error but continue scanning
                    tracing::warn!("Failed to access directory entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();

            // Skip if not a directory
            if !path.is_dir() {
                continue;
            }

            // Skip hidden directories if configured
            if options.skip_hidden
                && let Some(file_name) = path.file_name()
                && file_name.to_string_lossy().starts_with('.')
            {
                continue;
            }

            // Skip if we've already scanned this path (can happen with symlinks)
            if !scanned_paths.insert(path.to_path_buf()) {
                continue;
            }

            // Try to discover a project component in this directory
            match self.provider_registry.scan_directory(path) {
                Ok(Some(component)) => {
                    components.push(component);

                    // Check if we've hit the component limit
                    if let Some(max) = options.max_components
                        && components.len() >= max
                    {
                        break;
                    }
                }
                Ok(None) => {
                    // No component found in this directory, continue
                }
                Err(e) => {
                    // Log the error but continue scanning other directories
                    tracing::warn!("Error scanning directory {}: {}", path.display(), e);
                }
            }
        }

        Ok(MetaProject::new(root_path.to_path_buf(), components, depth))
    }

    /// Get the names of all registered providers
    pub fn provider_names(&self) -> Vec<&str> {
        self.provider_registry.provider_names()
    }

    /// Get the number of registered providers
    pub fn provider_count(&self) -> usize {
        self.provider_registry.provider_count()
    }
}

impl Default for ProjectScanner {
    fn default() -> Self {
        Self::with_default_providers()
    }
}
