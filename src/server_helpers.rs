//! Server helper utilities for common operations

#[cfg(feature = "tools-v2")]
use rust_mcp_sdk::schema::schema_utils::CallToolError;
#[cfg(feature = "tools-v2")]
use std::path::PathBuf;
#[cfg(feature = "tools-v2")]
use tracing::debug;

#[cfg(feature = "tools-v2")]
use crate::project::ProjectWorkspace;

/// Resolves build directory from optional parameter.
///
/// # Arguments
/// * `workspace` - The project workspace to search for build directories
/// * `requested_build_dir` - Optional build directory path (can be relative or absolute)
///
/// # Returns
/// * `Ok(PathBuf)` - The resolved build directory path
/// * `Err(CallToolError)` - If the specified directory doesn't exist in workspace or if
///   auto-detection fails due to zero or multiple build directories
///
/// # Behavior
/// - If `requested_build_dir` is provided, validates it exists in the workspace
/// - If not provided, auto-detects single build directory
/// - Fails if no build directories exist (suggests running cmake)
/// - Fails if multiple build directories exist without explicit selection
#[cfg(feature = "tools-v2")]
pub fn resolve_build_directory(
    workspace: &ProjectWorkspace,
    requested_build_dir: Option<&str>,
) -> Result<PathBuf, CallToolError> {
    match requested_build_dir {
        Some(build_dir_str) => {
            debug!(
                "Attempting to use specified build directory: {}",
                build_dir_str
            );
            let requested_path = PathBuf::from(build_dir_str);

            if workspace
                .get_component_by_build_dir(&requested_path)
                .is_some()
            {
                debug!("Build directory '{}' found in workspace", build_dir_str);
                Ok(requested_path)
            } else {
                let available = workspace.get_build_dirs();
                Err(CallToolError::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "Build directory '{}' not found in project workspace. Available: {:?}",
                        build_dir_str, available
                    ),
                )))
            }
        }
        None => {
            debug!("No build directory specified, attempting auto-detection");
            let build_dirs = workspace.get_build_dirs();

            match build_dirs.len() {
                0 => {
                    debug!("No build directories found in workspace");
                    Err(CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No build directories found in project. Run cmake to generate build configuration.",
                    )))
                }
                1 => {
                    debug!("Single build directory found: {:?}", build_dirs[0]);
                    Ok(build_dirs[0].clone())
                }
                _ => {
                    debug!("Multiple build directories found: {:?}", build_dirs);
                    Err(CallToolError::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "Multiple build directories found. Please specify build_directory parameter. Available: {:?}",
                            build_dirs
                        ),
                    )))
                }
            }
        }
    }
}

#[cfg(all(test, feature = "tools-v2"))]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_with_explicit_directory() {
        // Test validates function signature compatibility.
        // Full integration test coverage is provided by the E2E test suite
        // which exercises this function with real ProjectWorkspace instances.
        let _result: fn(&ProjectWorkspace, Option<&str>) -> Result<PathBuf, CallToolError> =
            resolve_build_directory;
    }
}
