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
