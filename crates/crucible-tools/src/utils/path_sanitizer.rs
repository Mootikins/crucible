//! Path sanitization utilities for security
//!
//! This module provides functions to sanitize and validate file paths
//! to prevent directory traversal attacks and ensure safe file operations.

use std::path::{Component, Path, PathBuf};
use tracing::{debug, warn};

/// Configuration for path sanitization
#[derive(Debug, Clone)]
pub struct PathSanitizerConfig {
    /// Whether to allow absolute paths
    pub allow_absolute_paths: bool,
    /// Whether to allow parent directory references (..)
    pub allow_parent_references: bool,
    /// Base directory for resolving relative paths
    pub base_directory: Option<PathBuf>,
    /// Maximum path length
    pub max_path_length: usize,
    /// Forbidden path components
    pub forbidden_components: Vec<String>,
    /// Allowed file extensions (empty = allow all)
    pub allowed_extensions: Vec<String>,
}

impl Default for PathSanitizerConfig {
    fn default() -> Self {
        Self {
            allow_absolute_paths: false,
            allow_parent_references: false,
            base_directory: None,
            max_path_length: 4096,
            forbidden_components: vec![
                ".git".to_string(),
                ".svn".to_string(),
                ".DS_Store".to_string(),
                "Thumbs.db".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".crucible".to_string(),
            ],
            allowed_extensions: vec![],
        }
    }
}

/// Path sanitizer for secure path handling
pub struct PathSanitizer {
    config: PathSanitizerConfig,
}

impl PathSanitizer {
    /// Create a new path sanitizer with default configuration
    pub fn new() -> Self {
        Self {
            config: PathSanitizerConfig::default(),
        }
    }

    /// Create a new path sanitizer with custom configuration
    pub fn with_config(config: PathSanitizerConfig) -> Self {
        Self { config }
    }

    /// Sanitize a path according to the configuration
    pub fn sanitize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, PathSanitizationError> {
        let path = path.as_ref();
        debug!("Sanitizing path: {:?}", path);

        // Check path length
        if path.as_os_str().len() > self.config.max_path_length {
            return Err(PathSanitizationError::PathTooLong {
                path: path.to_path_buf(),
                max_length: self.config.max_path_length,
            });
        }

        // Check for forbidden components
        for component in path.components() {
            if let Component::Normal(name) = component {
                if let Some(name_str) = name.to_str() {
                    if self.config.forbidden_components.contains(&name_str.to_string()) {
                        return Err(PathSanitizationError::ForbiddenComponent {
                            path: path.to_path_buf(),
                            component: name_str.to_string(),
                        });
                    }
                }
            }
        }

        // Process path components
        let mut sanitized = PathBuf::new();
        let mut has_parent_refs = false;

        for component in path.components() {
            match component {
                Component::RootDir => {
                    if !self.config.allow_absolute_paths {
                        return Err(PathSanitizationError::AbsolutePathNotAllowed {
                            path: path.to_path_buf(),
                        });
                    }
                    sanitized.push(component);
                }
                Component::Prefix(prefix) => {
                    if !self.config.allow_absolute_paths {
                        return Err(PathSanitizationError::AbsolutePathNotAllowed {
                            path: path.to_path_buf(),
                        });
                    }
                    sanitized.push(prefix.as_os_str());
                }
                Component::ParentDir => {
                    has_parent_refs = true;
                    if !self.config.allow_parent_references {
                        return Err(PathSanitizationError::ParentReferenceNotAllowed {
                            path: path.to_path_buf(),
                        });
                    }
                    sanitized.push(component);
                }
                Component::CurDir => {
                    // Skip current directory components
                }
                Component::Normal(name) => {
                    // Sanitize the component name
                    let sanitized_name = self.sanitize_component_name(name)?;
                    sanitized.push(sanitized_name);
                }
            }
        }

        // Resolve against base directory if provided
        if let Some(ref base_dir) = self.config.base_directory {
            if !path.is_absolute() {
                let resolved = base_dir.join(&sanitized);
                debug!("Resolved path against base directory: {:?}", resolved);
                return Ok(resolved);
            }
        }

        // Normalize the path
        let normalized = self.normalize_path(&sanitized);
        debug!("Sanitized path: {:?}", normalized);

        Ok(normalized)
    }

    /// Validate that a path is safe
    pub fn is_safe<P: AsRef<Path>>(&self, path: P) -> bool {
        self.sanitize(path).is_ok()
    }

    /// Check if a path is within the allowed boundaries
    pub fn is_within_bounds<P: AsRef<Path>>(&self, path: P, base: P) -> bool {
        let path = path.as_ref();
        let base = base.as_ref();

        match (path.canonicalize(), base.canonicalize()) {
            (Ok(canonical_path), Ok(canonical_base)) => {
                canonical_path.starts_with(canonical_base)
            }
            _ => false,
        }
    }

