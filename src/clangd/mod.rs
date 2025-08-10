//! Clangd Session Management Module
//!
//! This module provides clean session management for clangd processes with LSP clients.
//! It uses lsp_v2 components directly for process management, transport, and LSP communication.

#![allow(dead_code)]
//!
//! # Architecture
//!
//! - **ClangdSession**: Manages lifecycle of clangd process + LSP client
//! - **ClangdConfig**: Configuration with builder pattern and validation
//! - **Error Types**: Comprehensive error handling with context preservation
//!
//! # Usage
//!
//! ```rust
//! use clangd::{ClangdSession, ClangdConfigBuilder};
//!
//! // Build configuration
//! let config = ClangdConfigBuilder::new()
//!     .working_directory("/path/to/project")
//!     .build_directory("/path/to/build")
//!     .build()?;
//!
//! // Create session
//! let session = ClangdSession::new(config).await?;
//!
//! // Use LSP client
//! let client = session.client();
//! // Make LSP requests...
//!
//! // Clean shutdown
//! session.close().await?;
//! ```

pub mod config;
pub mod error;
pub mod file_manager;
pub mod index;
pub mod session;
pub mod testing;
pub mod version;

// Re-export main types for convenience
#[allow(unused_imports)]
pub use config::{ClangdConfig, ClangdConfigBuilder};
#[allow(unused_imports)]
pub use error::{ClangdConfigError, ClangdSessionError};
#[allow(unused_imports)]
pub use file_manager::ClangdFileManager;
#[allow(unused_imports)]
pub use index::ProjectIndex;
#[allow(unused_imports)]
pub use session::{ClangdSession, ClangdSessionTrait};
#[allow(unused_imports)]
pub use version::{ClangdVersion, ClangdVersionError};

// Re-export testing utilities when in test mode
#[cfg(test)]
#[allow(unused_imports)]
pub use testing::{MockClangdSession, MockProjectWorkspace};
