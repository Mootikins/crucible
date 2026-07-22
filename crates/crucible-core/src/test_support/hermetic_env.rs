//! Hermetic environments for test child processes.
//!
//! Tests that spawn `cru` (daemons, CLIs, TUIs) must NOT hand the child the
//! parent's real environment: developer machines carry live provider
//! credentials (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GLM_AUTH_TOKEN`, …),
//! and a test daemon that inherits them can silently make real, billable
//! API calls — or leak the keys into captured output. Scrubbing by
//! *removing known variables* is a denylist that rots; the only safe shape
//! is `env_clear()` plus an explicit allowlist.
//!
//! Usage (std or assert_cmd commands both expose `env_clear`/`env`):
//!
//! ```ignore
//! cmd.env_clear();
//! for (k, v) in hermetic_env_pairs(temp_home.path()) {
//!     cmd.env(k, v);
//! }
//! // then any test-specific vars (CRUCIBLE_SOCKET, fixtures…)
//! ```

use std::path::Path;

/// Variables passed through from the parent because the child needs them to
/// run at all — none can carry credentials.
const PASSTHROUGH: &[&str] = &[
    // Finding executables (shells, agents, `env` itself).
    "PATH",
    // Terminal behavior for PTY/TUI children.
    "TERM",
    // Locale: keeps UTF-8 handling identical to the parent.
    "LANG",
    "LC_ALL",
    // Debuggability of failing children.
    "RUST_BACKTRACE",
    "RUST_LOG",
    // CI detection (some code paths intentionally branch on it).
    "CI",
];

/// The full environment for a hermetic test child, rooted at `home`.
///
/// `home` should be a per-test temporary directory; every path-shaped
/// variable (`HOME`, the XDG dirs) lands inside it, so the child can never
/// read the developer's real config, secrets file, kiln registry, or api_key
/// file — and anything it writes dies with the TempDir.
///
/// Apply after `env_clear()`; add test-specific variables afterwards.
pub fn hermetic_env_pairs(home: &Path) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = PASSTHROUGH
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();

    let p = |path: &Path| path.display().to_string();
    pairs.push(("HOME".into(), p(home)));
    pairs.push(("XDG_CONFIG_HOME".into(), p(&home.join(".config"))));
    pairs.push(("XDG_DATA_HOME".into(), p(&home.join(".local/share"))));
    pairs.push(("XDG_CACHE_HOME".into(), p(&home.join(".cache"))));
    // Socket resolution ($XDG_RUNTIME_DIR/crucible.sock) stays inside the
    // sandbox even when a test forgets to set CRUCIBLE_SOCKET explicitly.
    pairs.push(("XDG_RUNTIME_DIR".into(), p(home)));
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_never_include_credential_variables() {
        // Regardless of what the parent environment holds, only allowlisted
        // names appear.
        let home = tempfile::tempdir().unwrap();
        let pairs = hermetic_env_pairs(home.path());
        let allowed: Vec<&str> = PASSTHROUGH
            .iter()
            .copied()
            .chain([
                "HOME",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_CACHE_HOME",
                "XDG_RUNTIME_DIR",
            ])
            .collect();
        for (k, _) in &pairs {
            assert!(allowed.contains(&k.as_str()), "unexpected env var {k}");
        }
    }

    #[test]
    fn path_shaped_vars_root_inside_home() {
        let home = tempfile::tempdir().unwrap();
        let pairs = hermetic_env_pairs(home.path());
        let get = |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        let root = home.path().display().to_string();
        for var in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
        ] {
            assert!(
                get(var).starts_with(&root),
                "{var} must live under the temp home"
            );
        }
    }
}
