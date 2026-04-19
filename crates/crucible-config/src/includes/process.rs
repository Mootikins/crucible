use std::path::Path;

#[cfg(feature = "toml")]
use tracing::{debug, warn};

use super::error::IncludeError;
#[cfg(feature = "toml")]
use super::path::resolve_include_path;
use super::reference::ResolveMode;
#[cfg(feature = "toml")]
use super::reference::{parse_ref_kind, read_dir_as_value, read_file_as_value, RefKind};

/// Process all `{file:path}`, `{env:VAR}`, and `{dir:path}` references in a TOML value tree
///
/// Walks the entire TOML value tree and replaces:
/// - `{file:path}` with the content of the referenced file
/// - `{env:VAR}` with the value of the environment variable
/// - `{dir:path}` with merged content of all .toml files in the directory (config.d style)
#[cfg(feature = "toml")]
pub fn process_file_references(
    value: &mut toml::Value,
    base_dir: &Path,
    mode: ResolveMode,
) -> Result<(), Vec<IncludeError>> {
    let mut errors = Vec::new();
    process_refs_recursive(value, base_dir, &mut errors, mode);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(feature = "toml")]
pub(super) fn process_refs_recursive(
    value: &mut toml::Value,
    base_dir: &Path,
    errors: &mut Vec<IncludeError>,
    mode: ResolveMode,
) {
    match value {
        toml::Value::String(s) => {
            // Parse the reference and handle accordingly
            if let Some(ref_kind) = parse_ref_kind(s) {
                match ref_kind {
                    RefKind::File(file_path) => {
                        let resolved = resolve_include_path(file_path.to_str().unwrap(), base_dir);
                        debug!(
                            "Processing file reference: {} -> {}",
                            file_path.display(),
                            resolved.display()
                        );

                        match read_file_as_value(&resolved) {
                            Ok(file_value) => {
                                *value = file_value;
                            }
                            Err(e) => {
                                match mode {
                                    ResolveMode::Strict => {
                                        tracing::error!(
                                            "Failed to load file reference {}: {}",
                                            file_path.display(),
                                            e
                                        );
                                    }
                                    ResolveMode::BestEffort => {
                                        warn!(
                                            "Failed to load file reference {}: {}",
                                            file_path.display(),
                                            e
                                        );
                                    }
                                }
                                errors.push(e);
                            }
                        }
                    }
                    RefKind::Env(var_name) => {
                        debug!("Processing env reference: {}", var_name);

                        match std::env::var(&var_name) {
                            Ok(env_value) => {
                                *value = toml::Value::String(env_value);
                            }
                            Err(_) => {
                                match mode {
                                    ResolveMode::Strict => {
                                        tracing::error!(
                                            "Environment variable not found: {}",
                                            var_name
                                        );
                                    }
                                    ResolveMode::BestEffort => {
                                        warn!("Environment variable not found: {}", var_name);
                                    }
                                }
                                errors.push(IncludeError::EnvVarNotFound {
                                    var_name: var_name.clone(),
                                });
                            }
                        }
                    }
                    RefKind::Dir(dir_path) => {
                        let resolved = resolve_include_path(dir_path.to_str().unwrap(), base_dir);
                        debug!(
                            "Processing dir reference: {} -> {}",
                            dir_path.display(),
                            resolved.display()
                        );

                        match read_dir_as_value(&resolved, base_dir, errors, mode) {
                            Ok(dir_value) => {
                                *value = dir_value;
                            }
                            Err(e) => {
                                match mode {
                                    ResolveMode::Strict => {
                                        tracing::error!(
                                            "Failed to load dir reference {}: {}",
                                            dir_path.display(),
                                            e
                                        );
                                    }
                                    ResolveMode::BestEffort => {
                                        warn!(
                                            "Failed to load dir reference {}: {}",
                                            dir_path.display(),
                                            e
                                        );
                                    }
                                }
                                errors.push(e);
                            }
                        }
                    }
                }
            }
        }
        toml::Value::Array(arr) => {
            for item in arr.iter_mut() {
                process_refs_recursive(item, base_dir, errors, mode);
            }
        }
        toml::Value::Table(table) => {
            for (_key, val) in table.iter_mut() {
                process_refs_recursive(val, base_dir, errors, mode);
            }
        }
        // Other types (Integer, Float, Boolean, Datetime) don't contain refs
        _ => {}
    }
}
