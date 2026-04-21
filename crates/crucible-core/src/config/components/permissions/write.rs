use std::path::{Path, PathBuf};

use super::types::PermissionScope;

pub(super) fn resolve_config_path(
    scope: PermissionScope,
    config_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    match scope {
        PermissionScope::Project => {
            if let Some(dir) = config_dir {
                Ok(dir.join("crucible.toml"))
            } else {
                std::env::current_dir()
                    .map(|d| d.join("crucible.toml"))
                    .map_err(|e| format!("Failed to get current directory: {e}"))
            }
        }
        PermissionScope::User => {
            if let Some(dir) = config_dir {
                Ok(dir.join("config.toml"))
            } else {
                dirs::config_dir()
                    .map(|d| d.join("crucible").join("config.toml"))
                    .ok_or_else(|| "Could not determine user config directory".to_string())
            }
        }
    }
}

/// Write a permission rule to the allow list in the appropriate config file.
///
/// `config_dir` overrides the default config location (useful for testing).
/// For `Project` scope, writes to `crucible.toml`. For `User` scope, writes to `config.toml`.
///
/// Creates the file and parent directories if they don't exist.
/// Preserves existing config content. Skips duplicate rules.
#[cfg(feature = "toml")]
pub fn write_permission_rule(
    scope: PermissionScope,
    rule: &str,
    config_dir: Option<&Path>,
) -> Result<(), String> {
    let path = resolve_config_path(scope, config_dir)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Failed to read {}: {e}", path.display())),
    };

    let mut table: toml::Table = if content.is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(&content).map_err(|e| format!("Failed to parse {}: {e}", path.display()))?
    };

    let permissions = table
        .entry("permissions")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));

    let permissions_table = permissions
        .as_table_mut()
        .ok_or_else(|| "permissions is not a table".to_string())?;

    let allow = permissions_table
        .entry("allow")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    let allow_array = allow
        .as_array_mut()
        .ok_or_else(|| "permissions.allow is not an array".to_string())?;

    if allow_array.iter().any(|v| v.as_str() == Some(rule)) {
        return Ok(());
    }

    allow_array.push(toml::Value::String(rule.to_string()));

    let output =
        toml::to_string_pretty(&table).map_err(|e| format!("Failed to serialize config: {e}"))?;

    std::fs::write(&path, output)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

    Ok(())
}
