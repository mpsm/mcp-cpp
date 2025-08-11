use std::path::Path;
use walkdir::WalkDir;

use crate::project::{ProjectError, ProjectProviderRegistry, ProjectWorkspace};

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
/// and creates a ProjectWorkspace containing all discovered components.
pub struct ProjectScanner {
    provider_registry: ProjectProviderRegistry,
}

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
    /// A ProjectWorkspace containing all discovered components
    pub fn scan_project(
        &self,
        root_path: &Path,
        depth: usize,
        options: Option<ScanOptions>,
    ) -> Result<ProjectWorkspace, ProjectError> {
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

        Ok(ProjectWorkspace::new(
            root_path.to_path_buf(),
            components,
            depth,
        ))
    }
}

impl Default for ProjectScanner {
    fn default() -> Self {
        Self::with_default_providers()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "project-integration-tests")]
    use crate::test_utils::integration::TestWorkspace;

    // Auto-initialize logging for all tests in this module
    #[cfg(feature = "test-logging")]
    #[ctor::ctor]
    fn init_test_logging() {
        crate::test_utils::logging::init();
    }

    #[tokio::test]
    #[cfg(feature = "project-integration-tests")]
    async fn test_cmake_project_scanning() {
        // Create a workspace with multiple CMake projects
        let workspace = TestWorkspace::new().unwrap();

        // Create main project at root level
        let main_project = workspace
            .create_cmake_project("main_project")
            .await
            .unwrap();
        main_project.configure().await.unwrap();

        // Create a subproject in a subdirectory
        let sub_project = workspace
            .create_cmake_project("libs/sub_project")
            .await
            .unwrap();
        sub_project.configure().await.unwrap();

        let scanner = super::ProjectScanner::with_default_providers();

        // With max_depth(depth + 1) behavior:
        // Depth 0 => max_depth(1): workspace root only
        let meta_project = scanner.scan_project(workspace.path(), 0, None).unwrap();
        assert_eq!(
            meta_project.components.len(),
            0,
            "Depth 0 should find no components"
        );

        // Depth 1 => max_depth(2): workspace root + main_project, libs + their immediate children
        // This reaches main_project/build-debug
        let meta_project = scanner.scan_project(workspace.path(), 1, None).unwrap();
        assert_eq!(
            meta_project.components.len(),
            1,
            "Depth 1 should find main_project's build"
        );

        // Depth 2 => max_depth(3): Reaches both main_project/build-debug and libs/sub_project/build-debug
        let meta_project = scanner.scan_project(workspace.path(), 2, None).unwrap();
        assert_eq!(
            meta_project.components.len(),
            2,
            "Depth 2 should find both projects' builds"
        );

        // Verify both projects were found
        let build_paths: Vec<_> = meta_project
            .components
            .iter()
            .map(|c| {
                c.build_dir_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert!(
            build_paths.iter().all(|p| p == "build-debug"),
            "All builds should be in build-debug directories"
        );
    }

    #[tokio::test]
    #[cfg(feature = "project-integration-tests")]
    async fn test_mixed_cmake_meson_scanning() {
        // Create a workspace with both CMake and Meson projects
        let workspace = TestWorkspace::new().unwrap();

        // Create a CMake project
        let cmake_project = workspace.create_cmake_project("cmake_app").await.unwrap();
        cmake_project.configure().await.unwrap();

        // Create a Meson project
        let meson_project = workspace.create_meson_project("meson_lib").await.unwrap();
        meson_project.configure().await.unwrap();

        // Create nested projects
        let nested_cmake = workspace
            .create_cmake_project("libs/cmake_nested")
            .await
            .unwrap();
        nested_cmake.configure().await.unwrap();

        let nested_meson = workspace
            .create_meson_project("tools/meson_tool")
            .await
            .unwrap();
        nested_meson.configure().await.unwrap();

        let scanner = super::ProjectScanner::with_default_providers();

        // Scan the entire workspace
        let meta_project = scanner.scan_project(workspace.path(), 3, None).unwrap();

        // Should find all 4 projects
        assert_eq!(
            meta_project.components.len(),
            4,
            "Should find all 4 projects (2 CMake, 2 Meson)"
        );

        // Verify we have both CMake and Meson projects
        let cmake_count = meta_project
            .components
            .iter()
            .filter(|c| c.provider_type == "cmake")
            .count();
        let meson_count = meta_project
            .components
            .iter()
            .filter(|c| c.provider_type == "meson")
            .count();

        assert_eq!(cmake_count, 2, "Should find 2 CMake projects");
        assert_eq!(meson_count, 2, "Should find 2 Meson projects");
    }

    #[tokio::test]
    #[cfg(feature = "project-integration-tests")]
    async fn test_multi_provider_scanner_functionality() {
        // Test that the scanner properly uses multiple providers
        let scanner = super::ProjectScanner::with_default_providers();

        // Create a workspace with different project types in the same directory level
        let workspace = TestWorkspace::new().unwrap();

        // Create projects at the same depth to test provider differentiation
        let cmake_project = workspace.create_cmake_project("project_a").await.unwrap();
        cmake_project.configure().await.unwrap();

        let meson_project = workspace.create_meson_project("project_b").await.unwrap();
        meson_project.configure().await.unwrap();

        // Another CMake project to ensure provider can handle multiple instances
        let cmake_project2 = workspace.create_cmake_project("project_c").await.unwrap();
        cmake_project2.configure().await.unwrap();

        // Scan at depth that should find all projects
        let meta_project = scanner.scan_project(workspace.path(), 2, None).unwrap();

        // Should find all 3 projects
        assert_eq!(
            meta_project.components.len(),
            3,
            "Should find all 3 projects"
        );

        // Verify provider types are correctly assigned
        let mut provider_types: Vec<String> = meta_project
            .components
            .iter()
            .map(|c| c.provider_type.clone())
            .collect();
        provider_types.sort();

        let expected_types = vec![
            "cmake".to_string(),
            "cmake".to_string(),
            "meson".to_string(),
        ];
        assert_eq!(
            provider_types, expected_types,
            "Should have correct provider types"
        );

        // Verify each provider correctly identified its projects
        let cmake_projects: Vec<_> = meta_project
            .components
            .iter()
            .filter(|c| c.provider_type == "cmake")
            .collect();
        let meson_projects: Vec<_> = meta_project
            .components
            .iter()
            .filter(|c| c.provider_type == "meson")
            .collect();

        assert_eq!(cmake_projects.len(), 2, "Should find 2 CMake projects");
        assert_eq!(meson_projects.len(), 1, "Should find 1 Meson project");

        // Verify CMake projects have CMake-specific properties
        for cmake_project in cmake_projects {
            assert!(
                cmake_project.build_dir_path.ends_with("build-debug"),
                "CMake projects should use build-debug directory"
            );
            assert_eq!(cmake_project.provider_type, "cmake");
        }

        // Verify Meson project has Meson-specific properties
        for meson_project in meson_projects {
            assert!(
                meson_project.build_dir_path.ends_with("builddir"),
                "Meson projects should use builddir directory"
            );
            assert_eq!(meson_project.provider_type, "meson");
        }
    }

    #[tokio::test]
    #[cfg(feature = "project-integration-tests")]
    async fn test_provider_registry_isolation() {
        // Test that each provider only detects its own project types
        use crate::project::{CmakeProvider, MesonProvider, ProjectProviderRegistry};

        let workspace = TestWorkspace::new().unwrap();

        // Create one of each project type
        let cmake_project = workspace.create_cmake_project("cmake_only").await.unwrap();
        cmake_project.configure().await.unwrap();

        let meson_project = workspace.create_meson_project("meson_only").await.unwrap();
        meson_project.configure().await.unwrap();

        // Test CMake provider in isolation
        let cmake_only_registry =
            ProjectProviderRegistry::new().with_provider(Box::new(CmakeProvider::new()));
        let cmake_scanner = super::ProjectScanner::new(cmake_only_registry);

        let cmake_results = cmake_scanner
            .scan_project(workspace.path(), 2, None)
            .unwrap();
        assert_eq!(
            cmake_results.components.len(),
            1,
            "CMake provider should find only CMake project"
        );
        assert_eq!(cmake_results.components[0].provider_type, "cmake");

        // Test Meson provider in isolation
        let meson_only_registry =
            ProjectProviderRegistry::new().with_provider(Box::new(MesonProvider::new()));
        let meson_scanner = super::ProjectScanner::new(meson_only_registry);

        let meson_results = meson_scanner
            .scan_project(workspace.path(), 2, None)
            .unwrap();
        assert_eq!(
            meson_results.components.len(),
            1,
            "Meson provider should find only Meson project"
        );
        assert_eq!(meson_results.components[0].provider_type, "meson");

        // Test combined providers find both
        let combined_scanner = super::ProjectScanner::with_default_providers();
        let combined_results = combined_scanner
            .scan_project(workspace.path(), 2, None)
            .unwrap();
        assert_eq!(
            combined_results.components.len(),
            2,
            "Combined providers should find both projects"
        );
    }

    #[tokio::test]
    #[cfg(feature = "project-integration-tests")]
    async fn test_find_component_by_build_dir() {
        // Test finding components by their build directory paths
        let workspace = TestWorkspace::new().unwrap();

        // Create projects with known names and build directories
        let cmake_app = workspace.create_cmake_project("cmake_app").await.unwrap();
        cmake_app.configure().await.unwrap();

        let meson_lib = workspace.create_meson_project("meson_lib").await.unwrap();
        meson_lib.configure().await.unwrap();

        let nested_cmake = workspace
            .create_cmake_project("tools/nested_cmake")
            .await
            .unwrap();
        nested_cmake.configure().await.unwrap();

        let scanner = super::ProjectScanner::with_default_providers();
        let meta_project = scanner.scan_project(workspace.path(), 3, None).unwrap();

        // Should find all 3 projects
        assert_eq!(
            meta_project.components.len(),
            3,
            "Should find all 3 projects"
        );

        // Test finding by exact build directory path
        let cmake_app_build = workspace.path().join("cmake_app/build-debug");
        let meson_lib_build = workspace.path().join("meson_lib/builddir");
        let nested_cmake_build = workspace.path().join("tools/nested_cmake/build-debug");

        // Find components by their build directory paths
        let cmake_component = meta_project
            .components
            .iter()
            .find(|c| c.build_dir_path == cmake_app_build)
            .expect("Should find CMake component by build dir");

        let meson_component = meta_project
            .components
            .iter()
            .find(|c| c.build_dir_path == meson_lib_build)
            .expect("Should find Meson component by build dir");

        let nested_component = meta_project
            .components
            .iter()
            .find(|c| c.build_dir_path == nested_cmake_build)
            .expect("Should find nested CMake component by build dir");

        // Verify component properties
        assert_eq!(cmake_component.provider_type, "cmake");
        assert!(cmake_component.build_dir_path.ends_with("build-debug"));
        assert!(cmake_component.source_root_path.ends_with("cmake_app"));

        assert_eq!(meson_component.provider_type, "meson");
        assert!(meson_component.build_dir_path.ends_with("builddir"));
        assert!(meson_component.source_root_path.ends_with("meson_lib"));

        assert_eq!(nested_component.provider_type, "cmake");
        assert!(nested_component.build_dir_path.ends_with("build-debug"));
        assert!(nested_component.source_root_path.ends_with("nested_cmake"));

        // Test finding by partial path matching
        let cmake_matches: Vec<_> = meta_project
            .components
            .iter()
            .filter(|c| c.build_dir_path.to_string_lossy().contains("build-debug"))
            .collect();

        let meson_matches: Vec<_> = meta_project
            .components
            .iter()
            .filter(|c| c.build_dir_path.to_string_lossy().contains("builddir"))
            .collect();

        assert_eq!(
            cmake_matches.len(),
            2,
            "Should find 2 CMake projects with build-debug"
        );
        assert_eq!(
            meson_matches.len(),
            1,
            "Should find 1 Meson project with builddir"
        );

        // Test finding by source root path
        let source_root_matches: Vec<_> = meta_project
            .components
            .iter()
            .filter(|c| c.source_root_path.to_string_lossy().contains("tools"))
            .collect();

        assert_eq!(
            source_root_matches.len(),
            1,
            "Should find 1 project in tools directory"
        );
        assert_eq!(source_root_matches[0].provider_type, "cmake");

        // Test non-existent build directory
        let non_existent_build = workspace.path().join("non_existent/build");
        let not_found = meta_project
            .components
            .iter()
            .find(|c| c.build_dir_path == non_existent_build);

        assert!(
            not_found.is_none(),
            "Should not find component for non-existent build dir"
        );
    }
}
