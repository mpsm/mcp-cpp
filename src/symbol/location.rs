use crate::io::file_buffer::{FileBufferError, FilePosition as FileBufPosition};
use crate::io::file_manager::FileBufferManager;
use crate::io::file_system::FileSystemTrait;

use std::fmt;
use std::path::{Path, PathBuf};

use lsp_types::{
    Location as LspLocation, LocationLink as LspLocationLink, Position as LspPosition,
    Range as LspRange,
};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLocation {
    pub range: Range,
    pub file_path: PathBuf,
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
    pub fn get_start_line(&self) -> u32 {
        self.range.start.line
    }

    /// Convert FileLocation to FileLine using the start line
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
    lsp_types::Uri::from_str(&path.to_string_lossy()).expect("Failed to convert PathBuf to Uri")
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
