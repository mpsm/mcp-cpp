//! MCP Server implementation module
//!
//! This module contains the complete MCP (Model Context Protocol) server implementation
//! for C++ code analysis, including the server handler, helper utilities, and all
//! available tools for semantic analysis.

pub mod server;
pub mod server_helpers;
pub mod tools {
    pub mod analyze_symbols;
    pub mod project_tools;
    pub mod search_symbols;
    pub mod utils;
}

// Re-export main components for easier access
pub use server::CppServerHandler;
