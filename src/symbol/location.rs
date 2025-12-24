use crate::io::file_buffer::{FileBufferError, FilePosition as FileBufPosition};
use crate::io::file_manager::FileBufferManager;
use crate::io::file_system::FileSystemTrait;

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_types::{
    Location as LspLocation, LocationLink as LspLocationLink, Position as LspPosition,
    Range as LspRange,
};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilePosition {
    pub position: Position,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLocation {
    pub range: Range,
    pub file_path: PathBuf,
}

impl Serialize for FileLocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize to compact LSP-style format
        serializer.serialize_str(&self.to_compact_range())
    }
}

impl<'de> Deserialize<'de> for FileLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        use std::fmt;

        struct FileLocationVisitor;

        impl<'de> Visitor<'de> for FileLocationVisitor {
            type Value = FileLocation;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a compact location string or FileLocation object")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                // Parse compact format: file.cpp:23:5-25:10
                let parts: Vec<&str> = value.splitn(2, ':').collect();
                if parts.len() != 2 {
                    return Err(E::custom("Invalid compact location format"));
                }

                let file_path = PathBuf::from(parts[0]);
                let range_part = parts[1];

                // Parse range part: 23:5-25:10 or 23:5-20 or 23:5
                let range_parts: Vec<&str> = range_part.split('-').collect();

                let start_parts: Vec<&str> = range_parts[0].split(':').collect();
                if start_parts.len() < 2 {
                    return Err(E::custom("Invalid start position format"));
                }

                let start_line: u32 = start_parts[0].parse().map_err(E::custom)?;
                let start_col: u32 = start_parts[1].parse().map_err(E::custom)?;

                let (end_line, end_col) = if range_parts.len() > 1 {
                    let end_parts: Vec<&str> = range_parts[1].split(':').collect();
                    if end_parts.len() == 2 {
                        // Multi-line: 23:5-25:10
                        let end_line: u32 = end_parts[0].parse().map_err(E::custom)?;
                        let end_col: u32 = end_parts[1].parse().map_err(E::custom)?;
                        (end_line, end_col)
                    } else {
                        // Same line: 23:5-20
                        let end_col: u32 = end_parts[0].parse().map_err(E::custom)?;
                        (start_line, end_col)
                    }
                } else {
                    // Point location: 23:5
                    (start_line, start_col)
                };

                // Convert from 1-based to 0-based
                Ok(FileLocation {
                    file_path,
                    range: Range {
                        start: Position {
                            line: start_line.saturating_sub(1),
                            column: start_col.saturating_sub(1),
                        },
                        end: Position {
                            line: end_line.saturating_sub(1),
                            column: end_col.saturating_sub(1),
                        },
                    },
                })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut range = None;
                let mut file_path = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "range" => range = Some(map.next_value()?),
                        "file_path" => file_path = Some(map.next_value()?),
                        _ => {
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                Ok(FileLocation {
                    range: range.ok_or_else(|| de::Error::missing_field("range"))?,
                    file_path: file_path.ok_or_else(|| de::Error::missing_field("file_path"))?,
                })
            }
        }

        deserializer.deserialize_any(FileLocationVisitor)
    }
}

impl FromStr for FileLocation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.rsplitn(3, ':').collect();
        if parts.len() != 3 {
            return Err(format!(
                "Invalid format: expected '/path/file.cpp:line:column', got '{}'",
                s
            ));
        }

        let column: u32 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid column number: '{}'", parts[0]))?;
        let line: u32 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid line number: '{}'", parts[1]))?;
        let file_path = parts[2];

        if line == 0 || column == 0 {
            return Err("Line and column numbers must be 1-based (> 0)".to_string());
        }

        Ok(FileLocation {
            file_path: PathBuf::from(file_path),
            range: Range {
                start: Position {
                    line: line - 1,
                    column: column - 1,
                }, // Convert to 0-based
                end: Position {
                    line: line - 1,
                    column: column - 1,
                }, // Same position for point location
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLine {
    pub file_path: PathBuf,
    pub line_number: u32, // 0-based
}

impl fmt::Display for FileLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file_path.display(), self.line_number)
    }
}

impl FileLocation {
    /// Get the start line number (0-based) from the range
    #[allow(dead_code)]
    pub fn get_start_line(&self) -> u32 {
        self.range.start.line
    }

    /// Convert FileLocation to FileLine using the start line
    #[allow(dead_code)]
    pub fn to_file_line(&self) -> FileLine {
        FileLine {
            file_path: self.file_path.clone(),
            line_number: self.get_start_line(),
        }
    }

