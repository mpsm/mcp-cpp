//! Project component management module
//!
//! This module provides an extensible architecture for handling different build systems
//! through a provider pattern. Each provider can detect and parse project components
//! for their respective build system.

pub mod cmake_provider;
pub mod compilation_database;
pub mod component;
pub mod component_session;
pub mod error;
pub mod index;
pub mod meson_provider;
pub mod provider;
pub mod scanner;
pub mod workspace;
pub mod workspace_session;

pub use cmake_provider::CmakeProvider;

pub use compilation_database::{CompilationDatabase, CompilationDatabaseError};

pub use component::ProjectComponent;

pub use component_session::ComponentSession;

pub use error::ProjectError;

pub use meson_provider::MesonProvider;

pub use provider::{ProjectComponentProvider, ProjectProviderRegistry};

pub use scanner::ProjectScanner;

pub use workspace::ProjectWorkspace;

pub use workspace_session::WorkspaceSession;
