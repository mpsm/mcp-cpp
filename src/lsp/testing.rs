//! Testing utilities and mock implementations
//!
//! Provides mock implementations of all traits for comprehensive
//! testing of LSP client functionality.

// Re-export MockTransport from transport module for convenience
#[allow(unused_imports)]
pub use crate::io::transport::MockTransport;

// Re-export MockProcessManager from process module for convenience
#[cfg(test)]
#[allow(unused_imports)]
pub use crate::io::process::MockProcessManager;

// Re-export the mockall-generated mock for LspClientTrait
#[cfg(test)]
pub use crate::lsp::traits::MockLspClientTrait;