    /// Get the LSP URI for this file location
    pub fn get_uri(&self) -> lsp_types::Uri {
        let path_str = self.file_path.to_string_lossy();
        let uri_str = if path_str.starts_with('/') {
            format!("file://{}", path_str)
        } else {
            format!("file:///{}", path_str)
        };
        uri_str.parse().expect("Failed to parse URI from file path")
    }

    /// Convert FileLocation to compact LSP-style range format
    /// Examples:
    /// - Point location: "file.cpp:23:5"
    /// - Same line range: "file.cpp:23:5-20"
    /// - Multi-line range: "file.cpp:23:5-25:10"
    pub fn to_compact_range(&self) -> String {
        let path = self.file_path.to_string_lossy();
        let start = &self.range.start;
        let end = &self.range.end;

        // Convert from 0-based to 1-based for human readability
        let start_line = start.line + 1;
        let start_col = start.column + 1;
        let end_line = end.line + 1;
        let end_col = end.column + 1;

        if start_line == end_line {
            if start_col == end_col {
                // Point location
                format!("{}:{}:{}", path, start_line, start_col)
            } else {
                // Same line range
                format!("{}:{}:{}-{}", path, start_line, start_col, end_col)
            }
        } else {
            // Multi-line range
            format!(
                "{}:{}:{}-{}:{}",
                path, start_line, start_col, end_line, end_col
            )
        }
    }
}

impl From<Position> for FileBufPosition {
    fn from(position: Position) -> Self {
        FileBufPosition {
            line: position.line,
            column: position.column,
        }
    }
}

impl From<LspPosition> for Position {
    fn from(pos: LspPosition) -> Self {
        Position {
            line: pos.line,
            column: pos.character,
        }
    }
}

impl From<Position> for LspPosition {
    fn from(pos: Position) -> Self {
        LspPosition {
            line: pos.line,
            character: pos.column,
        }
    }
}

