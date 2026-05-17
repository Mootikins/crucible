//! Shared install/remove operations for user plugins.
//!
//! Both the `cru plugin add` / `cru plugin remove` CLI commands and the
//! `plugin.install` / `plugin.remove` RPC handlers call into these
//! functions. Centralizing the `plugins.toml` read-modify-write here
//! gives us one place to enforce file-level locking (`fs2::FileExt`)
//! so concurrent CLI and daemon writes can't corrupt the config.

use anyhow::{anyhow, Context, Result};
use crucible_core::config::{plugin_name_from_url, PluginEntry, PluginsConfig};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::{bootstrap_plugin_entry, BootstrapOutcome};

/// Outcome of a successful install operation.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub name: String,
    pub outcome: BootstrapOutcome,
    pub plugins_toml: PathBuf,
}

/// Outcome of a successful remove operation.
#[derive(Debug, Clone)]
pub struct RemoveOutcome {
    pub name: String,
    pub plugins_toml: PathBuf,
    pub purged_dir: Option<PathBuf>,
}

/// Resolve the user's `plugins.toml` path. Returns an error if the
/// platform doesn't expose a config directory (almost never on
/// production targets).
pub fn plugins_toml_path() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow!("could not determine config directory"))?;
    Ok(base.join("crucible").join("plugins.toml"))
}

/// Resolve the directory where cloned plugins live.
pub fn plugins_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow!("could not determine config directory"))?;
    Ok(base.join("crucible").join("plugins"))
}

/// Install a plugin: clone it (idempotent if already cloned) and
/// declare it in `plugins.toml`. The TOML write is guarded by a
/// file-level lock so CLI + daemon writes don't race.
pub async fn install(entry: PluginEntry) -> Result<InstallOutcome> {
    let toml_path = plugins_toml_path()?;
    let name = plugin_name_from_url(&entry.url)
        .ok_or_else(|| anyhow!("cannot derive plugin name from URL '{}'", entry.url))?;

    // Clone first. If the clone fails (bad URL, no network), don't
    // leave a phantom declaration behind in plugins.toml.
    let outcome = bootstrap_plugin_entry(&entry)
        .await
        .with_context(|| format!("failed to install plugin '{name}'"))?;

    // Read-modify-write under exclusive lock.
    with_locked_config(&toml_path, |config| {
        if config
            .plugin
            .iter()
            .any(|p| p.name().as_deref() == Some(name.as_str()))
        {
            return Err(anyhow!("plugin '{name}' already declared in plugins.toml"));
        }
        config.plugin.push(entry.clone());
        Ok(())
    })?;

    Ok(InstallOutcome {
        name,
        outcome,
        plugins_toml: toml_path,
    })
}

/// Remove a plugin: drop it from `plugins.toml` and optionally
/// delete its clone directory.
pub fn remove(name: &str, purge: bool) -> Result<RemoveOutcome> {
    let toml_path = plugins_toml_path()?;
    if !toml_path.exists() {
        return Err(anyhow!("no plugins.toml found at {}", toml_path.display()));
    }

    let found = with_locked_config(&toml_path, |config| {
        let before = config.plugin.len();
        config.plugin.retain(|p| p.name().as_deref() != Some(name));
        Ok(config.plugin.len() < before)
    })?;

    if !found {
        return Err(anyhow!("plugin '{name}' not found in {}", toml_path.display()));
    }

    let purged_dir = if purge {
        let dir = plugins_dir()?.join(name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("failed to remove plugin dir {}", dir.display()))?;
            Some(dir)
        } else {
            None
        }
    } else {
        None
    };

    Ok(RemoveOutcome {
        name: name.to_string(),
        plugins_toml: toml_path,
        purged_dir,
    })
}

/// Acquire an exclusive file lock on `plugins.toml` (creating it +
/// any parent dirs if needed), run `mutate` on the parsed config,
/// then atomically write it back. Lock is released when the lock
/// file handle drops at the end of the function.
///
/// Uses `try_lock_exclusive` rather than `lock_exclusive` so racing
/// writers fail fast with a clear "busy, retry" error instead of
/// blocking the caller indefinitely.
fn with_locked_config<F, R>(toml_path: &Path, mutate: F) -> Result<R>
where
    F: FnOnce(&mut PluginsConfig) -> Result<R>,
{
    if let Some(parent) = toml_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    // Lock the TOML file itself. OpenOptions create=true is safe here
    // because the lock guards both readers and writers — a future
    // read-only consumer (e.g. `cru plugin list`) can take a shared
    // lock against the same path.
    let file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(toml_path)
        .with_context(|| format!("failed to open {}", toml_path.display()))?;

    file.try_lock_exclusive().map_err(|e| {
        anyhow!(
            "plugins.toml is locked by another process (retry shortly): {e}",
        )
    })?;

    // Lock is held until `file` drops at function exit.
    let content = std::fs::read_to_string(toml_path).unwrap_or_default();
    let mut config: PluginsConfig = if content.is_empty() {
        PluginsConfig::default()
    } else {
        toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", toml_path.display()))?
    };

    let result = mutate(&mut config)?;

    let serialized = toml::to_string_pretty(&config).context("failed to serialize plugins.toml")?;
    std::fs::write(toml_path, serialized)
        .with_context(|| format!("failed to write {}", toml_path.display()))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entry(url: &str) -> PluginEntry {
        PluginEntry {
            url: url.to_string(),
            branch: None,
            pin: None,
            enabled: true,
        }
    }

    #[test]
    fn with_locked_config_creates_missing_file_and_writes_back() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("plugins.toml");

        // Initial mutate creates file.
        with_locked_config(&path, |c| {
            c.plugin.push(entry("user/repo"));
            Ok(())
        })
        .unwrap();

        let written: PluginsConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written.plugin.len(), 1);
        assert_eq!(written.plugin[0].url, "user/repo");
    }

    #[test]
    fn remove_returns_error_when_plugin_not_present() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("plugins.toml");
        std::fs::write(
            &path,
            toml::to_string_pretty(&PluginsConfig::default()).unwrap(),
        )
        .unwrap();

        let err = with_locked_config(&path, |config| {
            let before = config.plugin.len();
            config.plugin.retain(|p| p.url != "ghost");
            Ok(config.plugin.len() < before)
        })
        .unwrap();

        assert!(!err, "no plugin should have been removed");
    }
}
