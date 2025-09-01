//! Clangd index file (.idx) parser
//!
//! This module provides functionality for parsing clangd index files to extract
//! translation unit information and content hashes from the `srcs` chunk.
//!
//! Supports clangd format versions 12-20 with inline version handling.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use thiserror::Error;

/// Errors that can occur during index file parsing
#[derive(Debug, Error)]
pub enum IdxParseError {
    #[error("Invalid RIFF magic bytes")]
    InvalidMagic,

    #[error("Invalid clangd index type identifier")]
    InvalidType,

    #[error("Unsupported format version: {0}")]
    UnsupportedVersion(u32),

    #[error("Missing required chunk: {0}")]
    MissingChunk(String),

    #[error("Corrupted chunk data: {0}")]
    CorruptedChunk(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Decompression error: {0}")]
    Decompression(String),

    #[error("String encoding error: {0}")]
    StringEncoding(#[from] std::string::FromUtf8Error),
}

/// Represents a parsed include graph node from the srcs chunk
#[derive(Debug, Clone, PartialEq)]
pub struct IncludeGraphNode {
    /// Flags indicating properties of this file
    pub flags: u8,
    /// URI of the file (typically a file path)
    pub uri: String,
    /// 8-byte content hash for staleness detection
    pub digest: [u8; 8],
    /// List of files directly included by this file
    pub direct_includes: Vec<String>,
}

impl IncludeGraphNode {
    /// Check if this node represents a translation unit (TU)
    pub fn is_translation_unit(&self) -> bool {
        (self.flags & 0x01) != 0 // IsTU flag
    }

    /// Check if this node had compilation errors
    pub fn had_errors(&self) -> bool {
        (self.flags & 0x02) != 0 // HadErrors flag
    }
}

/// Parsed data from a clangd index file
#[derive(Debug, Clone)]
pub struct IdxFileData {
    /// Format version of the index file
    pub format_version: u32,
    /// Include graph nodes from the srcs chunk
    pub include_graph: Vec<IncludeGraphNode>,
    /// String table for resolving string indices
    pub(crate) string_table: Vec<String>,
}

impl IdxFileData {
    /// Get translation units from the include graph
    pub fn translation_units(&self) -> Vec<&IncludeGraphNode> {
        self.include_graph
            .iter()
            .filter(|node| node.is_translation_unit())
            .collect()
    }

    /// Find a node by URI
    pub fn find_node_by_uri(&self, uri: &str) -> Option<&IncludeGraphNode> {
        self.include_graph.iter().find(|node| node.uri == uri)
    }
}

/// Internal representation of a RIFF chunk
struct RiffChunk {
    id: [u8; 4],
    data: Vec<u8>,
}

/// Main parser for clangd index files
pub struct IdxParser {
    chunks: HashMap<String, RiffChunk>,
}

impl IdxParser {
    /// Parse an index file from raw bytes
    pub fn parse(data: &[u8]) -> Result<IdxFileData, IdxParseError> {
        let mut parser = Self::new();
        parser.parse_riff_container(data)?;
        parser.extract_index_data()
    }

    /// Create a new parser instance
    fn new() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }

    /// Parse the RIFF container structure
    fn parse_riff_container(&mut self, data: &[u8]) -> Result<(), IdxParseError> {
        if data.len() < 12 {
            return Err(IdxParseError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "File too small for RIFF header",
            )));
        }

        let mut cursor = Cursor::new(data);

        // Read RIFF header
        let mut magic = [0u8; 4];
        cursor.read_exact(&mut magic)?;
        if &magic != b"RIFF" {
            return Err(IdxParseError::InvalidMagic);
        }

        let file_size = read_u32_le(&mut cursor)?;

        let mut type_id = [0u8; 4];
        cursor.read_exact(&mut type_id)?;
        if &type_id != b"CdIx" {
            return Err(IdxParseError::InvalidType);
        }

