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
//! 3. the tree extracted from the binary, for an install that put none on disk
//! 4. the compiled-in copy ([`crucible_lua::BUILTIN_INIT_LUA`])
//!
//! 3 and 4 are the same bytes by different routes: the constant covers this one
//! file, the extracted tree covers the other twenty-one, which have no such
//! fallback. The constant stays because it needs no successful write.
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
    // Installed layout first, then the dev tree; see `runtime_roots`.
    defaults_candidates_from(
        runtimepath,
        env_runtime,
        &crucible_core::runtime_roots::for_current_exe(),
    )
}

/// `defaults_candidates` with the exe-relative roots supplied rather than
/// discovered.
///
/// Tests must use this. The discovering version always appends the real
/// installed runtime directory, so a test asserting "nothing is installed"
/// passes on CI and fails on any machine where someone has run `cru` — which is
/// every developer's machine, and is exactly how this was found.
fn defaults_candidates_from(
    runtimepath: &[PathBuf],
    env_runtime: Option<&str>,
    exe_roots: &[PathBuf],
) -> Vec<PathBuf> {
    use crucible_core::runtime_path::{
        search_paths, EntryShape, Origin, RuntimeAsset, RuntimeEntry,
    };

    // The roots, in the order this resolver has always used: configured
    // entries, then `$CRUCIBLE_RUNTIME`, then exe-relative and bundled.
    //
    // Built here rather than through `runtime_path::daemon_path` because that
    // one carries `~/.config/crucible` as a `UserConfig` root, and
    // `Defaults::reaches` refuses it — `defaults/` names Crucible's own
    // defaults, and the user's entry point is `init.lua` beside it.
    let mut path: Vec<RuntimeEntry> = runtimepath
        .iter()
        .enumerate()
        .map(|(i, root)| RuntimeEntry::root(root.clone(), Origin::Config(i)))
        .collect();
    if let Some(base) = env_runtime {
        path.push(RuntimeEntry::root(PathBuf::from(base), Origin::Env));
    }
    path.extend(
        exe_roots
            .iter()
            .map(|root| RuntimeEntry::root(root.clone(), Origin::Bundled)),
    );

    // Both entry-point names at every root, preferred first: a copied-out
    // `defaults/init.luau` was invisible while this looked for one name. The
    // names come from the shape rather than being spelled here, so the table
    // and this resolver cannot disagree about what an entry file is called.
    let names: Vec<String> = match RuntimeAsset::Defaults.shape() {
        EntryShape::LuaEntryFile => crucible_lua::source_files::init_file_names().to_vec(),
        EntryShape::DirWithMarker(_)
        | EntryShape::PluginDir
        | EntryShape::NoteFiles
        | EntryShape::LuaSourceFiles => {
            unreachable!("Defaults is a LuaEntryFile; the table changed under this resolver")
        }
    };

    // `search_paths` consults no filesystem, which is what this resolver needs:
    // it records candidates that do NOT exist, because the file an agent plants
    // is by definition the file that was not there.
    let candidates: Vec<PathBuf> = search_paths(RuntimeAsset::Defaults, &path)
        .into_iter()
        .flat_map(|dir| {
            names
                .iter()
                .map(move |name| dir.path.join(name))
                .collect::<Vec<_>>()
        })
        .collect();

    // Each candidate is Lua the daemon VM executes before the user's config,
    // so the write-protected set has to name it — including the ones that do
    // not exist, which are exactly the ones an agent would plant. See
    // [`crate::execution_roots`] for why the loader records rather than the
    // protected set rebuilding the list.
    crate::execution_roots::record(candidates.iter().cloned());

    candidates
}

/// Load the defaults source, and say where it came from.
///
/// An unreadable candidate is skipped rather than fatal — a half-installed
/// runtime directory should degrade to the built-in copy, not leave sessions
/// with no defaults at all.
pub fn load_defaults(runtimepath: &[PathBuf]) -> (String, DefaultsSource) {
    let env_runtime = std::env::var("CRUCIBLE_RUNTIME").ok();
    load_defaults_from(
        runtimepath,
        env_runtime.as_deref(),
        &crucible_core::runtime_roots::for_current_exe(),
    )
}

