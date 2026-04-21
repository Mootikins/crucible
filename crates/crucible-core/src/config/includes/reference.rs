use std::path::{Path, PathBuf};

#[cfg(feature = "toml")]
use tracing::{debug, warn};

use super::error::IncludeError;
#[cfg(feature = "toml")]
use super::merge::merge_toml_values;
#[cfg(feature = "toml")]
use super::process::process_refs_recursive;

/// Pattern for file references: {file:path}
const FILE_REF_PREFIX: &str = "{file:";
const FILE_REF_SUFFIX: &str = "}";

/// Pattern for env references: {env:VAR}
const ENV_REF_PREFIX: &str = "{env:";
const ENV_REF_SUFFIX: &str = "}";

/// Pattern for directory references: {dir:path}
const DIR_REF_PREFIX: &str = "{dir:";
const DIR_REF_SUFFIX: &str = "}";

/// Reference kind enum for template resolution
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RefKind {
    /// File reference: {file:path}
    File(PathBuf),
    /// Environment variable reference: {env:VAR}
    Env(String),
    /// Directory reference: {dir:path}
    Dir(PathBuf),
}

/// Controls how template resolution handles missing references (e.g., env vars).
///
/// - `BestEffort` (default): logs warnings and continues, collecting errors.
/// - `Strict`: treats missing references as hard errors (logs at error level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolveMode {
    /// Treat missing references as hard errors.
    Strict,
    /// Log warnings and continue (default). Current callers use this.
    #[default]
    BestEffort,
}

/// Parse a reference string into RefKind if it matches any pattern
pub(super) fn parse_ref_kind(s: &str) -> Option<RefKind> {
    if s.starts_with(FILE_REF_PREFIX) && s.ends_with(FILE_REF_SUFFIX) {
        let path_str = &s[FILE_REF_PREFIX.len()..s.len() - FILE_REF_SUFFIX.len()];
        Some(RefKind::File(PathBuf::from(path_str)))
    } else if s.starts_with(ENV_REF_PREFIX) && s.ends_with(ENV_REF_SUFFIX) {
        let var_name = &s[ENV_REF_PREFIX.len()..s.len() - ENV_REF_SUFFIX.len()];
        Some(RefKind::Env(var_name.to_string()))
    } else if s.starts_with(DIR_REF_PREFIX) && s.ends_with(DIR_REF_SUFFIX) {
        let path_str = &s[DIR_REF_PREFIX.len()..s.len() - DIR_REF_SUFFIX.len()];
        Some(RefKind::Dir(PathBuf::from(path_str)))
    } else {
        None
    }
}

/// Read an include file and return its content as a TOML value
#[cfg(feature = "toml")]
pub fn read_include_file(path: &Path) -> Result<toml::Value, IncludeError> {
    if !path.exists() {
        return Err(IncludeError::FileNotFound(path.to_path_buf()));
    }

    let content = std::fs::read_to_string(path).map_err(|e| IncludeError::Io {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;

    let value: toml::Value = toml::from_str(&content).map_err(|e| IncludeError::Parse {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;

    Ok(value)
}

/// Read a file and return its content as a TOML value
///
/// - If the file has a `.toml` extension, parse it as TOML
/// - Otherwise, return the content as a trimmed string
#[cfg(feature = "toml")]
pub(super) fn read_file_as_value(path: &Path) -> Result<toml::Value, IncludeError> {
    if !path.exists() {
        return Err(IncludeError::FileNotFound(path.to_path_buf()));
    }

    let content = std::fs::read_to_string(path).map_err(|e| IncludeError::Io {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;

    // If it's a TOML file, parse it as structured data
    if path.extension().is_some_and(|ext| ext == "toml") {
        let value: toml::Value = toml::from_str(&content).map_err(|e| IncludeError::Parse {
            path: path.to_path_buf(),
            error: e.to_string(),
        })?;
        Ok(value)
    } else {
        // Otherwise, return as a trimmed string (useful for secrets)
        Ok(toml::Value::String(content.trim().to_string()))
    }
}

/// Read all .toml files from a directory and merge them
///
/// Files are processed in sorted order (alphabetically by filename),
/// allowing predictable override behavior with numeric prefixes:
/// - `00-base.toml` is processed first
/// - `99-override.toml` is processed last and overrides earlier values
///
/// Non-.toml files are ignored.
#[cfg(feature = "toml")]
pub(super) fn read_dir_as_value(
    dir_path: &Path,
    base_dir: &Path,
    errors: &mut Vec<IncludeError>,
    mode: ResolveMode,
) -> Result<toml::Value, IncludeError> {
    if !dir_path.exists() {
        return Err(IncludeError::DirNotFound(dir_path.to_path_buf()));
    }

    if !dir_path.is_dir() {
        return Err(IncludeError::NotADirectory(dir_path.to_path_buf()));
    }

    // Collect and sort .toml files
    let mut toml_files: Vec<PathBuf> = std::fs::read_dir(dir_path)
        .map_err(|e| IncludeError::Io {
            path: dir_path.to_path_buf(),
            error: e.to_string(),
        })?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| ext == "toml")
                && !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();

    toml_files.sort();

    debug!(
        "Reading {} .toml files from {}",
        toml_files.len(),
        dir_path.display()
    );

    // Start with an empty table
    let mut result = toml::Value::Table(toml::map::Map::new());

    // Merge each file in order
    for file_path in toml_files {
        debug!("Processing config fragment: {}", file_path.display());

        match read_file_as_value(&file_path) {
            Ok(mut file_value) => {
                // Recursively process any refs in this file
                process_refs_recursive(&mut file_value, base_dir, errors, mode);

                // Merge into result
                merge_toml_values(&mut result, &file_value);
            }
            Err(e) => {
                warn!(
                    "Failed to read config fragment {}: {}",
                    file_path.display(),
                    e
                );
                errors.push(e);
            }
        }
    }

    Ok(result)
}
