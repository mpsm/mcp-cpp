//! I/O layer - Generic abstractions for process management and transport
//!
//! This module provides fundamental I/O abstractions that are not specific to any protocol:
//!
//! - **Transport**: Pure I/O layer for bidirectional message exchange
//! - **Process**: External process lifecycle management with stdio integration
//! - **File Buffer**: UTF-8 file content management with position-based text extraction
//!
//! These abstractions can be used by any protocol layer (LSP, MCP, etc.)

pub mod file_buffer;
pub mod process;
pub mod transport;

// Re-export main types for convenience
#[allow(dead_code, unused_imports)]
pub use file_buffer::{
    FileBuffer, FileBufferError, FileBufferManager, FilePosition, FileSystemTrait,
    RealFileBufferManager, RealFileSystem,
};
pub use process::{ChildProcessManager, ProcessManager, StderrMonitor, StopMode};
pub use transport::StdioTransport;
