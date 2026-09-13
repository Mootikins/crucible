//! Plugin search-path resolution and git bootstrap.
//!
//! Free functions split out of `daemon_plugins/mod.rs`: everything here runs
//! before (or independently of) a [`super::DaemonPluginLoader`] — building the
//! prioritized search-path list and cloning the spec's `Git` entries that are
//! missing on disk.

use anyhow::Context;
use crucible_core::config::{Spec, SpecEntry, SpecRank, SpecSource};
use crucible_lua::PluginSource;
use std::path::PathBuf;
use tracing::{info, warn};

/// Build plugin search paths from config `runtimepath` + env vars + defaults.
///
/// `runtimepath` entries **add to** the shipped runtime, they do not replace
/// it: each entry's `plugins/` subdir is prepended ahead of the auto-detected
/// roots, which are always searched. This mirrors Vim's `runtimepath`, where
/// `$VIMRUNTIME` is always a member and a user appends to the list.
///
/// It used to be an either/or — a non-empty `runtimepath` skipped the
/// auto-detected roots entirely — which made the one thing `runtimepath` is
/// for unusable. Putting a kiln on it to pick up that kiln's plugins silently
/// unloaded all ten bundled ones (`oci`, `review`, `web-search`, …), with the
/// only evidence a `debug!` line naming what *was* added.
///
/// `CRUCIBLE_PLUGIN_PATH` env var always prepends (highest priority).
/// `~/.config/crucible/plugins/` is always included as User source.
///
/// Paths are ordered by priority (highest first) — same-named plugins at
/// higher-priority paths shadow lower-priority ones. A `runtimepath` entry
/// therefore shadows a same-named bundled plugin, which is how you override
/// one.
pub fn daemon_plugin_paths(runtimepath: &[std::path::PathBuf]) -> Vec<(PathBuf, PluginSource)> {
    daemon_plugin_paths_from(&crate::runtime_path::daemon_path(runtimepath))
}

/// [`daemon_plugin_paths`] with the path supplied rather than assembled.
///
/// Tests must use this. The assembling version reads `dirs::config_dir()` and
/// `runtime_roots::for_current_exe()`, so a test asserting "this root offers no
/// plugins" would pass on CI and fail on any machine where `cru` is installed —
/// which is every developer's. `runtime_plugin_paths` was split out for exactly
/// this reason before; this is that seam, moved up a level now that one
/// resolver answers for the whole path.
pub fn daemon_plugin_paths_from(
    path: &[crucible_core::runtime_path::RuntimeEntry],
) -> Vec<(PathBuf, PluginSource)> {
    let candidates = crucible_core::runtime_path::search_paths(
        crucible_core::runtime_path::RuntimeAsset::Plugins,
        path,
    );

    // Record BEFORE filtering for existence. Protection is judged on the name,
    // because the directory an agent creates is by definition the one that did
    // not exist. `search_paths` deliberately consults no filesystem, so this is
    // the full candidate set.
    //
    // Recording here rather than asking `protected` to rebuild the same list is
    // what keeps the two from drifting: a tree that reaches this function is
    // protected by the act of reaching it.
    crate::execution_roots::record(candidates.iter().map(|c| c.path.clone()));

    candidates
        .into_iter()
        .filter(|c| c.path.exists())
        .inspect(|c| tracing::debug!("Adding plugin path: {:?} ({:?})", c.path, c.origin))
        .map(|c| (c.path, plugin_source_for(c.origin)))
        .collect()
}

/// How a root's provenance reads to the plugin loader.
///
/// `PluginSource` is the loader's older, coarser vocabulary: it distinguishes
/// the env override, the user's own directory, and everything shipped or
/// configured. `Origin` is finer, so this is a narrowing.
///
/// The four origins no arm names — `Workspace`, `Kiln`, `Harness`, `Plugin` —
/// cannot appear: `RuntimeAsset::Plugins::reaches` refuses all four, and
/// `search_paths` drops them before a caller sees them. They map to `Runtime`
/// rather than panicking because a narrowing function is the wrong place to
/// enforce containment; the type-level gate in `asset.rs` is the right one, and
/// `the_daemon_path_offers_plugins_no_containment_hazard` proves it holds.
fn plugin_source_for(origin: crucible_core::runtime_path::Origin) -> PluginSource {
    use crucible_core::runtime_path::Origin;
    match origin {
        Origin::Env => PluginSource::EnvPath,
        Origin::UserConfig => PluginSource::User,
        Origin::Config(_)
        | Origin::UserRuntime
        | Origin::Bundled
        | Origin::Workspace
        | Origin::Kiln
        | Origin::Harness
        | Origin::Plugin => PluginSource::Runtime,
    }
}

