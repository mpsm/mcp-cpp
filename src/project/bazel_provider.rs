use crate::project::{ProjectComponent, ProjectComponentProvider, ProjectError};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Bazel project component provider
///
/// This provider detects Bazel workspaces by looking for WORKSPACE files
/// and attempts to find compilation databases in bazel-out directories.
#[allow(dead_code)]
pub struct BazelProvider;

#[allow(dead_code)]
impl BazelProvider {
    /// Create a new Bazel provider
    pub fn new() -> Self {
        Self
    }

    /// Parse WORKSPACE file and extract relevant information
    fn parse_workspace_file(&self, workspace_file: &Path) -> Result<BazelProjectInfo, ProjectError> {
        let content = fs::read_to_string(workspace_file).map_err(ProjectError::Io)?;

        let mut workspace_name = None;
        let mut rules = Vec::new();
        let mut dependencies = Vec::new();

        // Parse workspace file line by line
        for line in content.lines() {
            let line = line.trim();
            
            // Skip comments and empty lines
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            // Extract workspace name
            if line.starts_with("workspace(") && workspace_name.is_none() {
                if let Some(name_start) = line.find("name = \"") {
                    let name_start = name_start + 8; // length of "name = \""
                    if let Some(name_end) = line[name_start..].find('"') {
                        workspace_name = Some(line[name_start..name_start + name_end].to_string());
                    }
                }
            }

            // Track rule loads and dependencies
            if line.starts_with("load(") {
                rules.push(line.to_string());
            } else if line.contains("_repository(") {
                dependencies.push(line.to_string());
            }
        }

        Ok(BazelProjectInfo {
            workspace_name,
            rules,
            dependencies,
        })
    }

    /// Find compilation database in Bazel output directories
    fn find_compilation_database(&self, workspace_root: &Path) -> Option<PathBuf> {
        // Common locations for Bazel compilation databases
        let candidates = [
            "bazel-out/host/bin/compile_commands.json",
            "bazel-out/k8-fastbuild/bin/compile_commands.json", 
            "bazel-out/k8-opt/bin/compile_commands.json",
            "bazel-out/k8-dbg/bin/compile_commands.json",
            "compile_commands.json", // Sometimes generated at root
        ];

        for candidate in &candidates {
            let compile_commands = workspace_root.join(candidate);
            if compile_commands.exists() {
                return Some(compile_commands);
            }
        }

        // Try to find any compile_commands.json in bazel-out directory
        let bazel_out = workspace_root.join("bazel-out");
        if bazel_out.exists() {
            if let Ok(entries) = fs::read_dir(&bazel_out) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        let compile_commands = entry.path().join("bin/compile_commands.json");
                        if compile_commands.exists() {
                            return Some(compile_commands);
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if a directory is a Bazel workspace root
    fn is_bazel_workspace(&self, path: &Path) -> bool {
        path.join("WORKSPACE").exists() || path.join("WORKSPACE.bazel").exists()
    }
}

#[allow(dead_code)]
impl ProjectComponentProvider for BazelProvider {
    fn name(&self) -> &str {
        "bazel"
    }

    fn scan_path(&self, path: &Path) -> Result<Option<ProjectComponent>, ProjectError> {
        // Check if this looks like a Bazel workspace
        if !self.is_bazel_workspace(path) {
            return Ok(None);
        }

        // Determine which WORKSPACE file exists
        let workspace_file = if path.join("WORKSPACE").exists() {
            path.join("WORKSPACE")
        } else {
            path.join("WORKSPACE.bazel")
        };

        // Parse the WORKSPACE file
        let bazel_info = self.parse_workspace_file(&workspace_file)?;

        // For Bazel, the workspace root is both the source root and build root
        let source_root = path.to_path_buf();

        // Try to find compilation database
        let compilation_database_path = self.find_compilation_database(path);

        // If no compilation database found, we can still create a component
        // but it will have limited functionality
        let compilation_database_path = compilation_database_path.unwrap_or_else(|| {
            // Default location where Bazel might put it
            path.join("compile_commands.json")
        });

        // Create build options from Bazel info
        let mut build_options = HashMap::new();
        if let Some(ref workspace_name) = bazel_info.workspace_name {
            build_options.insert("WORKSPACE_NAME".to_string(), workspace_name.clone());
        }
        build_options.insert("RULES_COUNT".to_string(), bazel_info.rules.len().to_string());
        build_options.insert("DEPS_COUNT".to_string(), bazel_info.dependencies.len().to_string());

        // Create project component
        let component = ProjectComponent::new(
            path.to_path_buf(), // build_root = workspace_root for Bazel
            source_root,
            compilation_database_path,
            "bazel".to_string(),
            "Bazel".to_string(), // generator
            "fastbuild".to_string(), // default build type
            build_options,
        )?;

        Ok(Some(component))
    }
}

impl Default for BazelProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal struct to hold parsed Bazel workspace information
#[derive(Debug)]
#[allow(dead_code)]
struct BazelProjectInfo {
    workspace_name: Option<String>,
    rules: Vec<String>,
    dependencies: Vec<String>,
}
