//! Plugin search-path resolution and git bootstrap.
//!
//! Free functions split out of `daemon_plugins/mod.rs`: everything here runs
//! before (or independently of) a [`super::DaemonPluginLoader`] — building the
//! prioritized search-path list and cloning declared plugins that are missing
//! on disk.

use anyhow::Context;
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
    let mut paths = Vec::new();

    // 1. CRUCIBLE_PLUGIN_PATH env var (highest priority, for dev/CI)
    if let Ok(env_paths) = std::env::var("CRUCIBLE_PLUGIN_PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for p in env_paths.split(sep) {
            if !p.is_empty() {
                paths.push((PathBuf::from(p), PluginSource::EnvPath));
            }
        }
    }

    // 2. User plugins (~/.config/crucible/plugins/)
    if let Some(config_dir) = dirs::config_dir() {
        paths.push((
            config_dir.join("crucible").join("plugins"),
            PluginSource::User,
        ));
    }

    // 3a. Configured runtimepath entries, ahead of the shipped runtime so they
    // can shadow a bundled plugin by name. Additive — 3b still runs.
    for rtp in runtimepath {
        let expanded = expand_tilde(rtp);
        let plugins_dir = expanded.join("plugins");
        if plugins_dir.exists() {
            tracing::debug!("Adding runtimepath plugin dir: {:?}", plugins_dir);
            paths.push((plugins_dir, PluginSource::Runtime));
        }
    }

    // 3b. The shipped runtime, always searched.
    {
        // Auto-detect: CRUCIBLE_RUNTIME env → exe-relative fallback
        if let Ok(runtime_base) = std::env::var("CRUCIBLE_RUNTIME") {
            let runtime_plugins = PathBuf::from(runtime_base).join("plugins");
            if runtime_plugins.exists() {
                tracing::debug!("Adding runtime plugin path: {:?}", runtime_plugins);
                paths.push((runtime_plugins, PluginSource::Runtime));
            }
        } else {
            // Installed layout, then the dev tree, then the copy extracted from
            // the binary; see `runtime_roots`.
            paths.extend(runtime_plugin_paths(
                &crucible_core::runtime_roots::for_current_exe(),
            ));
        }
    }

    // Every directory above is one the daemon executes Lua out of, so the
    // write-protected set has to name it. Recording here rather than asking
    // `protected` to rebuild the same list is what keeps the two from drifting:
    // a tree that reaches this return is protected by the act of reaching it.
    crate::execution_roots::record(paths.iter().map(|(dir, _)| dir.clone()));

    paths
}

/// The `plugins/` directories among `roots`, in the order given.
///
/// Split out so the auto-detect branch is reachable from a test without
/// controlling the running binary's location — the sibling resolver in
/// `skills::discovery` has had `runtime_skill_paths` for the same reason, and
/// this branch had no equivalent, which is why nothing caught that the bundled
/// plugins reached no installed user.
pub(crate) fn runtime_plugin_paths(roots: &[PathBuf]) -> Vec<(PathBuf, PluginSource)> {
    roots
        .iter()
        .map(|root| root.join("plugins"))
        .filter(|dir| dir.exists())
        .inspect(|dir| tracing::debug!("Adding runtime plugin path: {:?}", dir))
        .map(|dir| (dir, PluginSource::Runtime))
        .collect()
}

/// Expand `~` at the start of a path to the user's home directory.
///
/// Delegates to the one expander rather than keeping a fourth copy — this one
/// indexed `&s[2..]` on a bare `~`, which is a panic, not an expansion.
fn expand_tilde(path: &std::path::Path) -> PathBuf {
    // Only a `~` path reaches the expander; a non-UTF-8 path is returned
    // byte-for-byte rather than lossily rewritten. See
    // `kiln_manager::expand_tilde_path`.
    match path.to_str() {
        Some(s) if s.starts_with('~') => {
            crate::project_manager::resolve_registration_root(s, dirs::home_dir().as_deref())
        }
        _ => path.to_path_buf(),
    }
}

/// Return default plugin paths (no config runtimepath).
/// Convenience for callers that don't have access to config.
pub fn default_daemon_plugin_paths() -> Vec<(PathBuf, PluginSource)> {
    daemon_plugin_paths(&[])
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

/// Bootstrap a single plugin entry: clone into `plugins_dir` if missing,
/// check out pin if set. Returns a structured outcome so callers (CLI vs
/// daemon startup) can decide how loudly to react to failures. The target
/// dir is a parameter so tests can inject a temp dir instead of touching
/// the real `~/.config/crucible/plugins`.
///
/// Pin handling: when a pin is set we drop `--depth 1` because a shallow
/// clone often won't contain the target SHA on the tip. Tags and branch
/// names usually work shallow, but SHAs need full history. Trading
/// bandwidth for correctness.
pub async fn bootstrap_plugin_entry(
    entry: &crucible_core::config::PluginEntry,
    plugins_dir: &std::path::Path,
) -> anyhow::Result<BootstrapOutcome> {
    if !entry.enabled {
        return Ok(BootstrapOutcome::Disabled);
    }

    let name = plugin_name_from_url(&entry.url)
        .ok_or_else(|| anyhow::anyhow!("Plugin URL '{}' has no usable name segment", entry.url))?;
    let dest = plugins_dir.join(&name);
    if dest.exists() {
        return Ok(BootstrapOutcome::AlreadyPresent);
    }

    let url =
        normalize_git_url(&entry.url).with_context(|| format!("rejecting plugin '{}'", name))?;
    info!("Cloning plugin '{}' from {}", name, url);

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("clone");
    // Shallow clone unless we need to check out a specific SHA later —
    // shallow clones often don't contain the target SHA.
    if entry.pin.is_none() {
        cmd.args(["--depth", "1"]);
    }
    if let Some(ref branch) = entry.branch {
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

    if let Some(ref pin) = entry.pin {
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

/// Bootstrap declared plugins by git-cloning any that are missing.
///
/// Reads `PluginEntry` declarations (typically from `plugins.toml`).
/// Failures are warned and skipped — the daemon should start even if
/// one plugin can't be fetched. For per-entry error reporting (e.g.
/// `cru install`), use `bootstrap_plugin_entry` directly.
pub async fn bootstrap_plugins(
    entries: &[crucible_core::config::PluginEntry],
) -> anyhow::Result<()> {
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

/// Local alias for `crucible_core::config::plugin_name_from_url` so
/// the existing call sites in this file read naturally. The canonical
/// implementation lives in core so CLI, daemon, and any future
/// consumer share the same definition of "safe plugin directory name".
pub(crate) fn plugin_name_from_url(url: &str) -> Option<String> {
    crucible_core::config::plugin_name_from_url(url)
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