    /// Get the relative path from base to target
    pub fn get_relative_path<P: AsRef<Path>>(&self, base: P, target: P) -> Result<PathBuf, PathSanitizationError> {
        let base = base.as_ref();
        let target = target.as_ref();

        let sanitized_base = self.sanitize(base)?;
        let sanitized_target = self.sanitize(target)?;

        pathdiff::diff_paths(&sanitized_target, &sanitized_target)
            .ok_or_else(|| PathSanitizationError::CannotComputeRelativePath {
                base: sanitized_base,
                target: sanitized_target,
            })
    }

    /// Sanitize a component name
    fn sanitize_component_name(&self, name: &std::ffi::OsStr) -> Result<std::ffi::OsString, PathSanitizationError> {
        let name_str = name.to_str().ok_or_else(|| PathSanitizationError::InvalidPathComponent {
            component: name.to_os_string(),
        })?;

        // Check for dangerous characters
        if name_str.contains('\0') {
            return Err(PathSanitizationError::InvalidPathComponent {
                component: name.to_os_string(),
            });
        }

        // Check for Windows reserved names
        if cfg!(windows) {
            let upper_name = name_str.to_uppercase();
            if matches!(
                upper_name.as_str(),
                "CON" | "PRN" | "AUX" | "NUL"
                    | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
                    | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
            ) {
                return Err(PathSanitizationError::ReservedName {
                    name: name_str.to_string(),
                });
            }
        }

        // Check file extension
        if !self.config.allowed_extensions.is_empty() {
            if let Some(extension) = Path::new(name_str).extension().and_then(|ext| ext.to_str()) {
                if !self.config.allowed_extensions.contains(&extension.to_string()) {
                    return Err(PathSanitizationError::ExtensionNotAllowed {
                        extension: extension.to_string(),
                    });
                }
            }
        }

        Ok(name.to_os_string())
    }

    /// Normalize a path by resolving . and .. components
    fn normalize_path(&self, path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();

        for component in path.components() {
            match component {
                Component::ParentDir => {
                    if !normalized.pop() {
                        warn!("Cannot resolve parent directory beyond root");
                        normalized.push(component);
                    }
                }
                Component::CurDir => {
                    // Skip current directory
                }
                _ => {
                    normalized.push(component);
                }
            }
        }

        normalized
    }
}

impl Default for PathSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Path sanitization errors
#[derive(thiserror::Error, Debug)]
pub enum PathSanitizationError {
    #[error("Path too long: {path:?} (max: {max_length} bytes)")]
    PathTooLong {
        path: PathBuf,
        max_length: usize,
    },

    #[error("Absolute paths not allowed: {path:?}")]
    AbsolutePathNotAllowed {
        path: PathBuf,
    },

    #[error("Parent directory references not allowed: {path:?}")]
    ParentReferenceNotAllowed {
        path: PathBuf,
    },

    #[error("Forbidden path component: {component} in {path:?}")]
    ForbiddenComponent {
        path: PathBuf,
        component: String,
    },

    #[error("Invalid path component: {component:?}")]
    InvalidPathComponent {
        component: std::ffi::OsString,
    },

    #[error("Reserved name not allowed: {name}")]
    ReservedName {
        name: String,
    },

    #[error("File extension not allowed: {extension}")]
    ExtensionNotAllowed {
        extension: String,
    },

    #[error("Cannot compute relative path from {base:?} to {target:?}")]
    CannotComputeRelativePath {
        base: PathBuf,
        target: PathBuf,
    },
}

/// Quick sanitize function for common cases
pub fn sanitize_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, PathSanitizationError> {
    let sanitizer = PathSanitizer::new();
    sanitizer.sanitize(path)
}

/// Quick validation function for common cases
pub fn is_safe_path<P: AsRef<Path>>(path: P) -> bool {
    let sanitizer = PathSanitizer::new();
    sanitizer.is_safe(path)
}

/// Create a safe filename from a string
pub fn create_safe_filename(name: &str) -> String {
    let mut safe_name = String::new();

    for c in name.chars() {
        match c {
            c if c.is_alphanumeric() => safe_name.push(c),
            c if matches!(c, ' ' | '-' | '_' | '.') => safe_name.push(c),
            _ => safe_name.push('_'),
        }
    }

    // Remove leading/trailing underscores and spaces
    safe_name = safe_name.trim_matches('_').trim().to_string();

    // Ensure it's not empty
    if safe_name.is_empty() {
        safe_name = "unnamed".to_string();
    }

    // Limit length
    if safe_name.len() > 255 {
        safe_name.truncate(255);
    }

    safe_name
}

