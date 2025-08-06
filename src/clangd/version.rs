use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClangdVersionError {
    #[error("Failed to execute clangd: {0}")]
    ExecutionFailed(String),
    #[error("Failed to parse clangd version output")]
    ParseFailed,
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClangdVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub variant: Option<String>,
    pub date: Option<String>,
}

impl ClangdVersion {
    /// Detect clangd version by running --version command
    pub fn detect(clangd_path: &Path) -> Result<Self, ClangdVersionError> {
        let output = Command::new(clangd_path)
            .arg("--version")
            .output()
            .map_err(|e| ClangdVersionError::ExecutionFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(ClangdVersionError::ExecutionFailed(
                "clangd --version failed".to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_version_output(&stdout)
    }

    fn parse_version_output(output: &str) -> Result<Self, ClangdVersionError> {
        // Look for line containing "clangd version"
        let version_line = output
            .lines()
            .find(|line| line.contains("clangd version"))
            .ok_or(ClangdVersionError::ParseFailed)?;

        // Extract version string after "clangd version "
        let version_start = version_line
            .find("clangd version ")
            .ok_or(ClangdVersionError::ParseFailed)?
            + "clangd version ".len();

        let version_str = &version_line[version_start..];

        // Parse version components
        // Format examples:
        // "14.0.0-1ubuntu1.1"
        // "18.1.8 (++20240731024944+3b5b5c1ec4a3-1~exp1~20240731145000.144)"
        // "20.1.8 (++20250708082409+6fb913d3e2ec-1~exp1~20250708202428.132)"

        // Check for date in parentheses
        let (version_part, date) = if let Some(paren_idx) = version_str.find(" (") {
            let date_part = &version_str[paren_idx + 2..];
            let date = date_part.trim_end_matches(')').to_string();
            (&version_str[..paren_idx], Some(date))
        } else {
            (version_str, None)
        };

        // First split on dots for major.minor.patch
        let mut dot_parts = version_part.splitn(3, '.');

        let major = dot_parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or_else(|| ClangdVersionError::InvalidFormat("major version".to_string()))?;

        let minor = dot_parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or_else(|| ClangdVersionError::InvalidFormat("minor version".to_string()))?;

        // The patch part might contain variant after dash
        let patch_part = dot_parts
            .next()
            .ok_or_else(|| ClangdVersionError::InvalidFormat("patch version".to_string()))?;

        // Split patch part on dash to separate patch from variant
        let mut dash_parts = patch_part.splitn(2, '-');

        let patch = dash_parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or_else(|| ClangdVersionError::InvalidFormat("patch version".to_string()))?;

        // Collect variant without leading dash
        let variant = dash_parts.next().map(|s| s.to_string());

        Ok(ClangdVersion {
            major,
            minor,
            patch,
            variant,
            date,
        })
    }

    /// Get the format version for index files based on clangd version
    pub fn index_format_version(&self) -> u32 {
        match self.major {
            10 => 12,
            11 => 13,
            12 | 13 => 16,
            14 | 15 => 17,
            16 | 17 => 18,
            18 | 19 => 19,
            20 => 20,
            _ => 20, // Default to latest known version
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_simple() {
        let output = "Ubuntu clangd version 14.0.0-1ubuntu1.1\nFeatures: linux+grpc\nPlatform: x86_64-pc-linux-gnu\n";
        let version = ClangdVersion::parse_version_output(output).unwrap();
        assert_eq!(version.major, 14);
        assert_eq!(version.minor, 0);
        assert_eq!(version.patch, 0);
        assert_eq!(version.variant, Some("1ubuntu1.1".to_string()));
        assert_eq!(version.date, None);
    }

    #[test]
    fn test_parse_version_with_date() {
        let output = "Ubuntu clangd version 18.1.8 (++20240731024944+3b5b5c1ec4a3-1~exp1~20240731145000.144)\nFeatures: linux+grpc\nPlatform: x86_64-pc-linux-gnu\n";
        let version = ClangdVersion::parse_version_output(output).unwrap();
        assert_eq!(version.major, 18);
        assert_eq!(version.minor, 1);
        assert_eq!(version.patch, 8);
        assert_eq!(version.variant, None);
        assert_eq!(
            version.date,
            Some("++20240731024944+3b5b5c1ec4a3-1~exp1~20240731145000.144".to_string())
        );
    }

    #[test]
    fn test_index_format_version() {
        let version = ClangdVersion {
            major: 18,
            minor: 1,
            patch: 8,
            variant: None,
            date: None,
        };
        assert_eq!(version.index_format_version(), 19);

        let version = ClangdVersion {
            major: 14,
            minor: 0,
            patch: 0,
            variant: None,
            date: None,
        };
        assert_eq!(version.index_format_version(), 17);
    }
}
