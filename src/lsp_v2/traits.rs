//! LSP traits and abstractions
//!
//! Provides trait abstractions that enable polymorphic usage of LSP components
//! while maintaining type safety and testability.

use async_trait::async_trait;
use std::fmt;

// ============================================================================
// LSP Client Trait Abstraction
// ============================================================================

/// Trait abstraction for minimal LSP client functionality
///
/// Enables polymorphic usage of real LspClient and MockLspClient while
/// maintaining type safety and proper error handling.
///
/// This is a minimal trait focused on the core functionality needed by LSP sessions.
/// For more comprehensive LSP operations, consumers should use the concrete client types directly.
#[async_trait]
#[allow(dead_code)]
pub trait LspClientTrait: Send + Sync + fmt::Debug {
    /// LSP client error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Check if client is running/initialized
    fn is_running(&self) -> bool;

    /// Check if client is initialized (ready for LSP operations)
    fn is_initialized(&self) -> bool;
}