/// Validate a filename for security
pub fn is_safe_filename(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }

    // Check for dangerous characters
    if name.contains('\0') || name.contains('/') || name.contains('\\') {
        return false;
    }

    // Check for Windows reserved names
    if cfg!(windows) {
        let upper_name = name.to_uppercase();
        if matches!(
            upper_name.as_str(),
            "CON" | "PRN" | "AUX" | "NUL"
                | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
                | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
        ) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_basic_sanitization() {
        let sanitizer = PathSanitizer::new();

        assert!(sanitizer.sanitize("safe/path").is_ok());
        assert!(sanitizer.sanitize("safe\\path").is_ok());
        assert!(sanitizer.sanitize("./relative/path").is_ok());
    }

    #[test]
    fn test_absolute_path_restriction() {
        let config = PathSanitizerConfig {
            allow_absolute_paths: false,
            ..Default::default()
        };
        let sanitizer = PathSanitizer::with_config(config);

        assert!(matches!(
            sanitizer.sanitize("/absolute/path"),
            Err(PathSanitizationError::AbsolutePathNotAllowed { .. })
        ));
    }

    #[test]
    fn test_parent_reference_restriction() {
        let config = PathSanitizerConfig {
            allow_parent_references: false,
            ..Default::default()
        };
        let sanitizer = PathSanitizer::with_config(config);

        assert!(matches!(
            sanitizer.sanitize("path/../parent"),
            Err(PathSanitizationError::ParentReferenceNotAllowed { .. })
        ));
    }

    #[test]
    fn test_forbidden_components() {
        let mut config = PathSanitizerConfig::default();
        config.forbidden_components.push("forbidden".to_string());
        let sanitizer = PathSanitizer::with_config(config);

        assert!(matches!(
            sanitizer.sanitize("path/forbidden/file"),
            Err(PathSanitizationError::ForbiddenComponent { .. })
        ));
    }

    #[test]
    fn test_extension_filtering() {
        let mut config = PathSanitizerConfig::default();
        config.allowed_extensions = vec!["rn".to_string(), "rune".to_string()];
        let sanitizer = PathSanitizer::with_config(config);

        assert!(sanitizer.sanitize("tool.rn").is_ok());
        assert!(sanitizer.sanitize("tool.rune").is_ok());
        assert!(matches!(
            sanitizer.sanitize("tool.exe"),
            Err(PathSanitizationError::ExtensionNotAllowed { .. })
        ));
    }

    #[test]
    fn test_base_directory_resolution() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = PathSanitizerConfig::default();
        config.base_directory = Some(temp_dir.path().to_path_buf());
        let sanitizer = PathSanitizer::with_config(config);

        let resolved = sanitizer.sanitize("subdir/file.txt").unwrap();
        assert!(resolved.starts_with(temp_dir.path()));
    }

    #[test]
    fn test_path_normalization() {
        let sanitizer = PathSanitizer::new();

        let normalized = sanitizer.sanitize("path/./to/../file.txt").unwrap();
        assert_eq!(normalized, PathBuf::from("path/file.txt"));
    }

    #[test]
    fn test_safe_filename_creation() {
        assert_eq!(create_safe_filename("valid_name.txt"), "valid_name.txt");
        assert_eq!(create_safe_filename("invalid name!"), "invalid_name_.txt");
        assert_eq!(create_safe_filename(""), "unnamed");
        assert_eq!(create_safe_filename("../../../etc/passwd"), "_____etc_passwd");
    }

    #[test]
    fn test_safe_filename_validation() {
        assert!(is_safe_filename("valid_name.txt"));
        assert!(is_safe_filename("valid-name.txt"));
        assert!(!is_safe_filename("invalid/name.txt"));
        assert!(!is_safe_filename("invalid\\name.txt"));
        assert!(!is_safe_filename(""));
        assert!(!is_safe_filename(&"a".repeat(256)));
    }

    #[test]
    fn test_quick_functions() {
        assert!(sanitize_path("safe/path").is_ok());
        assert!(is_safe_path("safe/path"));
        assert!(!is_safe_path("../../../etc/passwd"));
    }

    #[test]
    fn test_within_bounds() {
        let temp_dir = TempDir::new().unwrap();
        let sanitizer = PathSanitizer::new();

        // Create a subdirectory
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        let file_in_subdir = subdir.join("file.txt");
        std::fs::write(&file_in_subdir, "test").unwrap();

        assert!(sanitizer.is_within_bounds(&file_in_subdir, temp_dir.path()));
        assert!(!sanitizer.is_within_bounds(temp_dir.path(), &file_in_subdir));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_reserved_names() {
        let sanitizer = PathSanitizer::new();

        assert!(matches!(
            sanitizer.sanitize("CON.txt"),
            Err(PathSanitizationError::ReservedName { .. })
        ));
        assert!(matches!(
            sanitizer.sanitize("PRN"),
            Err(PathSanitizationError::ReservedName { .. })
        ));
        assert!(matches!(
            sanitizer.sanitize("AUX.dat"),
            Err(PathSanitizationError::ReservedName { .. })
        ));
    }

    #[test]
    fn test_path_length_limit() {
        let mut config = PathSanitizerConfig::default();
        config.max_path_length = 10;
        let sanitizer = PathSanitizer::with_config(config);

        assert!(matches!(
            sanitizer.sanitize("very_long_path_name"),
            Err(PathSanitizationError::PathTooLong { .. })
        ));
    }
}