        // Parse chunks
        let end_pos = 8 + file_size as u64;
        while cursor.position() < end_pos {
            if cursor.position() + 8 > end_pos {
                break; // Not enough space for chunk header
            }

            let mut chunk_id = [0u8; 4];
            cursor.read_exact(&mut chunk_id)?;

            let chunk_size = read_u32_le(&mut cursor)?;

            if cursor.position() + chunk_size as u64 > end_pos {
                break; // Chunk extends beyond file
            }

            let mut chunk_data = vec![0u8; chunk_size as usize];
            cursor.read_exact(&mut chunk_data)?;

            let chunk_id_str = String::from_utf8_lossy(&chunk_id).into_owned();
            self.chunks.insert(
                chunk_id_str,
                RiffChunk {
                    id: chunk_id,
                    data: chunk_data,
                },
            );

            // Skip padding to even boundary
            if chunk_size % 2 == 1 {
                let mut padding = [0u8; 1];
                let _ = cursor.read_exact(&mut padding); // Ignore error for EOF
            }
        }

        Ok(())
    }

    /// Extract structured data from parsed chunks
    fn extract_index_data(&self) -> Result<IdxFileData, IdxParseError> {
        // Parse format version from meta chunk
        let format_version = self.parse_meta_chunk()?;

        // Parse string table from stri chunk
        let string_table = self.parse_string_table()?;

        // Parse include graph from srcs chunk
        let include_graph = self.parse_srcs_chunk(&string_table)?;

        Ok(IdxFileData {
            format_version,
            include_graph,
            string_table,
        })
    }

    /// Parse the meta chunk to get format version
    fn parse_meta_chunk(&self) -> Result<u32, IdxParseError> {
        let chunk = self
            .chunks
            .get("meta")
            .ok_or_else(|| IdxParseError::MissingChunk("meta".to_string()))?;

        if chunk.data.len() < 4 {
            return Err(IdxParseError::CorruptedChunk(
                "meta chunk too small".to_string(),
            ));
        }

        let version =
            u32::from_le_bytes([chunk.data[0], chunk.data[1], chunk.data[2], chunk.data[3]]);

        if !(12..=20).contains(&version) {
            return Err(IdxParseError::UnsupportedVersion(version));
        }

        Ok(version)
    }

    /// Parse the string table from stri chunk
    fn parse_string_table(&self) -> Result<Vec<String>, IdxParseError> {
        let chunk = self
            .chunks
            .get("stri")
            .ok_or_else(|| IdxParseError::MissingChunk("stri".to_string()))?;

        if chunk.data.len() < 4 {
            return Err(IdxParseError::CorruptedChunk(
                "stri chunk too small".to_string(),
            ));
        }

        let uncompressed_size =
            u32::from_le_bytes([chunk.data[0], chunk.data[1], chunk.data[2], chunk.data[3]]);

        let string_data_vec;
        let string_data: &[u8] = if uncompressed_size == 0 {
            // Raw data follows
            &chunk.data[4..]
        } else {
            // zlib compressed data
            let compressed_data = &chunk.data[4..];

            use std::io::Read;
            let mut decoder = flate2::read::ZlibDecoder::new(compressed_data);
            string_data_vec = {
                let mut decompressed = Vec::new();
                decoder
                    .read_to_end(&mut decompressed)
                    .map_err(|e| IdxParseError::Decompression(e.to_string()))?;

                if decompressed.len() != uncompressed_size as usize {
                    return Err(IdxParseError::Decompression(format!(
                        "Size mismatch: expected {}, got {}",
                        uncompressed_size,
                        decompressed.len()
                    )));
                }

                decompressed
            };
            &string_data_vec
        };

        Self::parse_string_data(string_data)
    }

    /// Parse null-terminated strings from raw data
    fn parse_string_data(data: &[u8]) -> Result<Vec<String>, IdxParseError> {
        let mut strings = Vec::new();
        let mut current = Vec::new();

        for &byte in data {
            if byte == 0 {
                let string = String::from_utf8(current)?;
                strings.push(string);
                current = Vec::new();
            } else {
                current.push(byte);
            }
        }

        // Handle case where data doesn't end with null
        if !current.is_empty() {
            let string = String::from_utf8(current)?;
            strings.push(string);
        }

        // Ensure empty string is at index 0
        if strings.is_empty() || !strings[0].is_empty() {
            strings.insert(0, String::new());
        }

        Ok(strings)
    }

    /// Parse the srcs chunk to get include graph
    fn parse_srcs_chunk(
        &self,
        string_table: &[String],
    ) -> Result<Vec<IncludeGraphNode>, IdxParseError> {
        let chunk = match self.chunks.get("srcs") {
            Some(chunk) => chunk,
            None => return Ok(Vec::new()), // srcs chunk is optional
        };

        let mut cursor = Cursor::new(chunk.data.as_slice());
        let mut nodes = Vec::new();

        while cursor.position() < chunk.data.len() as u64 {
            // Read flags
            let mut flags_buf = [0u8; 1];
            if cursor.read_exact(&mut flags_buf).is_err() {
                break; // End of data
            }
            let flags = flags_buf[0];

            // Read URI index
            let uri_idx = read_varint(&mut cursor)?;
            let uri = string_table
                .get(uri_idx as usize)
                .unwrap_or(&String::new())
                .clone();

            // Read digest (8 bytes)
            let mut digest = [0u8; 8];
            cursor.read_exact(&mut digest)?;

            // Read direct includes count
            let include_count = read_varint(&mut cursor)?;
            let mut direct_includes = Vec::new();

            for _ in 0..include_count {
                let include_idx = read_varint(&mut cursor)?;
                let include_path = string_table
                    .get(include_idx as usize)
                    .unwrap_or(&String::new())
                    .clone();
                direct_includes.push(include_path);
            }

            nodes.push(IncludeGraphNode {
                flags,
                uri,
                digest,
                direct_includes,
            });
        }

        Ok(nodes)
    }
}