impl From<LspRange> for Range {
    fn from(range: LspRange) -> Self {
        Range {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl From<Range> for LspRange {
    fn from(range: Range) -> Self {
        LspRange {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl From<LspLocation> for FilePosition {
    fn from(location: LspLocation) -> Self {
        FilePosition {
            position: location.range.start.into(),
            file_path: location.uri.path().to_string().into(),
        }
    }
}

pub fn uri_from_pathbuf(path: &Path) -> lsp_types::Uri {
    use std::str::FromStr;
    let uri_string = format!("file://{}", path.to_string_lossy());
    lsp_types::Uri::from_str(&uri_string).expect("Failed to convert PathBuf to Uri")
}

pub fn pathbuf_from_uri(uri: &lsp_types::Uri) -> PathBuf {
    uri.path().to_string().into()
}

impl From<FilePosition> for LspLocation {
    fn from(file_position: FilePosition) -> Self {
        LspLocation {
            uri: uri_from_pathbuf(&file_position.file_path),
            range: LspRange::from(Range {
                start: file_position.position,
                end: file_position.position,
            }),
        }
    }
}

impl From<&LspLocation> for FileLocation {
    fn from(location: &LspLocation) -> Self {
        FileLocation {
            range: Range::from(location.range),
            file_path: pathbuf_from_uri(&location.uri),
        }
    }
}

impl From<&LspLocationLink> for FileLocation {
    fn from(location_link: &LspLocationLink) -> Self {
        FileLocation {
            range: Range::from(location_link.target_selection_range),
            file_path: pathbuf_from_uri(&location_link.target_uri),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLocationWithContents {
    pub location: FileLocation,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLineWithContents {
    pub line: FileLine,
    pub contents: String,
}

impl FileLocationWithContents {
    #[allow(dead_code)]
    pub fn new_from_location<T: FileSystemTrait>(
        location: &FileLocation,
        file_buf_manager: &mut FileBufferManager<T>,
    ) -> Result<Self, FileBufferError> {
        let file_buffer = file_buf_manager.get_buffer(&location.file_path)?;
        Ok(FileLocationWithContents {
            location: location.clone(),
            contents: file_buffer
                .text_between(location.range.start.into(), location.range.end.into())?,
        })
    }

    /// Create FileLocationWithContents using the full line at the location, trimmed on both ends
    #[allow(dead_code)]
    pub fn new_from_location_full_line<T: FileSystemTrait>(
        location: &FileLocation,
        file_buf_manager: &mut FileBufferManager<T>,
    ) -> Result<Self, FileBufferError> {
        let file_buffer = file_buf_manager.get_buffer(&location.file_path)?;
        Ok(FileLocationWithContents {
            location: location.clone(),
            contents: file_buffer.get_line(location.range.start.line)?,
        })
    }
}

impl FileLineWithContents {
    /// Create FileLineWithContents from a FileLine, getting the full line content trimmed
    #[allow(dead_code)]
    pub fn new_from_file_line<T: FileSystemTrait>(
        file_line: &FileLine,
        file_buf_manager: &mut FileBufferManager<T>,
    ) -> Result<Self, FileBufferError> {
        let file_buffer = file_buf_manager.get_buffer(&file_line.file_path)?;
        Ok(FileLineWithContents {
            line: file_line.clone(),
            contents: file_buffer.get_line(file_line.line_number)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_range_point_location() {
        let loc = FileLocation {
            file_path: PathBuf::from("/path/to/file.cpp"),
            range: Range {
                start: Position {
                    line: 22,
                    column: 4,
                },
                end: Position {
                    line: 22,
                    column: 4,
                },
            },
        };
        assert_eq!(loc.to_compact_range(), "/path/to/file.cpp:23:5");
    }

    #[test]
    fn test_compact_range_same_line() {
        let loc = FileLocation {
            file_path: PathBuf::from("/path/to/file.cpp"),
            range: Range {
                start: Position {
                    line: 22,
                    column: 4,
                },
                end: Position {
                    line: 22,
                    column: 19,
                },
            },
        };
        assert_eq!(loc.to_compact_range(), "/path/to/file.cpp:23:5-20");
    }

    #[test]
    fn test_compact_range_multi_line() {
        let loc = FileLocation {
            file_path: PathBuf::from("/path/to/file.cpp"),
            range: Range {
                start: Position {
                    line: 22,
                    column: 4,
                },
                end: Position {
                    line: 24,
                    column: 9,
                },
            },
        };
        assert_eq!(loc.to_compact_range(), "/path/to/file.cpp:23:5-25:10");
    }

    #[test]
    fn test_serialize_file_location() {
        let loc = FileLocation {
            file_path: PathBuf::from("/test/file.cpp"),
            range: Range {
                start: Position { line: 9, column: 2 },
                end: Position {
                    line: 9,
                    column: 15,
                },
            },
        };
        let serialized = serde_json::to_string(&loc).unwrap();
        assert_eq!(serialized, "\"/test/file.cpp:10:3-16\"");
    }

    #[test]
    fn test_deserialize_compact_point_location() {
        let json = "\"/test/file.cpp:10:3\"";
        let loc: FileLocation = serde_json::from_str(json).unwrap();
        assert_eq!(loc.file_path, PathBuf::from("/test/file.cpp"));
        assert_eq!(loc.range.start.line, 9);
        assert_eq!(loc.range.start.column, 2);
        assert_eq!(loc.range.end.line, 9);
        assert_eq!(loc.range.end.column, 2);
    }

    #[test]
    fn test_deserialize_compact_same_line() {
        let json = "\"/test/file.cpp:10:3-16\"";
        let loc: FileLocation = serde_json::from_str(json).unwrap();
        assert_eq!(loc.file_path, PathBuf::from("/test/file.cpp"));
        assert_eq!(loc.range.start.line, 9);
        assert_eq!(loc.range.start.column, 2);
        assert_eq!(loc.range.end.line, 9);
        assert_eq!(loc.range.end.column, 15);
    }

    #[test]
    fn test_deserialize_compact_multi_line() {
        let json = "\"/test/file.cpp:10:3-12:7\"";
        let loc: FileLocation = serde_json::from_str(json).unwrap();
        assert_eq!(loc.file_path, PathBuf::from("/test/file.cpp"));
        assert_eq!(loc.range.start.line, 9);
        assert_eq!(loc.range.start.column, 2);
        assert_eq!(loc.range.end.line, 11);
        assert_eq!(loc.range.end.column, 6);
    }

    #[test]
    fn test_deserialize_object_format() {
        let json = r#"{
            "file_path": "/test/file.cpp",
            "range": {
                "start": {"line": 9, "column": 2},
                "end": {"line": 11, "column": 6}
            }
        }"#;
        let loc: FileLocation = serde_json::from_str(json).unwrap();
        assert_eq!(loc.file_path, PathBuf::from("/test/file.cpp"));
        assert_eq!(loc.range.start.line, 9);
        assert_eq!(loc.range.start.column, 2);
        assert_eq!(loc.range.end.line, 11);
        assert_eq!(loc.range.end.column, 6);
    }

    #[test]
    fn test_roundtrip_serialization() {
        let original = FileLocation {
            file_path: PathBuf::from("/path/to/test.cpp"),
            range: Range {
                start: Position {
                    line: 99,
                    column: 14,
                },
                end: Position {
                    line: 102,
                    column: 8,
                },
            },
        };

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: FileLocation = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original, deserialized);
    }
}
