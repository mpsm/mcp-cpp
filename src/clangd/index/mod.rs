//! Clangd index management module
//!
//! This module provides functionality for working with clangd's background index files.
//! It maps source files to their corresponding index files without parsing the index content.
//! Also provides indexing progress monitoring capabilities.

pub mod component_index;
pub mod hash;
pub mod idx_parser;
pub mod latch;
pub mod progress_events;
pub mod progress_monitor;

pub use component_index::{ComponentIndex, IndexingSummary};
pub use latch::IndexLatch;
pub use progress_events::ProgressEvent;
pub use progress_monitor::IndexProgressMonitor;