/// Read a little-endian u32 from a cursor
fn read_u32_le(cursor: &mut Cursor<&[u8]>) -> Result<u32, std::io::Error> {
    let mut bytes = [0u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

/// Read a variable-length integer from a cursor
fn read_varint(cursor: &mut Cursor<&[u8]>) -> Result<u32, std::io::Error> {
    let mut result = 0u32;
    let mut shift = 0;

    loop {
        let mut byte = [0u8; 1];
        cursor.read_exact(&mut byte)?;
        let b = byte[0];

        result |= ((b & 0x7F) as u32) << shift;

        if (b & 0x80) == 0 {
            break;
        }

        shift += 7;
        if shift >= 35 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Varint too large",
            ));
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_decoding() {
        // Test cases from clangd spec
        let test_cases = vec![
            (vec![0x1A], 0x1A),
            (vec![0x9A, 0x2F], 6042), // 26 + (47 << 7) = 26 + 6016 = 6042
        ];

        for (bytes, expected) in test_cases {
            let mut cursor = Cursor::new(bytes.as_slice());
            let result = read_varint(&mut cursor).unwrap();
            assert_eq!(result, expected, "Failed for bytes: {:?}", bytes);
        }
    }

    #[test]
    fn test_include_graph_node_flags() {
        let node = IncludeGraphNode {
            flags: 0x01, // IsTU flag set
            uri: "main.cpp".to_string(),
            digest: [0; 8],
            direct_includes: vec![],
        };

        assert!(node.is_translation_unit());
        assert!(!node.had_errors());

        let node_with_errors = IncludeGraphNode {
            flags: 0x03, // IsTU + HadErrors flags set
            uri: "broken.cpp".to_string(),
            digest: [0; 8],
            direct_includes: vec![],
        };

        assert!(node_with_errors.is_translation_unit());
        assert!(node_with_errors.had_errors());
    }

    #[test]
    fn test_idx_file_data_translation_units() {
        let nodes = vec![
            IncludeGraphNode {
                flags: 0x01, // IsTU
                uri: "main.cpp".to_string(),
                digest: [1; 8],
                direct_includes: vec![],
            },
            IncludeGraphNode {
                flags: 0x00, // Not TU
                uri: "header.h".to_string(),
                digest: [2; 8],
                direct_includes: vec![],
            },
            IncludeGraphNode {
                flags: 0x01, // IsTU
                uri: "test.cpp".to_string(),
                digest: [3; 8],
                direct_includes: vec![],
            },
        ];

        let idx_data = IdxFileData {
            format_version: 19,
            include_graph: nodes,
            string_table: vec![],
        };

        let tus = idx_data.translation_units();
        assert_eq!(tus.len(), 2);
        assert_eq!(tus[0].uri, "main.cpp");
        assert_eq!(tus[1].uri, "test.cpp");
    }

    #[test]
    fn test_find_node_by_uri() {
        let nodes = vec![
            IncludeGraphNode {
                flags: 0x01,
                uri: "main.cpp".to_string(),
                digest: [1; 8],
                direct_includes: vec![],
            },
            IncludeGraphNode {
                flags: 0x00,
                uri: "header.h".to_string(),
                digest: [2; 8],
                direct_includes: vec![],
            },
        ];

        let idx_data = IdxFileData {
            format_version: 19,
            include_graph: nodes,
            string_table: vec![],
        };

        let found = idx_data.find_node_by_uri("header.h");
        assert!(found.is_some());
        assert_eq!(found.unwrap().digest, [2; 8]);

        let not_found = idx_data.find_node_by_uri("nonexistent.cpp");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_riff_container_parsing() {
        // Create a minimal RIFF container for testing
        let mut riff_data = Vec::new();

        // RIFF header
        riff_data.extend_from_slice(b"RIFF");
        riff_data.extend_from_slice(&16u32.to_le_bytes()); // file size (excluding first 8 bytes)
        riff_data.extend_from_slice(b"CdIx");

        // Add a simple chunk
        riff_data.extend_from_slice(b"test");
        riff_data.extend_from_slice(&4u32.to_le_bytes()); // chunk size
        riff_data.extend_from_slice(b"data");

        let mut parser = IdxParser::new();
        let result = parser.parse_riff_container(&riff_data);
        assert!(result.is_ok());
        assert!(parser.chunks.contains_key("test"));
        assert_eq!(parser.chunks["test"].data, b"data");
    }

    #[test]
    fn test_riff_container_invalid_magic() {
        let mut invalid_data = Vec::new();
        invalid_data.extend_from_slice(b"WRONG");
        invalid_data.extend_from_slice(&16u32.to_le_bytes());
        invalid_data.extend_from_slice(b"CdIx");

        let mut parser = IdxParser::new();
        let result = parser.parse_riff_container(&invalid_data);
        assert!(matches!(result, Err(IdxParseError::InvalidMagic)));
    }

    #[test]
    fn test_riff_container_invalid_type() {
        let mut invalid_data = Vec::new();
        invalid_data.extend_from_slice(b"RIFF");
        invalid_data.extend_from_slice(&16u32.to_le_bytes());
        invalid_data.extend_from_slice(b"WRONG");

        let mut parser = IdxParser::new();
        let result = parser.parse_riff_container(&invalid_data);
        assert!(matches!(result, Err(IdxParseError::InvalidType)));
    }

    #[test]
    fn test_string_table_parsing() {
        // Test raw (uncompressed) string table
        let mut stri_data = Vec::new();
        stri_data.extend_from_slice(&0u32.to_le_bytes()); // uncompressed_size = 0 (raw data)
        stri_data.extend_from_slice(b"\0hello\0world\0");

        let strings = IdxParser::parse_string_data(&stri_data[4..]).unwrap();
        assert_eq!(strings.len(), 3);
        assert_eq!(strings[0], ""); // Empty string at index 0
        assert_eq!(strings[1], "hello");
        assert_eq!(strings[2], "world");
    }

    #[test]
    fn test_varint_edge_cases() {
        // Test single byte maximum value
        let single_byte_max = vec![0x7F]; // 127
        let mut cursor = Cursor::new(single_byte_max.as_slice());
        assert_eq!(read_varint(&mut cursor).unwrap(), 127);

        // Test two bytes minimum (128)
        let two_byte_min = vec![0x80, 0x01]; // 128
        let mut cursor = Cursor::new(two_byte_min.as_slice());
        assert_eq!(read_varint(&mut cursor).unwrap(), 128);

        // Test larger value
        let larger_value = vec![0xF8, 0xAC, 0xD1, 0x91, 0x01]; // 0x12345678
        let mut cursor = Cursor::new(larger_value.as_slice());
        assert_eq!(read_varint(&mut cursor).unwrap(), 0x12345678);
    }

    #[test]
    fn test_parse_error_creation() {
        let parse_error = IdxParseError::CorruptedChunk("test chunk corrupted".to_string());
        assert!(parse_error.to_string().contains("test chunk corrupted"));

        let version_error = IdxParseError::UnsupportedVersion(99);
        assert!(version_error.to_string().contains("99"));
    }
}
