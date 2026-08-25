//! Shared install/remove operations for user plugins.
//!
//! Both the `cru plugin add` / `cru plugin remove` CLI commands and the
//! `plugin.install` / `plugin.remove` RPC handlers call into these
//! functions. The machine's record of installed plugins is
//! `<data_home>/plugins.installed.json`, a [`RegistryStore`] file like
//! `kilns.json` — locked through a sidecar, replaced atomically. The
//! user's own declarations live in `init.lua` (`plugins.declare.<name>`)
//! and are never written here: a machine that edits the user's config file
//! is the defect this split removes.
//!
//! `plugins.toml` — the file that used to hold both — is no longer read.
//! [`import_legacy_plugins_toml`] moves its entries into the manifest once
//! (idempotently), and the boot warns while the leftover file exists.

use anyhow::{anyhow, Context, Result};
use crucible_core::config::{plugin_name_from_url, PluginEntry, PluginsConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::registry_store::RegistryStore;
use crate::{bootstrap_plugin_entry, BootstrapOutcome};

/// The schema version this daemon writes and understands.
pub const INSTALLED_PLUGINS_VERSION: u32 = 1;

/// The file name under the daemon data root.
pub const INSTALLED_PLUGINS_FILE: &str = "plugins.installed.json";

/// The whole of `plugins.installed.json`: name → entry.
///
/// Name-keyed (the URL-derived directory name), because every read is by
/// name: the bootstrap union, the remove path, the membership check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugins {
    /// Gates future shape changes. A reader refuses a higher number rather
    /// than rewriting a schema it does not know.
    pub version: u32,
    /// Installed plugins, keyed by the URL-derived name.
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginEntry>,
}

impl Default for InstalledPlugins {
    fn default() -> Self {
        Self {
            version: INSTALLED_PLUGINS_VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

/// Outcome of a successful install operation.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub name: String,
    pub outcome: BootstrapOutcome,
    pub manifest: PathBuf,
}

/// Outcome of a successful remove operation.
#[derive(Debug, Clone)]
pub struct RemoveOutcome {
    pub name: String,
    pub manifest: PathBuf,
    pub purged_dir: Option<PathBuf>,
    /// The manifest commit lands before the purge, so a purge failure must
    /// not fail the whole removal — and must not surface as an error message
    /// claiming the entry is "still installed" when it is already gone.
    pub purge_error: Option<String>,
}

/// The installed-plugins manifest under `data_home`.
pub fn installed_manifest_path(data_home: &Path) -> PathBuf {
    data_home.join(INSTALLED_PLUGINS_FILE)
}

/// The legacy declaration file this manifest replaced. Read only by
/// [`import_legacy_plugins_toml`]; nothing writes it any more.
pub fn legacy_plugins_toml_path() -> Option<PathBuf> {
    dirs::config_dir().map(|base| base.join("crucible").join("plugins.toml"))
}

/// Resolve the directory where cloned plugins live.
pub fn plugins_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow!("could not determine config directory"))?;
    Ok(base.join("crucible").join("plugins"))
}

/// Read-modify-write on the manifest, under its sidecar lock, with the
/// version gate applied before `mutate` sees the value.
fn with_manifest<R>(
    manifest_path: &Path,
    mutate: impl FnOnce(&mut InstalledPlugins) -> Result<R>,
) -> Result<R> {
    let store: RegistryStore<InstalledPlugins> = RegistryStore::new(manifest_path.to_path_buf());
    store.update(|manifest| {
        if manifest.version > INSTALLED_PLUGINS_VERSION {
            anyhow::bail!(
                "{} is version {} but this daemon understands up to {}; upgrade Crucible",
                manifest_path.display(),
                manifest.version,
                INSTALLED_PLUGINS_VERSION
            );
        }
        mutate(manifest)
    })
}

