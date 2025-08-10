//! LSP v2 - Clean layered architecture for Language Server Protocol communication
//!
//! This module provides a clean, testable, and extensible LSP client implementation
//! with proper separation of concerns:
//!
//! - **Framing**: LSP message framing (Content-Length headers)  
//! - **Protocol**: JSON-RPC 2.0 protocol implementation
//! - **Client**: High-level typed LSP API using lsp-types
//! - **Testing**: Mock implementations for comprehensive testing
//!
//! This module uses the generic I/O layer (`crate::io`) for transport and process management.

pub mod client;
pub mod framing;
pub mod protocol;
pub mod testing;
pub mod traits;

//
// Example usage with direct component coordination:
//
// ```rust
// use mcp_cpp::io::{ChildProcessManager, StdioTransport};
// use mcp_cpp::lsp_v2::LspClient;
//
// // Start process
// let mut process = ChildProcessManager::new("clangd".to_string(), args, Some(working_dir));
// process.start().await?;
//
// // Create transport and client
// let transport = process.create_stdio_transport()?;
// let mut client = LspClient::new(transport);
//
// // Initialize LSP
// client.initialize(Some("file:///path/to/project".to_string())).await?;
//
// // Make LSP requests...
//
// // Clean shutdown
// client.shutdown().await?;
// process.stop(StopMode::Graceful).await?;
// ```

// Re-export main types for convenience
#[allow(unused_imports)]
pub use client::{LspClient, LspError};
#[allow(unused_imports)]
pub use traits::LspClientTrait;

// Re-export I/O types for convenience (these are now in crate::io)
#[allow(unused_imports)]
pub use crate::io::{
    ChildProcessManager, MockTransport, ProcessExitEvent, ProcessExitHandler, ProcessManager,
    ProcessState, StderrMonitor, StdioTransport, StopMode, Transport,
};