/// Return default plugin paths (no config runtimepath).
/// Convenience for callers that don't have access to config.
pub fn default_daemon_plugin_paths() -> Vec<(PathBuf, PluginSource)> {
    daemon_plugin_paths(&[])
}

/// The spec's `Git` entries: what the bootstrap clones when the directory
/// is missing. The operator's `init.lua` and the installed manifest both
/// land in the spec, so this is the whole set, with the operator's entry
/// already laid over the installed one for a shared name.
pub fn bootstrap_entries(spec: &Spec) -> Vec<SpecEntry> {
    spec.iter()
        .filter(|entry| matches!(entry.source, SpecSource::Git { .. }))
        .cloned()
        .collect()
}

/// Whether the operator's own entry for `name` names a `Git` source.
///
/// This is the "declared" test. An installed plugin also has a `Git`
/// source, at `SpecRank::Builtin`, and an operator entry `{ "greeter",
/// enabled = false }` over it has none of its own. Both are not declared:
/// `cru plugin remove` may act on them, because the record it removes is
/// the manifest's, not a line in `init.lua`.
pub fn declared_git_entry(spec: &Spec, name: &str) -> bool {
    spec.at(name, SpecRank::Operator)
        .is_some_and(|entry| matches!(entry.source, SpecSource::Git { .. }))
}

/// Outcome of attempting to bootstrap a single plugin entry.
#[derive(Debug, Clone)]
pub enum BootstrapOutcome {
    /// Plugin already cloned at the expected destination; no work done.
    AlreadyPresent,
    /// Disabled in config; skipped.
    Disabled,
    /// Successfully cloned (and pinned, if specified).
    Cloned { dest: PathBuf },
}

/// Bootstrap one spec entry with a `Git` source: clone into `plugins_dir`
/// if missing, check out the pin if set. Returns a structured outcome so
/// callers (CLI vs daemon startup) can decide how loudly to react to
/// failures. The target dir is a parameter so tests can inject a temp dir
/// instead of touching the real `~/.config/crucible/plugins`.
///
/// An entry whose `enabled` is `Some(false)` is skipped. The config leaf
/// `plugins.<name>.enabled` is not read here: a plugin a setting disables
/// is still cloned, and activation is what reads the leaf.
///
/// Pin handling: when a pin is set we drop `--depth 1` because a shallow
/// clone often won't contain the target SHA on the tip. Tags and branch
/// names usually work shallow, but SHAs need full history. Trading
/// bandwidth for correctness.
pub async fn bootstrap_plugin_entry(
    entry: &SpecEntry,
    plugins_dir: &std::path::Path,
) -> anyhow::Result<BootstrapOutcome> {
    let SpecSource::Git { url, branch, pin } = &entry.source else {
        anyhow::bail!(
            "plugin '{}' has no git source; a runtimepath entry is not cloned",
            entry.name
        );
    };
    if entry.enabled == Some(false) {
        return Ok(BootstrapOutcome::Disabled);
    }

    let name = &entry.name;
    let dest = plugins_dir.join(name);
    if dest.exists() {
        return Ok(BootstrapOutcome::AlreadyPresent);
    }

    let url = normalize_git_url(url).with_context(|| format!("rejecting plugin '{}'", name))?;
    info!("Cloning plugin '{}' from {}", name, url);

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("clone");
    // Shallow clone unless we need to check out a specific SHA later —
    // shallow clones often don't contain the target SHA.
    if pin.is_none() {
        cmd.args(["--depth", "1"]);
    }
    if let Some(branch) = branch {
        cmd.args(["--branch", branch]);
    }
    // Defense-in-depth: `--` stops git from parsing any subsequent argv
    // as flags, even if a future caller bypasses normalize_git_url.
    cmd.arg("--").arg(&url).arg(&dest);

    let output = cmd
        .output()
        .await
        .with_context(|| format!("failed to spawn git clone for '{}'", name))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed for '{}': {}", name, stderr.trim());
    }

    if let Some(pin) = pin {
        let checkout = tokio::process::Command::new("git")
            .args(["checkout", pin])
            .current_dir(&dest)
            .output()
            .await
            .with_context(|| format!("failed to spawn git checkout for pin '{}'", pin))?;
        if !checkout.status.success() {
            // Roll back the cloned dir so retries don't get stuck on
            // a half-installed plugin. Warn loudly if rollback itself
            // fails — the user needs to know `dest` is dirty so they
            // can clean it up manually.
            if let Err(rb_err) = tokio::fs::remove_dir_all(&dest).await {
                warn!(
                    plugin = %name,
                    path = %dest.display(),
                    error = %rb_err,
                    "Failed to roll back half-installed plugin after pin checkout failure; \
                     remove the directory manually before retrying"
                );
            }
            let stderr = String::from_utf8_lossy(&checkout.stderr);
            anyhow::bail!(
                "git checkout failed for pin '{}' of plugin '{}' (manually remove {} if it still exists): {}",
                pin,
                name,
                dest.display(),
                stderr.trim()
            );
        }
    }

    Ok(BootstrapOutcome::Cloned { dest })
}