/// Read the manifest without writing it back.
fn read_manifest(manifest_path: &Path) -> Result<InstalledPlugins> {
    let store: RegistryStore<InstalledPlugins> = RegistryStore::new(manifest_path.to_path_buf());
    let manifest = store.read()?;
    if manifest.version > INSTALLED_PLUGINS_VERSION {
        anyhow::bail!(
            "{} is version {} but this daemon understands up to {}; upgrade Crucible",
            manifest_path.display(),
            manifest.version,
            INSTALLED_PLUGINS_VERSION
        );
    }
    Ok(manifest)
}

/// Install a plugin: clone it (idempotent if already cloned) and record it
/// in the installed manifest.
///
/// Path-resolving wrapper over [`install_at`] for callers acting without a
/// daemon context (CLI offline fallback); the RPC handler passes the
/// daemon's own `data_home` instead.
pub async fn install(entry: PluginEntry) -> Result<InstallOutcome> {
    install_at(
        entry,
        &installed_manifest_path(&crucible_core::config::crucible_home()),
        &plugins_dir()?,
    )
    .await
}

/// [`install`]'s core with every path injected, so tests never touch the
/// real data root or `~/.config/crucible`.
pub async fn install_at(
    entry: PluginEntry,
    manifest_path: &Path,
    plugins_dir: &Path,
) -> Result<InstallOutcome> {
    let name = plugin_name_from_url(&entry.url)
        .ok_or_else(|| anyhow!("cannot derive plugin name from URL '{}'", entry.url))?;

    // Clone first. If the clone fails (bad URL, no network), don't
    // leave a phantom record behind in the manifest.
    let outcome = bootstrap_plugin_entry(&entry, plugins_dir)
        .await
        .with_context(|| format!("failed to install plugin '{name}'"))?;

    with_manifest(manifest_path, |manifest| {
        if manifest.plugins.contains_key(&name) {
            return Err(anyhow!(
                "plugin '{name}' is already installed (see {})",
                manifest_path.display()
            ));
        }
        manifest.plugins.insert(name.clone(), entry.clone());
        Ok(())
    })?;

    Ok(InstallOutcome {
        name,
        outcome,
        manifest: manifest_path.to_path_buf(),
    })
}

/// Remove a plugin: drop it from the installed manifest and optionally
/// delete its clone directory.
///
/// Path-resolving wrapper over [`remove_at`].
pub fn remove(name: &str, purge: bool) -> Result<RemoveOutcome> {
    remove_at(
        name,
        purge,
        &installed_manifest_path(&crucible_core::config::crucible_home()),
        &plugins_dir()?,
    )
}

