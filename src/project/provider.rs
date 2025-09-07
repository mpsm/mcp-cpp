use crate::project::{ProjectComponent, ProjectError};
use std::path::Path;

/// Trait for project component providers
///
/// Each provider implements detection and parsing logic for a specific build system.
/// The provider should return None if the directory is not applicable to its build system,
/// and Some(component) if it successfully detects and parses a project.
pub trait ProjectComponentProvider: Send + Sync {
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
pub struct ProjectProviderRegistry {
    providers: Vec<Box<dyn ProjectComponentProvider>>,
}

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
}

impl Default for ProjectProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct MockProvider {
        name: String,
    }

    impl ProjectComponentProvider for MockProvider {
        fn scan_path(&self, _path: &Path) -> Result<Option<ProjectComponent>, ProjectError> {
            Ok(None)
        }
    }

    #[allow(dead_code)]
    impl MockProvider {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }
}
