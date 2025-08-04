use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, warn};
use walkdir::WalkDir;

type WorkspaceParseResult = (
    Option<String>,      // workspace_name
    Vec<String>,         // rules
    Vec<String>,         // dependencies
    Option<PathBuf>,     // workspace_root
);

#[derive(Debug, Error)]
pub enum BazelError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not a Bazel project: no WORKSPACE or WORKSPACE.bazel found")]
    NotBazelProject,

    #[error("WORKSPACE file is corrupted or unreadable: {0}")]
    CorruptedWorkspace(String),

    #[error("Multiple issues detected: {0:?}")]
    MultipleIssues(Vec<String>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildOutput {
    pub path: PathBuf,
    pub relative_path: String,
    pub config: Option<String>,
    pub cpu: Option<String>,
    pub compilation_mode: Option<String>,
    pub has_compile_commands: bool,
    pub symlink_target: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BazelProjectStatus {
    pub is_bazel_project: bool,
    pub workspace_root: PathBuf,
    pub workspace_name: Option<String>,
    pub build_outputs: Vec<BuildOutput>,
    pub rules: Vec<String>,
    pub dependencies: Vec<String>,
    pub issues: Vec<String>,
}

impl BazelProjectStatus {
    pub fn analyze_current_directory() -> Result<Self, BazelError> {
        Self::analyze_directory(&std::env::current_dir()?)
    }

    pub fn analyze_directory(project_path: &Path) -> Result<Self, BazelError> {
        debug!("Analyzing directory: {:?}", project_path);

        let mut status = BazelProjectStatus {
            is_bazel_project: false,
            workspace_root: project_path.to_path_buf(),
            workspace_name: None,
            build_outputs: Vec::new(),
            rules: Vec::new(),
            dependencies: Vec::new(),
            issues: Vec::new(),
        };

        // Check if this is a Bazel project
        let workspace_file = if project_path.join("WORKSPACE").exists() {
            project_path.join("WORKSPACE")
        } else if project_path.join("WORKSPACE.bazel").exists() {
            project_path.join("WORKSPACE.bazel")
        } else {
            return Err(BazelError::NotBazelProject);
        };

        status.is_bazel_project = true;
        debug!("Found WORKSPACE file, this is a Bazel project");

        // Parse WORKSPACE file
        match Self::parse_workspace_file(&workspace_file) {
            Ok((workspace_name, rules, dependencies, _)) => {
                status.workspace_name = workspace_name;
                status.rules = rules;
                status.dependencies = dependencies;
            }
            Err(e) => {
                let issue = format!("Failed to parse WORKSPACE file: {e}");
                warn!("{}", issue);
                status.issues.push(issue);
            }
        }

        // Scan for build outputs
        status.build_outputs = Self::scan_build_outputs(project_path, &mut status.issues)?;

        if !status.issues.is_empty() && status.build_outputs.is_empty() {
            return Err(BazelError::MultipleIssues(status.issues.clone()));
        }

        Ok(status)
    }

    fn scan_build_outputs(
        workspace_root: &Path,
        issues: &mut Vec<String>,
    ) -> Result<Vec<BuildOutput>, BazelError> {
        let mut build_outputs = Vec::new();

        // Look for bazel-out directory and its subdirectories
        let bazel_out = workspace_root.join("bazel-out");
        if bazel_out.exists() {
            debug!("Found bazel-out directory: {:?}", bazel_out);
            
            // Scan for configuration directories (e.g., k8-fastbuild, host, etc.)
            if let Ok(entries) = fs::read_dir(&bazel_out) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        let config_path = entry.path();
                        let config_name = config_path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        match Self::analyze_build_output(workspace_root, &config_path, &config_name) {
                            Ok(build_output) => build_outputs.push(build_output),
                            Err(e) => {
                                let issue = format!("Build output {config_path:?}: {e}");
                                warn!("{}", issue);
                                issues.push(issue);
                            }
                        }
                    }
                }
            }
        }

        // Also check for convenience symlinks (bazel-bin, bazel-genfiles, etc.)
        let symlinks = ["bazel-bin", "bazel-genfiles", "bazel-testlogs"];
        for symlink_name in &symlinks {
            let symlink_path = workspace_root.join(symlink_name);
            if symlink_path.exists() {
                match Self::analyze_symlink(workspace_root, &symlink_path, symlink_name) {
                    Ok(Some(build_output)) => build_outputs.push(build_output),
                    Ok(None) => {} // Not an error, just no useful info
                    Err(e) => {
                        let issue = format!("Symlink {symlink_path:?}: {e}");
                        warn!("{}", issue);
                        issues.push(issue);
                    }
                }
            }
        }

        // Sort by path for consistent output
        build_outputs.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(build_outputs)
    }

    fn analyze_build_output(
        workspace_root: &Path,
        config_path: &Path,
        config_name: &str,
    ) -> Result<BuildOutput, BazelError> {
        let relative_path = config_path
            .strip_prefix(workspace_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| config_path.to_string_lossy().to_string());

        // Parse config name (e.g., "k8-fastbuild" -> cpu="k8", compilation_mode="fastbuild")
        let (cpu, compilation_mode) = Self::parse_config_name(config_name);

        // Check for compile_commands.json in bin subdirectory
        let compile_commands = config_path.join("bin").join("compile_commands.json");
        let has_compile_commands = compile_commands.exists();

        Ok(BuildOutput {
            path: config_path.to_path_buf(),
            relative_path,
            config: Some(config_name.to_string()),
            cpu,
            compilation_mode,
            has_compile_commands,
            symlink_target: None,
        })
    }

    fn analyze_symlink(
        workspace_root: &Path,
        symlink_path: &Path,
        symlink_name: &str,
    ) -> Result<Option<BuildOutput>, BazelError> {
        let relative_path = symlink_path
            .strip_prefix(workspace_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| symlink_path.to_string_lossy().to_string());

        // Try to resolve the symlink target
        let symlink_target = symlink_path.read_link().ok();

        // Check for compile_commands.json
        let compile_commands = symlink_path.join("compile_commands.json");
        let has_compile_commands = compile_commands.exists();

        Ok(Some(BuildOutput {
            path: symlink_path.to_path_buf(),
            relative_path,
            config: None,
            cpu: None,
            compilation_mode: None,
            has_compile_commands,
            symlink_target,
        }))
    }

    fn parse_config_name(config_name: &str) -> (Option<String>, Option<String>) {
        // Parse config names like "k8-fastbuild", "host", "k8-opt", "darwin-dbg", etc.
        if config_name == "host" {
            return (Some("host".to_string()), None);
        }

        if let Some(dash_pos) = config_name.rfind('-') {
            let cpu = &config_name[..dash_pos];
            let compilation_mode = &config_name[dash_pos + 1..];
            (Some(cpu.to_string()), Some(compilation_mode.to_string()))
        } else {
            (Some(config_name.to_string()), None)
        }
    }

    fn parse_workspace_file(workspace_file: &Path) -> Result<WorkspaceParseResult, std::io::Error> {
        let content = fs::read_to_string(workspace_file)?;

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

            // Track rule loads
            if line.starts_with("load(") {
                rules.push(line.to_string());
            }

            // Track repository rules (dependencies)
            if line.contains("_repository(") || line.contains("_archive(") || line.contains("git_repository(") {
                dependencies.push(line.to_string());
            }
        }

        // Sort for consistent output
        rules.sort();
        dependencies.sort();

        Ok((workspace_name, rules, dependencies, Some(workspace_file.parent().unwrap_or(workspace_file).to_path_buf())))
    }

    /// Find compilation database in Bazel build outputs
    ///
    /// This function looks for compile_commands.json in various Bazel output locations
    /// and returns the path to the most suitable one.
    #[allow(dead_code)]
    pub fn find_compilation_database(workspace_root: &Path) -> Option<PathBuf> {
        // Common locations for Bazel compilation databases
        let candidates = [
            "bazel-out/host/bin/compile_commands.json",
            "bazel-out/k8-fastbuild/bin/compile_commands.json", 
            "bazel-out/k8-opt/bin/compile_commands.json",
            "bazel-out/k8-dbg/bin/compile_commands.json",
            "bazel-bin/compile_commands.json",
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
}