/// [`remove`]'s core with every path injected.
pub fn remove_at(
    name: &str,
    purge: bool,
    manifest_path: &Path,
    plugins_dir: &Path,
) -> Result<RemoveOutcome> {
    let found = with_manifest(manifest_path, |manifest| {
        Ok(manifest.plugins.remove(name).is_some())
    })?;

    if !found {
        return Err(anyhow!(
            "plugin '{name}' is not installed in {}",
            manifest_path.display()
        ));
    }

    let (purged_dir, purge_error) = if purge {
        let dir = plugins_dir.join(name);
        if dir.exists() {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => (Some(dir), None),
                Err(e) => (
                    None,
                    Some(format!(
                        "failed to remove plugin dir {}: {e}",
                        dir.display()
                    )),
                ),
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    Ok(RemoveOutcome {
        name: name.to_string(),
        manifest: manifest_path.to_path_buf(),
        purged_dir,
        purge_error,
    })
}

/// Whether `name` is recorded in the installed manifest. A missing file
/// records nothing — every bundled `runtime/plugins/*` plugin lands here,
/// which is how `plugin.remove` tells "removable" from "bundled".
pub fn installed_at(manifest_path: &Path, name: &str) -> Result<bool> {
    Ok(read_manifest(manifest_path)?.plugins.contains_key(name))
}

/// Every installed entry, name-keyed, for the bootstrap union and listings.
pub fn installed_entries(manifest_path: &Path) -> Result<Vec<(String, PluginEntry)>> {
    Ok(read_manifest(manifest_path)?.plugins.into_iter().collect())
}

/// Move the legacy `plugins.toml` entries into the manifest, once.
///
/// Idempotent: an entry whose name the manifest already holds is left
/// alone, so the call is safe on every boot while the leftover file exists.
/// The file itself is not deleted or renamed — it is the user's to remove,
/// and the boot warning names it until they do. Returns how many entries
/// were imported this time.
pub fn import_legacy_plugins_toml(toml_path: &Path, manifest_path: &Path) -> Result<usize> {
    if !toml_path.exists() {
        return Ok(0);
    }
    let content = std::fs::read_to_string(toml_path)
        .with_context(|| format!("failed to read {}", toml_path.display()))?;
    let legacy: PluginsConfig = if content.trim().is_empty() {
        PluginsConfig::default()
    } else {
        toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", toml_path.display()))?
    };

    with_manifest(manifest_path, |manifest| {
        let mut imported = 0usize;
        for entry in legacy.plugin {
            let Some(name) = entry.name() else { continue };
            if !manifest.plugins.contains_key(&name) {
                manifest.plugins.insert(name, entry);
                imported += 1;
            }
        }
        Ok(imported)
    })
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

    /// Write a fake pre-cloned plugin so `bootstrap_plugin_entry`
    /// short-circuits to `AlreadyPresent` before URL normalization —
    /// no git, no network, no `file://` (the URL allowlist stays intact).
    fn write_clone(plugins_dir: &Path, name: &str) {
        let dir = plugins_dir.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.yaml"),
            format!("name: {name}\nversion: \"0.1.0\"\nmain: init.lua\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join("init.lua"),
            format!("return {{ name = '{name}' }}\n"),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn install_at_records_an_already_cloned_plugin_in_the_manifest() {
        let tmp = tempdir().unwrap();
        let manifest = tmp.path().join(INSTALLED_PLUGINS_FILE);
        let plugins_dir = tmp.path().join("plugins");
        write_clone(&plugins_dir, "repo");

        let outcome = install_at(entry("user/repo"), &manifest, &plugins_dir)
            .await
            .unwrap();
        assert_eq!(outcome.name, "repo");
        assert!(matches!(outcome.outcome, BootstrapOutcome::AlreadyPresent));

        let written = read_manifest(&manifest).unwrap();
        assert_eq!(written.version, INSTALLED_PLUGINS_VERSION);
        assert_eq!(written.plugins["repo"].url, "user/repo");

        // Installing the same plugin twice is a record conflict.
        let err = install_at(entry("user/repo"), &manifest, &plugins_dir)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already installed"), "got: {err}");
    }

    #[tokio::test]
    async fn remove_at_drops_the_record_and_purges_only_on_request() {
        let tmp = tempdir().unwrap();
        let manifest = tmp.path().join(INSTALLED_PLUGINS_FILE);
        let plugins_dir = tmp.path().join("plugins");
        write_clone(&plugins_dir, "repo");
        install_at(entry("user/repo"), &manifest, &plugins_dir)
            .await
            .unwrap();

        // Without purge the clone dir survives.
        let removed = remove_at("repo", false, &manifest, &plugins_dir).unwrap();
        assert_eq!(removed.purged_dir, None);
        assert!(plugins_dir.join("repo").exists());
        assert!(read_manifest(&manifest).unwrap().plugins.is_empty());

        // Reinstall + purge deletes the dir too.
        install_at(entry("user/repo"), &manifest, &plugins_dir)
            .await
            .unwrap();
        let removed = remove_at("repo", true, &manifest, &plugins_dir).unwrap();
        assert_eq!(removed.purged_dir, Some(plugins_dir.join("repo")));
        assert!(!plugins_dir.join("repo").exists());
    }

    #[test]
    fn remove_at_errors_when_the_plugin_is_not_installed() {
        let tmp = tempdir().unwrap();
        let manifest = tmp.path().join(INSTALLED_PLUGINS_FILE);
        let err = remove_at("ghost", false, &manifest, &tmp.path().join("plugins")).unwrap_err();
        assert!(err.to_string().contains("not installed"), "got: {err}");
    }

    #[tokio::test]
    async fn installed_at_reports_membership_and_a_missing_file_records_nothing() {
        let tmp = tempdir().unwrap();
        let manifest = tmp.path().join(INSTALLED_PLUGINS_FILE);

        // No manifest at all — nothing is installed (every bundled
        // runtime/plugins/* plugin lands here).
        assert!(!installed_at(&manifest, "repo").unwrap());

        let plugins_dir = tmp.path().join("plugins");
        write_clone(&plugins_dir, "repo");
        install_at(entry("user/repo"), &manifest, &plugins_dir)
            .await
            .unwrap();
        assert!(installed_at(&manifest, "repo").unwrap());
        assert!(!installed_at(&manifest, "other").unwrap());
    }

    #[test]
    fn a_newer_manifest_version_is_refused_not_rewritten() {
        let tmp = tempdir().unwrap();
        let manifest = tmp.path().join(INSTALLED_PLUGINS_FILE);
        std::fs::write(
            &manifest,
            format!(
                r#"{{ "version": {}, "plugins": {{}} }}"#,
                INSTALLED_PLUGINS_VERSION + 1
            ),
        )
        .unwrap();

        let err = installed_at(&manifest, "x").unwrap_err();
        assert!(err.to_string().contains("upgrade Crucible"), "got: {err}");
        let err = with_manifest(&manifest, |_| Ok(())).unwrap_err();
        assert!(err.to_string().contains("upgrade Crucible"), "got: {err}");
    }

    /// The manifest commit lands before the purge, so a purge failure must
    /// not fail the removal — and must not produce an error claiming the
    /// entry is "still installed" when it is already gone.
    #[test]
    fn remove_at_reports_purge_failure_without_lying_about_the_manifest() {
        let tmp = tempdir().unwrap();
        let manifest = tmp.path().join(INSTALLED_PLUGINS_FILE);
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        // A FILE where the clone dir should be: remove_dir_all fails on it.
        std::fs::write(plugins_dir.join("ghost"), "not a directory").unwrap();
        with_manifest(&manifest, |m| {
            m.plugins.insert("ghost".into(), entry("user/ghost"));
            Ok(())
        })
        .unwrap();

        let outcome = remove_at("ghost", true, &manifest, &plugins_dir)
            .expect("a purge failure must not fail the removal itself");

        assert!(outcome.purged_dir.is_none());
        assert!(
            outcome.purge_error.is_some(),
            "the purge failure must be reported, not swallowed"
        );
        assert!(
            !installed_at(&manifest, "ghost").unwrap(),
            "the manifest entry really is gone — no message may claim otherwise"
        );
    }

    #[test]
    fn legacy_import_moves_entries_once_and_only_once() {
        let tmp = tempdir().unwrap();
        let toml_path = tmp.path().join("plugins.toml");
        let manifest = tmp.path().join(INSTALLED_PLUGINS_FILE);
        std::fs::write(
            &toml_path,
            r#"
[[plugin]]
url = "user/repo"
pin = "v1"

[[plugin]]
url = "other/tool"
enabled = false
"#,
        )
        .unwrap();

        assert_eq!(import_legacy_plugins_toml(&toml_path, &manifest).unwrap(), 2);
        let manifest_now = read_manifest(&manifest).unwrap();
        assert_eq!(manifest_now.plugins["repo"].pin.as_deref(), Some("v1"));
        assert!(!manifest_now.plugins["tool"].enabled);

        // Idempotent: a second boot imports nothing and clobbers nothing.
        with_manifest(&manifest, |m| {
            m.plugins.get_mut("repo").unwrap().pin = Some("v2-local-edit".into());
            Ok(())
        })
        .unwrap();
        assert_eq!(import_legacy_plugins_toml(&toml_path, &manifest).unwrap(), 0);
        assert_eq!(
            read_manifest(&manifest).unwrap().plugins["repo"]
                .pin
                .as_deref(),
            Some("v2-local-edit"),
            "a re-import must not overwrite a later manifest edit"
        );

        // A missing legacy file imports nothing.
        std::fs::remove_file(&toml_path).unwrap();
        assert_eq!(import_legacy_plugins_toml(&toml_path, &manifest).unwrap(), 0);
    }
}
