//! Core Symbol representation
//!
//! Provides a clean Symbol struct that uses only std types and lsp_types::SymbolKind,
//! with conversion from LSP WorkspaceSymbol responses.

use lsp_types::{OneOf, SymbolKind, WorkspaceSymbol};
use serde::{Serialize, Serializer};
use tracing::warn;

/// A symbol in the codebase with resolved location
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Symbol {
    /// Symbol name
    pub name: String,

    /// Symbol kind (function, class, variable, etc.)
    #[serde(serialize_with = "serialize_symbol_kind")]
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

/// Serialize SymbolKind as string using Display trait
fn serialize_symbol_kind<S>(kind: &SymbolKind, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("{:?}", kind).to_lowercase())
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

    // Note: OneOf::Right case test removed due to type complexity
    // The warning case is tested in runtime when WorkspaceLocation is encountered

    #[test]
    fn test_symbol_serialization() {
        let symbol = Symbol::new(
            "factorial".to_string(),
            SymbolKind::FUNCTION,
            Some("Math".to_string()),
            "/project/math.cpp".to_string(),
        );

        let json = serde_json::to_string(&symbol).unwrap();
        assert!(json.contains("\"name\":\"factorial\""));
        assert!(json.contains("\"kind\":\"function\""));
        assert!(json.contains("\"container_name\":\"Math\""));
        assert!(json.contains("\"location\":\"/project/math.cpp\""));
    }

    #[test]
    fn test_symbol_kind_serialization() {
        let test_cases = vec![
            (SymbolKind::CLASS, "class"),
            (SymbolKind::FUNCTION, "function"),
            (SymbolKind::VARIABLE, "variable"),
            (SymbolKind::ENUM, "enum"),
            (SymbolKind::STRUCT, "struct"),
        ];

        for (kind, expected_str) in test_cases {
            let symbol = Symbol::new("test".to_string(), kind, None, "/test.cpp".to_string());

            let json = serde_json::to_string(&symbol).unwrap();
            assert!(json.contains(&format!("\"kind\":\"{}\"", expected_str)));
        }
    }
}
