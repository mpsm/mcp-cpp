use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};
use walkdir::WalkDir;

type CacheParseResult = (Option<String>, Option<String>, Vec<(String, String)>);

#[derive(Debug, Error)]
pub enum CmakeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Not a CMake project: no CMakeLists.txt found")]
    NotCmakeProject,
    
    #[error("CMakeCache.txt is corrupted or unreadable: {0}")]
    CorruptedCache(String),
    
    #[error("Multiple issues detected: {0:?}")]
    MultipleIssues(Vec<String>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildDirectory {
    pub path: PathBuf,
    pub relative_path: String,
    pub generator: Option<String>,
    pub build_type: Option<String>,
    pub configured_options: Vec<(String, String)>,
    pub cache_exists: bool,
    pub cache_readable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CmakeProjectStatus {
    pub is_cmake_project: bool,
    pub project_root: PathBuf,
    pub build_directories: Vec<BuildDirectory>,
    pub issues: Vec<String>,
}

impl CmakeProjectStatus {
    pub fn analyze_current_directory() -> Result<Self, CmakeError> {
        Self::analyze_directory(&std::env::current_dir()?)
    }
    
    pub fn analyze_directory(project_path: &Path) -> Result<Self, CmakeError> {
        debug!("Analyzing directory: {:?}", project_path);
        
        let mut status = CmakeProjectStatus {
            is_cmake_project: false,
            project_root: project_path.to_path_buf(),
            build_directories: Vec::new(),
            issues: Vec::new(),
        };
        
        // Check if this is a CMake project
        let cmake_lists = project_path.join("CMakeLists.txt");
        if !cmake_lists.exists() {
            return Err(CmakeError::NotCmakeProject);
        }
        
        status.is_cmake_project = true;
        debug!("Found CMakeLists.txt, this is a CMake project");
        
        // Scan for build directories
        status.build_directories = Self::scan_build_directories(project_path, &mut status.issues)?;
        
        if !status.issues.is_empty() && status.build_directories.is_empty() {
            return Err(CmakeError::MultipleIssues(status.issues.clone()));
        }
        
        Ok(status)
    }
    
    fn scan_build_directories(project_root: &Path, issues: &mut Vec<String>) -> Result<Vec<BuildDirectory>, CmakeError> {
        let mut build_dirs = Vec::new();
        
        // Look for directories containing CMakeCache.txt
        for entry in WalkDir::new(project_root)
            .max_depth(2) // Don't go too deep to avoid scanning the entire filesystem
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
        {
            let dir_path = entry.path();
            let cache_file = dir_path.join("CMakeCache.txt");
            
            if cache_file.exists() {
                debug!("Found potential build directory: {:?}", dir_path);
                
                match Self::analyze_build_directory(project_root, dir_path) {
                    Ok(build_dir) => build_dirs.push(build_dir),
                    Err(e) => {
                        let issue = format!("Build directory {:?}: {}", dir_path, e);
                        warn!("{}", issue);
                        issues.push(issue);
                    }
                }
            }
        }
        
        // Sort by path for consistent output
        build_dirs.sort_by(|a, b| a.path.cmp(&b.path));
        
        Ok(build_dirs)
    }
    
    fn analyze_build_directory(project_root: &Path, build_path: &Path) -> Result<BuildDirectory, CmakeError> {
        let cache_file = build_path.join("CMakeCache.txt");
        let cache_exists = cache_file.exists();
        
        let relative_path = build_path
            .strip_prefix(project_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| build_path.to_string_lossy().to_string());
        
        let mut build_dir = BuildDirectory {
            path: build_path.to_path_buf(),
            relative_path,
            generator: None,
            build_type: None,
            configured_options: Vec::new(),
            cache_exists,
            cache_readable: false,
        };
        
        if cache_exists {
            match Self::parse_cmake_cache(&cache_file) {
                Ok((generator, build_type, options)) => {
                    build_dir.cache_readable = true;
                    build_dir.generator = generator;
                    build_dir.build_type = build_type;
                    build_dir.configured_options = options;
                }
                Err(e) => {
                    return Err(CmakeError::CorruptedCache(format!("{:?}: {}", cache_file, e)));
                }
            }
        }
        
        Ok(build_dir)
    }
    
    fn parse_cmake_cache(cache_file: &Path) -> Result<CacheParseResult, std::io::Error> {
        let content = fs::read_to_string(cache_file)?;
        
        let mut generator = None;
        let mut build_type = None;
        let mut options = Vec::new();
        
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
                    _ if key.starts_with("CMAKE_") => {
                        // Skip internal CMake variables for cleaner output
                    }
                    _ => {
                        // User-defined options
                        options.push((key.to_string(), value.to_string()));
                    }
                }
            }
        }
        
        // Sort options for consistent output
        options.sort_by(|a, b| a.0.cmp(&b.0));
        
        Ok((generator, build_type, options))
    }
}