/// `load_defaults` with the environment supplied rather than read. See
/// [`defaults_candidates_from`] for why tests need this.
fn load_defaults_from(
    runtimepath: &[PathBuf],
    env_runtime: Option<&str>,
    exe_roots: &[PathBuf],
) -> (String, DefaultsSource) {
    for candidate in defaults_candidates_from(runtimepath, env_runtime, exe_roots) {
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

    /// Root order beats extension order.
    ///
    /// Each root offers BOTH entry-point names, `.luau` first, but a
    /// higher-priority root's `.lua` still outranks a lower one's `.luau` —
    /// otherwise adding a file to a low-priority root could displace the
    /// defaults a user copied out.
    #[test]
    fn runtimepath_entries_outrank_everything_else() {
        let rtp = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let candidates = defaults_candidates(&rtp, Some("/env"));

        assert_eq!(candidates[0], PathBuf::from("/a/defaults/init.luau"));
        assert_eq!(candidates[1], PathBuf::from("/a/defaults/init.lua"));
        assert_eq!(candidates[2], PathBuf::from("/b/defaults/init.luau"));
        assert_eq!(candidates[3], PathBuf::from("/b/defaults/init.lua"));
        assert_eq!(candidates[4], PathBuf::from("/env/defaults/init.luau"));
        assert_eq!(candidates[5], PathBuf::from("/env/defaults/init.lua"));
    }

    #[test]
    fn env_runtime_is_used_when_no_runtimepath_is_configured() {
        let candidates = defaults_candidates(&[], Some("/env"));
        assert_eq!(candidates[0], PathBuf::from("/env/defaults/init.luau"));
        assert_eq!(candidates[1], PathBuf::from("/env/defaults/init.lua"));
    }

    #[test]
    fn exe_relative_candidates_are_the_last_resort_before_builtin() {
        let candidates = defaults_candidates(&[], None);
        // Each root offers both names, so the LAST pair is the extracted
        // tree's; `last` is its `.lua`.
        let (last, rest) = candidates
            .split_last()
            .expect("the exe-relative pair is always offered");

        // The tree extracted from the binary is the one candidate that does not
        // sit under a plain `runtime/` — its directory is version-stamped, so
        // two installed Crucibles do not fight over one copy. It must come
        // last: anything a packaging route actually put on disk outranks a copy
        // Crucible wrote for itself.
        match crucible_core::runtime_roots::bundled_runtime_dir() {
            Some(bundled) => {
                assert_eq!(
                    last,
                    &bundled.join("defaults").join("init.lua"),
                    "the extracted copy is the last file candidate, ahead of only the built-in"
                );
                assert!(
                    rest.iter().all(|c| {
                        c.ends_with("runtime/defaults/init.lua")
                            || c.ends_with("runtime/defaults/init.luau")
                            || c == &bundled.join("defaults").join("init.luau")
                    }),
                    "expected only exe-relative candidates ahead of it, got {rest:?}"
                );
            }
            // No data directory on this box, so nothing to extract to.
            None => assert!(
                candidates.iter().all(|c| {
                    c.ends_with("runtime/defaults/init.lua")
                        || c.ends_with("runtime/defaults/init.luau")
                }),
                "expected only exe-relative candidates, got {candidates:?}"
            ),
        }
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
            !source.contains("cru.on("),
            "shadowing must REPLACE the shipped defaults, not append to them"
        );
    }

    /// A bare binary with no runtime directory still gets defaults.
    #[test]
    fn falls_back_to_the_builtin_when_nothing_is_installed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (source, origin) = load_defaults_from(&[tmp.path().to_path_buf()], None, &[]);

        assert_eq!(origin, DefaultsSource::Builtin);
        assert!(
            source.contains("cru.on(\"precognition_format\""),
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

        let (source, origin) = load_defaults_from(&[tmp.path().to_path_buf()], None, &[]);

        assert_eq!(origin, DefaultsSource::Builtin);
        assert!(source.contains("cru.on(\"precognition_format\""));
    }
}
