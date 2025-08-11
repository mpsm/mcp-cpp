//! Project component management module
//!
//! This module provides an extensible architecture for handling different build systems
//! through a provider pattern. Each provider can detect and parse project components
//! for their respective build system.

pub mod cmake_provider;
pub mod compilation_database;
pub mod component;
pub mod error;
pub mod meson_provider;
pub mod provider;
pub mod scanner;
pub mod workspace;
pub mod workspace_session;

#[allow(unused_imports)]
pub use cmake_provider::CmakeProvider;
#[allow(unused_imports)]
pub use compilation_database::{CompilationDatabase, CompilationDatabaseError};
#[allow(unused_imports)]
pub use component::ProjectComponent;
#[allow(unused_imports)]
pub use error::ProjectError;
#[allow(unused_imports)]
pub use meson_provider::MesonProvider;
#[allow(unused_imports)]
pub use provider::{ProjectComponentProvider, ProjectProviderRegistry};
#[allow(unused_imports)]
pub use scanner::{ProjectScanner, ScanOptions};
#[allow(unused_imports)]
pub use workspace::ProjectWorkspace;
#[allow(unused_imports)]
pub use workspace_session::WorkspaceSession;

// Suppress unused warnings since this module is not integrated yet
#[allow(unused_imports)]
use cmake_provider::CmakeProvider as _UnusedCmakeProvider;
#[allow(unused_imports)]
use meson_provider::MesonProvider as _UnusedMesonProvider;
#[allow(unused_imports)]
use provider::ProjectProviderRegistry as _UnusedRegistry;
#[allow(unused_imports)]
use scanner::{ProjectScanner as _UnusedScanner, ScanOptions as _UnusedScanOptions};
#[allow(unused_imports)]
use workspace::ProjectWorkspace as _UnusedProjectWorkspace;
