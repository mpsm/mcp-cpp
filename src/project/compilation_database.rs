use json_compilation_db::Entry;
use serde::{Deserialize, Serialize, Serializer};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompilationDatabaseError {
    #[error("Compilation database file not found: {path}")]
    FileNotFound { path: String },
    #[error("Failed to read compilation database file: {error}")]
    ReadError { error: String },
    #[error("Failed to parse compilation database JSON: {error}")]
    ParseError { error: String },
    #[error("Compilation database is empty")]
    EmptyDatabase,
}

/// Wrapper around compilation database providing structured access to compilation entries
///
/// This struct contains both the path to the compilation database file and the parsed entries.
/// When serialized, only the path is included in the output to avoid serializing large database content.
#[derive(Debug, Deserialize)]
pub struct CompilationDatabase {
    /// Path to the compilation database file (compile_commands.json)
    pub path: PathBuf,
    /// Parsed compilation database entries (loaded at initialization)
    #[serde(skip)]
    #[allow(dead_code)]
    pub entries: Vec<Entry>,
}

impl CompilationDatabase {
    /// Create a new compilation database by loading and parsing the file at the given path
    ///
    /// This immediately loads and parses the compilation database, returning an error if
    /// the file doesn't exist, can't be read, or contains invalid JSON.
    pub fn new(path: PathBuf) -> Result<Self, CompilationDatabaseError> {
        // Check if file exists
        if !path.exists() {
            return Err(CompilationDatabaseError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            });
        }

        // Open and read the file
        let file = std::fs::File::open(&path).map_err(|e| CompilationDatabaseError::ReadError {
            error: e.to_string(),
        })?;

        // Parse the JSON compilation database
        let reader = std::io::BufReader::new(file);
        let entries: Vec<Entry> =
            serde_json::from_reader(reader).map_err(|e| CompilationDatabaseError::ParseError {
                error: e.to_string(),
            })?;

        // Check if database is empty
        if entries.is_empty() {
            return Err(CompilationDatabaseError::EmptyDatabase);
        }

        Ok(Self { path, entries })
    }

    /// Get all compilation database entries
    #[allow(dead_code)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Get the number of entries in the compilation database
    #[allow(dead_code)]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the path to the compilation database file
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Check if the database contains an entry for the specified file
    #[allow(dead_code)]
    pub fn contains_file(&self, file_path: &Path) -> bool {
        self.entries.iter().any(|entry| entry.file == file_path)
    }

    /// Get all unique source files referenced in the compilation database
    #[allow(dead_code)]
    pub fn source_files(&self) -> Vec<&Path> {
        let mut files: Vec<&Path> = self
            .entries
            .iter()
            .map(|entry| entry.file.as_path())
            .collect();
        files.sort();
        files.dedup();
        files
    }

    /// Get all unique directories referenced in the compilation database
    #[allow(dead_code)]
    pub fn directories(&self) -> Vec<&Path> {
        let mut dirs: Vec<&Path> = self
            .entries
            .iter()
            .map(|entry| entry.directory.as_path())
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    }
}

/// Custom serialization that only outputs the path field
///
/// This ensures that when the CompilationDatabase is serialized (e.g., in JSON responses),
/// only the path is included, not the potentially large entries array.
impl Serialize for CompilationDatabase {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.path.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_compilation_db(content: &str) -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        temp_file
    }

    #[test]
    fn test_new_with_valid_json() {
        let content = r#"[
            {
                "directory": "/home/user/project",
                "file": "src/main.cpp",
                "arguments": ["clang++", "-c", "src/main.cpp"]
            },
            {
                "directory": "/home/user/project",
                "file": "src/lib.cpp",
                "command": "clang++ -c src/lib.cpp"
            }
        ]"#;

        let temp_file = create_temp_compilation_db(content);
        let db = CompilationDatabase::new(temp_file.path().to_path_buf());

        assert!(db.is_ok());
        let db = db.unwrap();
        assert_eq!(db.entry_count(), 2);
        assert!(db.contains_file(Path::new("src/main.cpp")));
        assert!(db.contains_file(Path::new("src/lib.cpp")));
        assert!(!db.contains_file(Path::new("src/nonexistent.cpp")));
    }

    #[test]
    fn test_new_with_nonexistent_file() {
        let db = CompilationDatabase::new(PathBuf::from("/nonexistent/path/compile_commands.json"));

        assert!(db.is_err());
        match db.unwrap_err() {
            CompilationDatabaseError::FileNotFound { path } => {
                assert!(path.contains("nonexistent"));
            }
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_new_with_invalid_json() {
        let content = r#"{ "invalid": "json", "not": ["an", "array"] }"#;
        let temp_file = create_temp_compilation_db(content);
        let db = CompilationDatabase::new(temp_file.path().to_path_buf());

        assert!(db.is_err());
        match db.unwrap_err() {
            CompilationDatabaseError::ParseError { .. } => {}
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_new_with_empty_array() {
        let content = r#"[]"#;
        let temp_file = create_temp_compilation_db(content);
        let db = CompilationDatabase::new(temp_file.path().to_path_buf());

        assert!(db.is_err());
        match db.unwrap_err() {
            CompilationDatabaseError::EmptyDatabase => {}
            _ => panic!("Expected EmptyDatabase error"),
        }
    }

    #[test]
    fn test_serialization_only_includes_path() {
        let content = r#"[
            {
                "directory": "/home/user/project",
                "file": "src/main.cpp",
                "arguments": ["clang++", "-c", "src/main.cpp"]
            }
        ]"#;

        let temp_file = create_temp_compilation_db(content);
        let db = CompilationDatabase::new(temp_file.path().to_path_buf()).unwrap();

        let serialized = serde_json::to_string(&db).unwrap();
        let expected_path = format!("\"{}\"", temp_file.path().to_string_lossy());

        assert_eq!(serialized, expected_path);
        // Ensure entries are not serialized
        assert!(!serialized.contains("entries"));
        assert!(!serialized.contains("main.cpp"));
    }

    #[test]
    fn test_source_files_deduplication() {
        let content = r#"[
            {
                "directory": "/home/user/project",
                "file": "src/main.cpp",
                "arguments": ["clang++", "-c", "src/main.cpp"]
            },
            {
                "directory": "/home/user/project",
                "file": "src/main.cpp",
                "arguments": ["clang++", "-O2", "-c", "src/main.cpp"]
            },
            {
                "directory": "/home/user/project",
                "file": "src/lib.cpp",
                "command": "clang++ -c src/lib.cpp"
            }
        ]"#;

        let temp_file = create_temp_compilation_db(content);
        let db = CompilationDatabase::new(temp_file.path().to_path_buf()).unwrap();

        let files = db.source_files();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&Path::new("src/lib.cpp")));
        assert!(files.contains(&Path::new("src/main.cpp")));
    }

    #[test]
    fn test_directories_deduplication() {
        let content = r#"[
            {
                "directory": "/home/user/project",
                "file": "src/main.cpp",
                "arguments": ["clang++", "-c", "src/main.cpp"]
            },
            {
                "directory": "/home/user/project",
                "file": "src/lib.cpp",
                "command": "clang++ -c src/lib.cpp"
            },
            {
                "directory": "/home/user/other",
                "file": "test/test.cpp",
                "command": "clang++ -c test/test.cpp"
            }
        ]"#;

        let temp_file = create_temp_compilation_db(content);
        let db = CompilationDatabase::new(temp_file.path().to_path_buf()).unwrap();

        let dirs = db.directories();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&Path::new("/home/user/other")));
        assert!(dirs.contains(&Path::new("/home/user/project")));
    }
}
