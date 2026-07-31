//! Resolving the shipped Lua defaults off the runtimepath.
//!
//! `runtime/defaults/init.lua` is an ordinary runtime file — same tree as
//! `runtime/themes/` and `runtime/plugins/`, no privileged API, no distinct
//! semantics. Its only privilege is running before the user's config.
//!
//! Resolution mirrors [`crate::daemon_plugins::daemon_plugin_paths`] so
//! "where does Crucible look for its runtime files" has ONE answer:
//!
//! 1. `CRUCIBLE_RUNTIME` (dev/CI, and what `cru setup` tells you to export)
//! 2. exe-relative — installed `<prefix>/share/crucible/runtime/`, then the
//!    dev `<repo>/runtime/`
//! 3. the compiled-in copy ([`crucible_lua::BUILTIN_INIT_LUA`])
//!
//! First hit wins, so a copied-out tree fully replaces the shipped defaults —
//! the same shadowing plugins already have. Editing your copy is how you
//! remove a default outright, as opposed to layering another statement on top
//! of it from your own `init.lua`.

use std::path::PathBuf;

/// Where a set of defaults came from. Carried so logs can say which file is
/// in force — "my edit did nothing" is otherwise very hard to diagnose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultsSource {
    /// A file on the runtimepath.
    File(PathBuf),
    /// The copy compiled into the binary.
    Builtin,
}

impl std::fmt::Display for DefaultsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(p) => write!(f, "{}", p.display()),
            Self::Builtin => write!(f, "<built-in>"),
        }
    }
}

/// Candidate `defaults/init.lua` paths, highest priority first.
///
/// Split out so the ordering is testable without touching the filesystem or
/// reading whatever the developer happens to have installed.
pub fn defaults_candidates(runtimepath: &[PathBuf], env_runtime: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    for rtp in runtimepath {
        candidates.push(rtp.join("defaults").join("init.lua"));
    }

    if let Some(base) = env_runtime {
        candidates.push(PathBuf::from(base).join("defaults").join("init.lua"));
    }

    // Installed layout first, then the dev tree; see `runtime_roots`.
    for root in crucible_core::runtime_roots::for_current_exe() {
        candidates.push(root.join("defaults").join("init.lua"));
    }

    candidates
}

/// Load the defaults source, and say where it came from.
///
/// An unreadable candidate is skipped rather than fatal — a half-installed
/// runtime directory should degrade to the built-in copy, not leave sessions
/// with no defaults at all.
pub fn load_defaults(runtimepath: &[PathBuf]) -> (String, DefaultsSource) {
    let env_runtime = std::env::var("CRUCIBLE_RUNTIME").ok();
    for candidate in defaults_candidates(runtimepath, env_runtime.as_deref()) {
        match std::fs::read_to_string(&candidate) {
            Ok(source) => return (source, DefaultsSource::File(candidate)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!(
                    path = %candidate.display(),
                    error = %e,
                    "runtime defaults exist but could not be read; trying the next candidate"
                );
            }
        }
    }
    (
        crucible_lua::BUILTIN_INIT_LUA.to_string(),
        DefaultsSource::Builtin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtimepath_entries_outrank_everything_else() {
        let rtp = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let candidates = defaults_candidates(&rtp, Some("/env"));

        assert_eq!(candidates[0], PathBuf::from("/a/defaults/init.lua"));
        assert_eq!(candidates[1], PathBuf::from("/b/defaults/init.lua"));
        assert_eq!(candidates[2], PathBuf::from("/env/defaults/init.lua"));
    }

    #[test]
    fn env_runtime_is_used_when_no_runtimepath_is_configured() {
        let candidates = defaults_candidates(&[], Some("/env"));
        assert_eq!(candidates[0], PathBuf::from("/env/defaults/init.lua"));
    }

    #[test]
    fn exe_relative_candidates_are_the_last_resort_before_builtin() {
        let candidates = defaults_candidates(&[], None);
        // Exe-relative only; nothing higher-priority was configured.
        assert!(
            candidates
                .iter()
                .all(|c| c.ends_with("runtime/defaults/init.lua")),
            "expected only exe-relative candidates, got {candidates:?}"
        );
    }

    /// A copied-out tree REPLACES the shipped defaults rather than layering,
    /// which is what makes "edit your copy" a way to remove a default.
    #[test]
    fn a_runtimepath_file_shadows_the_builtin() {
        let tmp = tempfile::TempDir::new().unwrap();
        let defaults_dir = tmp.path().join("defaults");
        std::fs::create_dir_all(&defaults_dir).unwrap();
        std::fs::write(defaults_dir.join("init.lua"), "-- mine\n").unwrap();

        let (source, origin) = load_defaults(&[tmp.path().to_path_buf()]);

        assert_eq!(source, "-- mine\n");
        assert_eq!(
            origin,
            DefaultsSource::File(defaults_dir.join("init.lua")),
            "the resolved path is reported so logs can say which file is in force"
        );
        assert!(
            !source.contains("cru.defaults"),
            "shadowing must REPLACE the shipped defaults, not append to them"
        );
    }

    /// A bare binary with no runtime directory still gets defaults.
    #[test]
    fn falls_back_to_the_builtin_when_nothing_is_installed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (source, origin) = load_defaults(&[tmp.path().to_path_buf()]);

        assert_eq!(origin, DefaultsSource::Builtin);
        assert!(
            source.contains("cru.defaults.system_prompt"),
            "the built-in copy must carry the shipped defaults"
        );
    }

    /// A directory where the file should be (or any other read error) must not
    /// leave sessions with no defaults.
    #[test]
    fn an_unreadable_candidate_degrades_to_the_next_one() {
        let tmp = tempfile::TempDir::new().unwrap();
        // A DIRECTORY named init.lua: exists, but reading it errors.
        std::fs::create_dir_all(tmp.path().join("defaults").join("init.lua")).unwrap();

        let (source, origin) = load_defaults(&[tmp.path().to_path_buf()]);

        assert_eq!(origin, DefaultsSource::Builtin);
        assert!(source.contains("cru.defaults.system_prompt"));
    }
}
