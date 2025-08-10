//! I/O layer - Generic abstractions for process management and transport
//!
//! This module provides fundamental I/O abstractions that are not specific to any protocol:
//!
//! - **Transport**: Pure I/O layer for bidirectional message exchange
//! - **Process**: External process lifecycle management with stdio integration
//!
//! These abstractions can be used by any protocol layer (LSP, MCP, etc.)

pub mod process;
pub mod transport;

// Re-export main types for convenience
pub use process::{
    ChildProcessManager, ProcessExitEvent, ProcessExitHandler, ProcessManager, ProcessState,
    StderrMonitor, StopMode,
};
pub use transport::{MockTransport, StdioTransport, Transport};
