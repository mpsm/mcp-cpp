//! Clangd index management module
//!
//! This module provides functionality for working with clangd's background index files.
//! It maps source files to their corresponding index files without parsing the index content.

pub mod hash;
pub mod project_index;

pub use project_index::ProjectIndex;
