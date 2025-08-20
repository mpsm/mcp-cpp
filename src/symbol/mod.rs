//! Symbol abstraction module
//!
//! Provides a clean Symbol abstraction that uses std types and lsp_types::SymbolKind
//! while enabling conversion from LSP WorkspaceSymbol responses.

mod location;
#[allow(clippy::module_inception)]
mod symbol;

pub use location::FileLocation;
pub use symbol::Symbol;
