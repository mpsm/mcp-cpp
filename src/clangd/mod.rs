//! Clangd Session Management Module
//!
//! This module provides clean session management for clangd processes with LSP clients.
//! It uses lsp_v2 components directly for process management, transport, and LSP communication.

#![allow(dead_code)]
//!
//! # Architecture
//!
//! - **ClangdSession**: Manages lifecycle of clangd process + LSP client
//! - **ClangdSessionFactory**: Creates and configures sessions
//! - **ClangdConfig**: Configuration with builder pattern and validation
//! - **Error Types**: Comprehensive error handling with context preservation
//!
//! # Usage
//!
//! ```rust
//! use clangd::{ClangdSessionFactory, ClangdConfigBuilder};
//!
//! // Create factory
//! let factory = ClangdSessionFactory::new();
//!
//! // Build configuration
//! let config = ClangdConfigBuilder::new()
//!     .working_directory("/path/to/project")
//!     .build_directory("/path/to/build")
//!     .build()?;
//!
//! // Create and start session
//! let mut session = factory.create_session(config).await?;
//! session.start().await?;
//!
//! // Use LSP client
//! if let Some(client) = session.client_mut() {
//!     // Make LSP requests...
//! }
//!
//! // Clean shutdown
//! session.shutdown().await?;
//! ```

pub mod config;
pub mod error;
pub mod factory;
pub mod session;
pub mod testing;

// Re-export main types for convenience
#[allow(unused_imports)]
pub use config::{ClangdConfig, ClangdConfigBuilder};
#[allow(unused_imports)]
pub use error::{ClangdConfigError, ClangdSessionError};
#[allow(unused_imports)]
pub use factory::{ClangdSessionFactory, ClangdSessionFactoryTrait};
#[allow(unused_imports)]
pub use session::{ClangdSession, ClangdSessionTrait};

// Re-export testing utilities when in test mode
#[cfg(test)]
#[allow(unused_imports)]
pub use testing::{MockClangdSession, MockClangdSessionFactory, MockMetaProject};
