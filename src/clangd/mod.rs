//! Clangd Session Management Module
//!
//! This module provides clean session management for clangd processes with LSP clients.
//! It uses lsp components directly for process management, transport, and LSP communication.

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
pub mod log_monitor;
pub mod session;
pub mod session_builder;
pub mod version;

#[cfg(test)]
pub mod testing;

pub use crate::clangd::config::ClangdConfigBuilder;
pub use crate::clangd::session::ClangdSession;
pub use crate::clangd::session_builder::ClangdSessionBuilder;
