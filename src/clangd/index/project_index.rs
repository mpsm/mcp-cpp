use super::hash::compute_file_hash;
use crate::clangd::version::ClangdVersion;
use crate::project::CompilationDatabase;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProjectIndexError {
    #[error("Index directory does not exist: {path}")]
    IndexDirectoryNotFound { path: String },
    #[error("Unable to access index directory: {path}")]
    IndexDirectoryAccess { path: String },
}

/// Maps source files to their corresponding clangd index files
pub struct ProjectIndex {
    /// Path to the index directory (.cache/clangd/index/)
    index_dir: PathBuf,
    /// Mapping from source file path to index file path
    file_to_index: HashMap<PathBuf, PathBuf>,
    /// Clangd version for hash function selection
    format_version: u32,
}

impl ProjectIndex {
    /// Create a new ProjectIndex from a compilation database and clangd version
    pub fn new(
        compilation_db: &CompilationDatabase,
        clangd_version: &ClangdVersion,
    ) -> Result<Self, ProjectIndexError> {
        let compilation_db_path = compilation_db.path();
        let compilation_db_dir = compilation_db_path
            .parent()
            .unwrap_or_else(|| Path::new("."));

        // Index directory is .cache/clangd/index/ relative to compilation database
        let index_dir = compilation_db_dir
            .join(".cache")
            .join("clangd")
            .join("index");

        // Check if index directory exists
        if !index_dir.exists() {
            return Err(ProjectIndexError::IndexDirectoryNotFound {
                path: index_dir.to_string_lossy().to_string(),
            });
        }

        if !index_dir.is_dir() {
            return Err(ProjectIndexError::IndexDirectoryAccess {
                path: index_dir.to_string_lossy().to_string(),
            });
        }

        let format_version = clangd_version.index_format_version();
        let mut file_to_index = HashMap::new();

        // Build mapping for each file in the compilation database
        for entry in compilation_db.entries() {
            let source_file = &entry.file;

            // Compute hash for the source file path
            let file_path_str = source_file.to_string_lossy();
            let hash = compute_file_hash(&file_path_str, format_version);

            // Extract basename from path
            let basename = source_file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");

            // Construct index filename: basename.hash.idx
            let index_filename = format!("{basename}.{hash:016X}.idx");
            let index_path = index_dir.join(&index_filename);

            // Only add mapping if index file actually exists
            if index_path.exists() {
                file_to_index.insert(source_file.to_path_buf(), index_path);
            }
        }

        Ok(ProjectIndex {
            index_dir,
            file_to_index,
            format_version,
        })
    }

    /// Get the index file path for a given source file
    pub fn get_index_file(&self, source_file: &Path) -> Option<&Path> {
        self.file_to_index.get(source_file).map(|p| p.as_path())
    }

    /// Get all source files that have corresponding index files
    pub fn indexed_files(&self) -> Vec<&Path> {
        self.file_to_index.keys().map(|p| p.as_path()).collect()
    }

    /// Get the number of indexed files
    pub fn indexed_count(&self) -> usize {
        self.file_to_index.len()
    }

    /// Get the index directory path
    pub fn index_directory(&self) -> &Path {
        &self.index_dir
    }

    /// Get the format version used for this index
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Check if a source file has an index file
    pub fn is_indexed(&self, source_file: &Path) -> bool {
        self.file_to_index.contains_key(source_file)
    }

    /// Get all index file paths
    pub fn index_files(&self) -> Vec<&Path> {
        self.file_to_index.values().map(|p| p.as_path()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clangd::version::ClangdVersion;
    use crate::project::CompilationDatabase;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_compilation_database(dir: &Path) -> std::io::Result<CompilationDatabase> {
        let compile_commands_path = dir.join("compile_commands.json");
        let mut file = fs::File::create(&compile_commands_path)?;

        // Create a simple compilation database
        let content = r#"[
            {
                "directory": "/test/project",
                "command": "clang++ -c main.cpp -o main.o",
                "file": "/test/project/main.cpp"
            },
            {
                "directory": "/test/project",
                "command": "clang++ -c utils.cpp -o utils.o", 
                "file": "/test/project/utils.cpp"
            }
        ]"#;

        file.write_all(content.as_bytes())?;
        Ok(CompilationDatabase::new(compile_commands_path).unwrap())
    }

    fn create_test_index_files(index_dir: &Path, version: &ClangdVersion) -> std::io::Result<()> {
        fs::create_dir_all(index_dir)?;

        // Create index files for the test files
        let test_files = ["/test/project/main.cpp", "/test/project/utils.cpp"];

        for file_path in &test_files {
            let hash = compute_file_hash(file_path, version.index_format_version());
            let basename = Path::new(file_path).file_name().unwrap().to_str().unwrap();
            let index_filename = format!("{basename}.{hash:016X}.idx");
            let index_path = index_dir.join(index_filename);

            // Create empty index file
            fs::File::create(index_path)?;
        }

        Ok(())
    }

    #[test]
    fn test_project_index_creation() -> std::io::Result<()> {
        let temp_dir = TempDir::new()?;
        let build_dir = temp_dir.path();

        // Create compilation database
        let compilation_db = create_test_compilation_database(build_dir)?;

        // Create index directory and files
        let index_dir = build_dir.join(".cache").join("clangd").join("index");
        let version = ClangdVersion {
            major: 18,
            minor: 1,
            patch: 8,
            variant: None,
            date: None,
        };
        create_test_index_files(&index_dir, &version)?;

        // Create ProjectIndex
        let project_index = ProjectIndex::new(&compilation_db, &version).unwrap();

        // Verify mappings
        assert_eq!(project_index.indexed_count(), 2);
        assert!(project_index.is_indexed(Path::new("/test/project/main.cpp")));
        assert!(project_index.is_indexed(Path::new("/test/project/utils.cpp")));

        let main_index = project_index.get_index_file(Path::new("/test/project/main.cpp"));
        assert!(main_index.is_some());
        assert!(main_index.unwrap().exists());

        Ok(())
    }

    #[test]
    fn test_project_index_missing_directory() {
        let temp_dir = TempDir::new().unwrap();
        let build_dir = temp_dir.path();

        let compilation_db = create_test_compilation_database(build_dir).unwrap();
        let version = ClangdVersion {
            major: 18,
            minor: 1,
            patch: 8,
            variant: None,
            date: None,
        };

        // Don't create index directory
        let result = ProjectIndex::new(&compilation_db, &version);
        assert!(matches!(
            result,
            Err(ProjectIndexError::IndexDirectoryNotFound { .. })
        ));
    }

    #[test]
    fn test_format_version_selection() {
        let version_18 = ClangdVersion {
            major: 18,
            minor: 1,
            patch: 8,
            variant: None,
            date: None,
        };
        assert_eq!(version_18.index_format_version(), 19);

        let version_14 = ClangdVersion {
            major: 14,
            minor: 0,
            patch: 0,
            variant: Some("1ubuntu1.1".to_string()),
            date: None,
        };
        assert_eq!(version_14.index_format_version(), 17);
    }
}
