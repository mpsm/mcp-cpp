//! Core Symbol representation
//!
//! Provides a clean Symbol struct that uses only std types and lsp_types::SymbolKind,
//! with conversion from LSP WorkspaceSymbol responses.

use lsp_types::{OneOf, SymbolKind, WorkspaceSymbol};
use serde::{Deserialize, Serialize};
use tracing::warn;

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

    /// Resolved file path as string
    pub location: String,
}

impl Symbol {
    /// Create a new Symbol
    #[allow(dead_code)]
    pub fn new(
        name: String,
        kind: SymbolKind,
        container_name: Option<String>,
        location: String,
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
            OneOf::Left(location) => {
                // Extract file path from URI
                location.uri.path().to_string()
            }
            OneOf::Right(_workspace_symbol) => {
                warn!("WorkspaceSymbol location variant not supported, using empty location");
                String::new()
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

/// Extract symbol location from WorkspaceSymbol
pub fn get_symbol_location(symbol: &WorkspaceSymbol) -> Option<crate::symbol::FileLocation> {
    match &symbol.location {
        lsp_types::OneOf::Left(loc) => Some(crate::symbol::FileLocation::from(loc)),
        lsp_types::OneOf::Right(_) => None, // WorkspaceLocation is not directly supported
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Location, Position, Range, Uri};
    use std::str::FromStr;

    #[test]
    fn test_symbol_creation() {
        let symbol = Symbol::new(
            "test_function".to_string(),
            SymbolKind::FUNCTION,
            Some("TestClass".to_string()),
            "/path/to/file.cpp".to_string(),
        );

        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.kind, SymbolKind::FUNCTION);
        assert_eq!(symbol.container_name, Some("TestClass".to_string()));
        assert_eq!(symbol.location, "/path/to/file.cpp");
    }

    #[test]
    fn test_from_workspace_symbol_with_location() {
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
        assert_eq!(symbol.location, "/path/to/test.cpp");
    }
}
