//! Clangd index management module
//!
//! This module provides functionality for working with clangd's background index files.
//! It maps source files to their corresponding index files without parsing the index content.
//! Also provides indexing progress monitoring capabilities.

pub mod hash;
pub mod latch;
pub mod monitor;
pub mod progress_events;
pub mod project_index;

pub use latch::{IndexLatch, LatchError};
pub use monitor::IndexMonitor;
pub use progress_events::ProgressEvent;