/// Bootstrap the spec's `Git` entries by git-cloning any that are missing.
///
/// Failures are warned and skipped — the daemon should start even if
/// one plugin can't be fetched. For per-entry error reporting (e.g.
/// `cru install`), use `bootstrap_plugin_entry` directly.
pub async fn bootstrap_plugins(entries: &[SpecEntry]) -> anyhow::Result<()> {
    let plugins_dir = crate::plugin_ops::plugins_dir()?;
    for entry in entries {
        match bootstrap_plugin_entry(entry, &plugins_dir).await {
            Ok(_) => {}
            Err(e) => {
                warn!("Plugin bootstrap failed: {}", e);
            }
        }
    }
    Ok(())
}

/// Normalize and validate a plugin git URL.
///
/// Accepted forms:
/// - `https://...` / `http://...`
/// - `ssh://git@host/repo[.git]`
/// - `git@host:user/repo[.git]`
/// - Bare `user/repo` shorthand (expanded to `https://github.com/user/repo.git`)
///
/// Rejected:
/// - URLs starting with `-` (parsed as a git flag — CVE-2017-1000117 family)
/// - URLs containing `::` (git external transport — RCE vector via `ext::sh ...`)
/// - Other schemes (`file://`, `git://`, custom) — narrows the attack surface to
///   forms with a vetted use case
/// - Shorthand containing anything outside `[A-Za-z0-9._/-]` (defends against
///   shell-quoting hazards if the value ever lands in a non-`exec`-style context)
pub(crate) fn normalize_git_url(url: &str) -> anyhow::Result<String> {
    if url.is_empty() {
        anyhow::bail!("plugin URL is empty");
    }
    if url.starts_with('-') {
        anyhow::bail!(
            "plugin URL '{}' starts with '-' (would be parsed as a git flag)",
            url
        );
    }
    if url.contains("::") {
        anyhow::bail!(
            "plugin URL '{}' contains '::' (git external transport, disallowed)",
            url
        );
    }

    if url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://git@")
        || url.starts_with("git@")
    {
        Ok(url.to_string())
    } else if url.contains("://") {
        anyhow::bail!(
            "plugin URL '{}' uses unsupported scheme (allowed: https, http, ssh://git@, git@host:repo)",
            url
        )
    } else {
        if !url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
        {
            anyhow::bail!(
                "plugin shorthand '{}' must match [A-Za-z0-9._/-]+ (got '{}')",
                url,
                url
            );
        }
        Ok(format!("https://github.com/{}.git", url))
    }
}
