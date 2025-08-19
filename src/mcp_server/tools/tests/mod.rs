//! Tests for MCP Server Tools
//!
//! This module contains tests for various MCP server tools including
//! member analysis, symbol search, and project tools.

#[cfg(all(test, feature = "clangd-integration-tests"))]
pub mod call_hierarchy_tests;
#[cfg(all(test, feature = "clangd-integration-tests"))]
pub mod definitions_tests;
#[cfg(all(test, feature = "clangd-integration-tests"))]
pub mod examples_tests;
#[cfg(all(test, feature = "clangd-integration-tests"))]
pub mod hover_tests;
#[cfg(all(test, feature = "clangd-integration-tests"))]
pub mod member_tests;
#[cfg(all(test, feature = "clangd-integration-tests"))]
pub mod symbol_resolution_tests;
#[cfg(all(test, feature = "clangd-integration-tests"))]
pub mod type_hierarchy_tests;
