//! Project index management module
//!
//! This module provides comprehensive index management for C++ projects, including:
//! - Reading clangd index files with automatic staleness detection
//! - Tracking index state for compilation database entries
//! - Storage abstraction for different index backends
//! - Component-level index monitoring and progress tracking
//!
//! The module is split into focused components:
//! - `reader`: IndexReader for reading and validating index files
//! - `state`: IndexState for tracking compilation database indexing status
//! - `storage`: Storage trait and implementations for index backends
//! - `component_monitor`: ComponentIndexMonitor for managing index state per build directory

pub mod component_monitor;
#[allow(dead_code)]
pub mod reader;
#[allow(dead_code)]
pub mod session;
#[allow(dead_code)]
pub mod state;
#[allow(dead_code)]
pub mod storage;

// Public exports
pub use component_monitor::{ComponentIndexMonitor, ComponentIndexState};
pub use session::IndexSession;

#[cfg(all(test, feature = "clangd-integration-tests"))]
mod integration_tests;
