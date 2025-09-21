use json_compilation_db::Entry;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Type alias for bidirectional path mappings
/// (original_path -> canonical_path, canonical_path -> original_path)
pub type PathMappings = (HashMap<PathBuf, PathBuf>, HashMap<PathBuf, PathBuf>);

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
#[derive(Debug, Clone, Deserialize)]
pub struct CompilationDatabase {
    /// Path to the compilation database file (compile_commands.json)
    pub path: PathBuf,
    /// Parsed compilation database entries (loaded at initialization)
    #[serde(skip)]
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

    /// Create a compilation database from entries for testing
    ///
    /// This bypasses filesystem operations and creates a CompilationDatabase
    /// directly from provided entries, useful for unit tests.
    #[cfg(test)]
    pub fn from_entries(entries: Vec<Entry>) -> Self {
        Self {
            path: PathBuf::from("/test/compile_commands.json"),
            entries,
        }
    }

    /// Get all compilation database entries
    #[allow(dead_code)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Get the path to the compilation database file
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get all unique source files with canonicalized paths
    ///
    /// This method resolves relative paths against the compilation database directory
    /// and canonicalizes them to absolute paths. This ensures consistent path handling
    /// between CMake (which uses absolute paths) and Meson (which uses relative paths).
    pub fn canonical_source_files(&self) -> Result<Vec<PathBuf>, CompilationDatabaseError> {
        let mut canonical_files = Vec::new();
        let mut seen_files = std::collections::HashSet::new();

        for entry in &self.entries {
            let canonical_path = self.canonicalize_entry_path(&entry.file)?;
            if seen_files.insert(canonical_path.clone()) {
                canonical_files.push(canonical_path);
            }
        }

        canonical_files.sort();
        Ok(canonical_files)
    }

    /// Get bidirectional mappings between original and canonical paths
    ///
    /// Returns (original -> canonical, canonical -> original) mappings.
    /// This enables efficient lookup in both directions without repeated canonicalization.
    pub fn path_mappings(&self) -> Result<PathMappings, CompilationDatabaseError> {
        let mut original_to_canonical = HashMap::new();
        let mut canonical_to_original = HashMap::new();

        for entry in &self.entries {
            let original_path = entry.file.clone();
            let canonical_path = self.canonicalize_entry_path(&entry.file)?;

            original_to_canonical.insert(original_path.clone(), canonical_path.clone());
            canonical_to_original.insert(canonical_path, original_path);
        }

        Ok((original_to_canonical, canonical_to_original))
    }

    /// Canonicalize a single entry path using the same logic for all paths
    ///
    /// This is the single source of truth for path canonicalization in the system.
    /// It handles both CMake's absolute paths and Meson's relative paths consistently.
    fn canonicalize_entry_path(
        &self,
        entry_path: &Path,
    ) -> Result<PathBuf, CompilationDatabaseError> {
        let compilation_db_dir = self.path.parent().unwrap_or_else(|| Path::new("."));

        // Resolve relative paths against compilation database directory
        let resolved_path = if entry_path.is_relative() {
            compilation_db_dir.join(entry_path)
        } else {
            entry_path.to_path_buf()
        };

        // Attempt canonicalization, fall back to resolved path if it fails
        // This handles cases where files don't exist yet (like in tests)
        match resolved_path.canonicalize() {
            Ok(canonical) => Ok(canonical),
            Err(_) => {
                // For non-existent files (tests, etc.), use the resolved path
                Ok(resolved_path)
            }
        }
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
