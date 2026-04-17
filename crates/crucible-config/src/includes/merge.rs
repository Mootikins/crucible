use std::path::{Path, PathBuf};

#[cfg(feature = "toml")]
use tracing::{debug, warn};

use super::config::IncludeConfig;
use super::error::IncludeError;
#[cfg(feature = "toml")]
use super::path::resolve_include_path;
#[cfg(feature = "toml")]
use super::reference::read_include_file;

/// Merge include files into a TOML value
///
/// This function takes the main config as a TOML Value and merges
/// included files into the appropriate sections.
#[cfg(feature = "toml")]
pub fn merge_includes(
    main_config: &mut toml::Value,
    base_dir: &Path,
) -> Result<(), Vec<IncludeError>> {
    let mut errors = Vec::new();

    // Extract the include section if present
    let includes = if let Some(toml::Value::Table(table)) = main_config.get("include") {
        match toml::Value::Table(table.clone()).try_into::<IncludeConfig>() {
            Ok(inc) => inc,
            Err(e) => {
                errors.push(IncludeError::Parse {
                    path: PathBuf::from("[include section]"),
                    error: e.to_string(),
                });
                return Err(errors);
            }
        }
    } else {
        return Ok(()); // No includes to process
    };

    if includes.is_empty() {
        return Ok(());
    }

    // Process each include
    for (section, include_path) in includes.all_includes() {
        let resolved_path = resolve_include_path(include_path, base_dir);
        debug!(
            "Processing include: {} -> {}",
            section,
            resolved_path.display()
        );

        match read_include_file(&resolved_path) {
            Ok(included_value) => {
                // Merge into the main config at the specified section
                if let toml::Value::Table(ref mut main_table) = main_config {
                    // If the section already exists, merge; otherwise set
                    if let Some(existing) = main_table.get_mut(section) {
                        merge_toml_values(existing, &included_value);
                    } else {
                        main_table.insert(section.to_string(), included_value);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to include {}: {}", include_path, e);
                errors.push(e);
            }
        }
    }

    // Remove the include section from the final config
    if let toml::Value::Table(ref mut main_table) = main_config {
        main_table.remove("include");
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Merge two TOML values, with source overriding target for conflicts
#[cfg(feature = "toml")]
pub(super) fn merge_toml_values(target: &mut toml::Value, source: &toml::Value) {
    match (target, source) {
        (toml::Value::Table(target_table), toml::Value::Table(source_table)) => {
            // Deep merge tables
            for (key, source_value) in source_table {
                if let Some(target_value) = target_table.get_mut(key) {
                    merge_toml_values(target_value, source_value);
                } else {
                    target_table.insert(key.clone(), source_value.clone());
                }
            }
        }
        (toml::Value::Array(target_array), toml::Value::Array(source_array)) => {
            // Append source array items to target
            target_array.extend(source_array.iter().cloned());
        }
        (target, source) => {
            // For other types, source overwrites target
            *target = source.clone();
        }
    }
}
