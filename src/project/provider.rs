use crate::project::{ProjectComponent, ProjectError};
use std::path::Path;

/// Trait for project component providers
///
/// Each provider implements detection and parsing logic for a specific build system.
/// The provider should return None if the directory is not applicable to its build system,
/// and Some(component) if it successfully detects and parses a project.
#[allow(dead_code)]
pub trait ProjectComponentProvider {
    /// Get the name of this provider (e.g., "cmake", "meson")
    fn name(&self) -> &str;

    /// Scan a directory and attempt to create a project component
    ///
    /// Returns:
    /// - Ok(Some(component)) if this provider can handle the directory and parsing succeeds
    /// - Ok(None) if this provider cannot handle the directory (not applicable)
    /// - Err(error) if this provider should handle the directory but parsing fails
    fn scan_path(&self, path: &Path) -> Result<Option<ProjectComponent>, ProjectError>;
}

/// Registry for managing multiple project component providers
///
/// This registry allows multiple providers to be registered and will attempt
/// to scan directories with each provider until one succeeds.
#[allow(dead_code)]
pub struct ProjectProviderRegistry {
    providers: Vec<Box<dyn ProjectComponentProvider>>,
}

#[allow(dead_code)]
impl ProjectProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a provider to the registry
    pub fn with_provider(mut self, provider: Box<dyn ProjectComponentProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Add a provider to the registry (mutable version)
    pub fn add_provider(&mut self, provider: Box<dyn ProjectComponentProvider>) {
        self.providers.push(provider);
    }

    /// Scan a directory with all registered providers
    ///
    /// Returns the first successful match from any provider.
    /// If no providers can handle the directory, returns Ok(None).
    /// If a provider can handle the directory but fails, returns the error.
    pub fn scan_directory(&self, path: &Path) -> Result<Option<ProjectComponent>, ProjectError> {
        for provider in &self.providers {
            match provider.scan_path(path)? {
                Some(component) => return Ok(Some(component)),
                None => continue,
            }
        }
        Ok(None)
    }

    /// Get the names of all registered providers
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.iter().map(|p| p.name()).collect()
    }

    /// Get the number of registered providers
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

impl Default for ProjectProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
