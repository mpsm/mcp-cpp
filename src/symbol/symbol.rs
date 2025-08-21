//! Core Symbol representation
//!
//! Provides a clean Symbol struct that uses only std types and lsp_types::SymbolKind,
//! with conversion from LSP WorkspaceSymbol responses.

use lsp_types::{OneOf, SymbolKind, WorkspaceSymbol};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::symbol::FileLocation;

/// A symbol in the codebase with resolved location
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol name
    pub name: String,

    /// Symbol kind (function, class, variable, etc.)
    pub kind: SymbolKind,

    /// Container name (namespace, class, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,

    /// Symbol location with file path and range
    pub location: FileLocation,
}

impl Symbol {
    /// Create a new Symbol
    #[allow(dead_code)]
    pub fn new(
        name: String,
        kind: SymbolKind,
        container_name: Option<String>,
        location: FileLocation,
    ) -> Self {
        Self {
            name,
            kind,
            container_name,
            location,
        }
    }
}

impl From<WorkspaceSymbol> for Symbol {
    fn from(ws_symbol: WorkspaceSymbol) -> Self {
        let location = match ws_symbol.location {
            OneOf::Left(location) => FileLocation::from(&location),
            OneOf::Right(_workspace_symbol) => {
                warn!("WorkspaceSymbol location variant not supported, using empty location");
                // Create a default FileLocation with empty path and zero range
                FileLocation {
                    file_path: std::path::PathBuf::new(),
                    range: crate::symbol::location::Range {
                        start: crate::symbol::location::Position { line: 0, column: 0 },
                        end: crate::symbol::location::Position { line: 0, column: 0 },
                    },
                }
            }
        };

        Self {
            name: ws_symbol.name,
            kind: ws_symbol.kind,
            container_name: ws_symbol.container_name,
            location,
        }
    }
}

impl From<(&lsp_types::DocumentSymbol, &std::path::Path)> for Symbol {
    fn from((doc_symbol, file_path): (&lsp_types::DocumentSymbol, &std::path::Path)) -> Self {
        use crate::symbol::location::Range as SymRange;

        Self {
            name: doc_symbol.name.clone(),
            kind: doc_symbol.kind,
            container_name: None, // DocumentSymbol doesn't have container_name
            location: FileLocation {
                file_path: file_path.to_path_buf(),
                range: SymRange::from(doc_symbol.selection_range),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Location, Position, Range, Uri};
    use std::str::FromStr;

    #[test]
    fn test_symbol_creation() {
        use crate::symbol::location::{FileLocation, Position as SymPosition, Range as SymRange};
        use std::path::PathBuf;

        let location = FileLocation {
            file_path: PathBuf::from("/path/to/file.cpp"),
            range: SymRange {
                start: SymPosition {
                    line: 10,
                    column: 5,
                },
                end: SymPosition {
                    line: 10,
                    column: 20,
                },
            },
        };

        let symbol = Symbol::new(
            "test_function".to_string(),
            SymbolKind::FUNCTION,
            Some("TestClass".to_string()),
            location.clone(),
        );

        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.kind, SymbolKind::FUNCTION);
        assert_eq!(symbol.container_name, Some("TestClass".to_string()));
        assert_eq!(symbol.location, location);
    }

    #[test]
    fn test_from_workspace_symbol_with_location() {
        use std::path::PathBuf;

        let uri = Uri::from_str("file:///path/to/test.cpp").unwrap();
        let location = Location {
            uri,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
        };

        let ws_symbol = WorkspaceSymbol {
            name: "Math".to_string(),
            kind: SymbolKind::CLASS,
            tags: None,
            container_name: Some("TestProject".to_string()),
            location: OneOf::Left(location),
            data: None,
        };

        let symbol = Symbol::from(ws_symbol);

        assert_eq!(symbol.name, "Math");
        assert_eq!(symbol.kind, SymbolKind::CLASS);
        assert_eq!(symbol.container_name, Some("TestProject".to_string()));
        assert_eq!(
            symbol.location.file_path,
            PathBuf::from("/path/to/test.cpp")
        );
        assert_eq!(symbol.location.range.start.line, 0);
        assert_eq!(symbol.location.range.start.column, 0);
        assert_eq!(symbol.location.range.end.line, 0);
        assert_eq!(symbol.location.range.end.column, 10);
    }
}
