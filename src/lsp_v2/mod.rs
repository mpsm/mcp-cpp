//! LSP v2 - Clean layered architecture for Language Server Protocol communication
//!
//! This module provides a clean, testable, and extensible LSP client implementation
//! with proper separation of concerns:
//!
//! - **Transport**: Pure I/O layer for sending/receiving messages
//! - **Framing**: LSP message framing (Content-Length headers)  
//! - **Process**: External process lifecycle management
//! - **Protocol**: JSON-RPC 2.0 protocol implementation
//! - **Client**: High-level typed LSP API using lsp-types
//! - **Orchestrator**: Coordinates process, transport, and client lifecycle
//! - **Testing**: Mock implementations for comprehensive testing

pub mod client;
pub mod framing;
pub mod orchestrator;
pub mod process;
pub mod protocol;
pub mod testing;
pub mod transport;

//
// The orchestrator is the recommended entry point for most use cases:
//
// ```rust
// use mcp_cpp::lsp_v2::orchestrator::{StandardLspOrchestrator, LspOrchestrator};
//
// let mut orchestrator = StandardLspOrchestrator::new(
//     "clangd".to_string(),
//     vec!["--compile-commands-dir=/path/to/build".to_string()]
// );
//
// // Start: process → transport → client → initialize
// orchestrator.start(Some("file:///path/to/project".to_string())).await?;
//
// // Use the LSP client
// if let Some(client) = orchestrator.client_mut() {
//     // Make LSP requests...
// }
//
// // Clean shutdown
// orchestrator.shutdown().await?;
// ```

// Re-export main types for convenience
#[allow(unused_imports)]
pub use client::{LspClient, LspError};
#[allow(unused_imports)]
pub use orchestrator::{
    LspOrchestrator, OrchestratorError, StandardLspOrchestrator,
};
#[allow(unused_imports)]
pub use process::{
    ChildProcessManager, ProcessExitEvent, ProcessExitHandler, ProcessManager, ProcessState,
    StderrMonitor, StopMode,
};
#[allow(unused_imports)]
pub use transport::{MockTransport, StdioTransport, Transport};
