//! Where the shipped `runtime/` tree lives, relative to the running binary.
//!
//! Four subsystems look for it — plugins, `defaults/init.lua`, bundled skills,
//! and `cru setup`'s copy source — and each used to open-code the candidate
//! list. They drifted: skills discovery tried only the dev layout, so an
//! installed `cru` silently loaded no bundled help skills at all. One list,
//! one order, four callers.
//!
//! Lives in `crucible-core` because two of the four callers are in the CLI.
//!
//! # Why the tree is also compiled in
//!
//! A correct resolver pointed at a directory that does not exist changes
//! nothing, and until now no install method put one there. `cargo-dist`'s
//! `include` key does place files in the release archive, but the shell
//! installer it generates moves only binaries and libraries out of the unpacked
//! directory and then deletes the rest — checked against the 0.31.0 installer
//! this repo ships. `cargo install` never had a data path at all. So every
//! consumer of `runtime/` was dead for anyone who installed Crucible rather
//! than building it.
//!
//! Rather than teach each packaging route to carry 144K of Lua, the tree
//! travels inside the binary and materialises on demand. Three of its files
//! (`defaults/init.lua`, `themes/default.lua`, `statusline/default.lua`) were
//! already `include_str!`'d one at a time for exactly this reason; this is that
//! answer generalised to the other nineteen.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The `runtime/` tree as shipped, compiled into the binary.
///
/// `debug-embed` is on deliberately: without it rust-embed reads from the build
/// machine's source path in debug builds, so the embedded route would only ever
/// execute in release — and "works in dev, dead when installed" is the exact
/// failure this module exists to end.
///
/// Plugin test suites are excluded. They were 297K of the tree's 617K — 48%,
/// mostly `web-search`'s HTML and JSON provider fixtures — carried by every
/// installed binary to no end: a suite is a development artifact, run from a
/// checkout by `cru plugin test runtime/plugins/<name>` or by the
/// `shipped_plugin_lua_suite_passes` gate, neither of which reads the extracted
/// tree. The visible consequence is that `cru plugin test` against a *shipped*
/// plugin on an installed Crucible reports "No test files found"; against a
/// user's own plugin, which is what that command is for, nothing changes.
#[derive(rust_embed::Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../runtime"]
#[exclude = "plugins/*/tests/**"]
struct BundledRuntime;

/// Runtime roots to try for a binary at `exe_dir`, highest priority first.
///
/// Installed before dev: a `~/.local/bin/cru` has a real
/// `~/.local/share/crucible/runtime`, whereas `<exe_dir>/../../runtime` for
/// that same binary is `~/runtime` — a path that is almost always absent and,
/// if it does exist, is not Crucible's.
///
/// Neither is checked for existence here; callers filter, because what counts
/// as present differs (a `plugins/` subdir, a `defaults/init.lua`, any
/// `*/skills`).
pub fn exe_relative(exe_dir: &Path) -> [PathBuf; 2] {
    [
        // Installed: <prefix>/bin/cru → <prefix>/share/crucible/runtime
        exe_dir
            .join("..")
            .join("share")
            .join("crucible")
            .join("runtime"),
        // Dev: <repo>/target/debug/cru → <repo>/runtime
        exe_dir.join("..").join("..").join("runtime"),
    ]
}

/// Where `cru setup` copies the runtime tree: `~/.config/crucible/runtime`.
///
/// A user-owned root, tried before anything shipped alongside the binary, so
/// `cru setup` is a way to *override* the bundled tree and not merely to
/// duplicate it.
pub fn user_runtime() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("crucible").join("runtime"))
}

/// Every root to try for the running binary, highest priority first.
///
/// `cru setup` copied the runtime into `~/.config/crucible/runtime` and then
/// printed instructions to set `CRUCIBLE_RUNTIME` by hand, because no resolver
/// looked there — a path written by one component and read by none, which is
/// the same defect this module exists to prevent. It is a root now, so `cru
/// setup` needs no follow-up.
pub fn for_current_exe() -> Vec<PathBuf> {
    let mut roots = on_disk_roots();
    // Named unconditionally and last: callers already filter for existence, so
    // an unextracted path costs a `stat`, whereas resolving the list is far too
    // common a thing to have write 144K as a side effect. Materialising is
    // [`ensure_bundled_runtime`]'s job.
    roots.extend(bundled_runtime_dir());
    roots
}

/// The roots a packaging route could have put on disk, highest priority first.
fn on_disk_roots() -> Vec<PathBuf> {
    let exe_relative = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(exe_relative))
        .map(Vec::from)
        .unwrap_or_default();

    user_runtime().into_iter().chain(exe_relative).collect()
}

/// Whether `dir` holds a runtime tree rather than merely existing.
///
/// The marker subdirectories, not the directory itself: `~/.config/crucible/runtime`
/// is routinely created empty by a half-finished `cru setup`, and an empty
/// directory that counts as a runtime would shadow the bundled tree with nothing.
pub fn looks_like_runtime(dir: &Path) -> bool {
    ["plugins", "themes", "defaults", "crucible-help"]
        .iter()
        .any(|marker| dir.join(marker).is_dir())
}

/// Whether the compiled-in tree is worth extracting, given what is on disk.
///
/// Last resort by construction: a user's `cru setup` tree, an installed
/// `share/crucible/runtime`, and the dev checkout all outrank it, so nobody who
/// already has a real tree pays for the extraction or risks a stale copy of it.
fn needs_bundled_runtime(on_disk: &[PathBuf]) -> bool {
    !on_disk.iter().any(|dir| looks_like_runtime(dir))
}

/// Where the compiled-in tree lands once something extracts it.
///
/// A path, not a promise that anything is there. Version-stamped so two
/// installed Crucibles do not overwrite each other's copy on every launch.
pub fn bundled_runtime_dir() -> Option<PathBuf> {
    Some(
        dirs::data_dir()?
            .join("crucible")
            .join(concat!("runtime-", env!("CARGO_PKG_VERSION"))),
    )
}

/// Materialise the compiled-in tree if no packaging route supplied one.
///
/// The single writing entry point on the automatic path, called once at daemon
/// startup rather than from resolution, so that merely asking where the runtime
/// lives stays free of side effects. Returns `None` when a real tree already
/// answers — there is nothing to extract — and when extraction fails.
///
/// Memoised: repeated calls in one process are a no-op.
pub fn ensure_bundled_runtime() -> Option<PathBuf> {
    static MATERIALISED: OnceLock<Option<PathBuf>> = OnceLock::new();
    MATERIALISED
        .get_or_init(|| ensure_bundled_runtime_at(bundled_runtime_dir(), &on_disk_roots()))
        .clone()
}

/// [`ensure_bundled_runtime`] with its two environment lookups passed in.
///
/// Split out so the decision and the extraction can be tested joined, without
/// writing to the developer's real data directory.
fn ensure_bundled_runtime_at(target: Option<PathBuf>, on_disk: &[PathBuf]) -> Option<PathBuf> {
    if !needs_bundled_runtime(on_disk) {
        return None;
    }
    let target = target?;
    match sync_bundled_runtime(&target) {
        Ok(()) => Some(target),
        Err(err) => {
            // Not fatal: every consumer already handles "no runtime root", and
            // a read-only or full data directory should degrade the way a
            // missing tree does rather than take the daemon down.
            tracing::warn!(
                target = %target.display(),
                error = %err,
                "could not write the bundled runtime tree"
            );
            None
        }
    }
}

/// Write the compiled-in tree to `target` unconditionally.
///
/// What `cru setup` wants: the user asked for the shipped files back, so
/// whatever they did to the copy is overwritten. Use [`sync_bundled_runtime`]
/// for the automatic path, where re-writing an unchanged tree on every launch
/// is waste.
///
/// Files are overwritten in place rather than the directory being cleared
/// first — deleting a tree the caller named is not this function's to do, and a
/// file dropped between releases is handled by the version in
/// [`bundled_runtime`]'s path rather than by deletion here.
pub fn write_bundled_runtime(target: &Path) -> std::io::Result<()> {
    let stamp = bundled_stamp();
    write_embedded_tree::<BundledRuntime>(target, &stamp)
}

/// Write an embedded tree to `target`, stamping it last.
///
/// Shared by the runtime tree and the bundled docs, which differ only in which
/// `Embed` they carry. The stamp goes last on purpose: an interrupted
/// extraction leaves none, so the next run redoes it rather than trusting a
/// half-written tree.
pub(crate) fn write_embedded_tree<E: rust_embed::RustEmbed>(
    target: &Path,
    stamp: &str,
) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for path in E::iter() {
        let file = E::get(&path).expect("rust-embed only iterates paths it can also get");
        let dest = target.join(path.as_ref());
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(dest, file.data)?;
    }
    std::fs::write(target.join(STAMP_FILE), stamp)
}

/// Identity of an embedded tree: this crate's version plus a hash of content.
///
/// The version alone is not enough — an unreleased rebuild changes the files
/// without changing the version, and a stamp that cannot see that would serve
/// the previous build's content out of a directory named for the current one.
pub(crate) fn embedded_stamp<E: rust_embed::RustEmbed>() -> String {
    let mut hasher = blake3::Hasher::new();
    // `iter()` yields in a stable order, but hashing is order-sensitive and a
    // reordering would read as a content change; sort so the stamp tracks the
    // files rather than the crate's iteration.
    let mut paths: Vec<_> = E::iter().collect();
    paths.sort();
    for path in paths {
        hasher.update(path.as_bytes());
        if let Some(file) = E::get(&path) {
            hasher.update(&file.data);
        }
    }
    format!(
        "{} {}",
        env!("CARGO_PKG_VERSION"),
        hasher.finalize().to_hex()
    )
}

/// Name of the marker file recording which build wrote a tree.
pub(crate) const STAMP_FILE: &str = ".stamp";

/// Identity of the compiled-in runtime tree.
fn bundled_stamp() -> String {
    embedded_stamp::<BundledRuntime>()
}

/// Make `target` hold this build's tree, writing only if it does not already.
///
/// The stamp is a claim about which build wrote the tree, not a checksum of
/// what is there now: a user who edits the extracted copy keeps their edits
/// until the next release. That is the right trade for a directory Crucible
/// creates on its own — `cru setup` is how you ask for a reset.
pub fn sync_bundled_runtime(target: &Path) -> std::io::Result<()> {
    if std::fs::read_to_string(target.join(STAMP_FILE)).is_ok_and(|w| w == bundled_stamp()) {
        return Ok(());
    }
    write_bundled_runtime(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cru setup`'s target outranks anything shipped with the binary.
    ///
    /// It is the user saying "use this tree", so it has to be able to shadow
    /// the bundled one rather than merely sit behind it.
    #[test]
    fn the_user_runtime_is_tried_before_anything_shipped() {
        let Some(user) = user_runtime() else {
            return; // no config dir on this box; nothing to order
        };
        let roots = for_current_exe();
        assert_eq!(
            roots.first(),
            Some(&user),
            "cru setup's target is the highest-priority root"
        );
    }

    #[test]
    fn the_installed_layout_is_tried_before_the_dev_tree() {
        let roots = exe_relative(Path::new("/usr/local/bin"));
        assert_eq!(
            roots[0],
            Path::new("/usr/local/bin/../share/crucible/runtime")
        );
        assert_eq!(roots[1], Path::new("/usr/local/bin/../../runtime"));
    }

    /// The whole point: a binary with no runtime tree beside it can still
    /// produce one, with every file each of the four consumers looks for.
    ///
    /// Named by file rather than by directory because "a `plugins/` exists" is
    /// what the old resolver could already say while the plugins themselves
    /// were absent from the release.
    #[test]
    fn the_compiled_in_tree_carries_every_file_the_consumers_need() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_bundled_runtime(tmp.path()).expect("extract");

        for needed in [
            "defaults/init.lua",
            "themes/default.lua",
            "themes/opencode.lua",
            "statusline/default.lua",
            "plugins/reflection/plugin.yaml",
            "plugins/reflection/lua/config.lua",
            "plugins/oci/init.lua",
            "plugins/reflection/plugin.yaml",
            "plugins/worktree/init.lua",
            "plugins/worktree/lua/git.lua",
            "crucible-help/skills/crucible-help/SKILL.md",
        ] {
            assert!(
                tmp.path().join(needed).is_file(),
                "extracted tree is missing {needed}"
            );
        }

        assert!(
            looks_like_runtime(tmp.path()),
            "the extracted tree has to satisfy the same check the resolver uses"
        );
    }

    /// Byte-for-byte, not merely present — an extraction that mangles line
    /// endings or truncates would still pass a file-exists check and then fail
    /// in the Lua VM, which is where this bug class always surfaces.
    #[test]
    fn the_extracted_files_match_the_source_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_bundled_runtime(tmp.path()).expect("extract");

        let extracted = std::fs::read_to_string(tmp.path().join("defaults/init.lua"))
            .expect("read extracted init.lua");
        assert_eq!(
            extracted,
            include_str!("../../../runtime/defaults/init.lua"),
            "the extracted defaults must be the shipped defaults"
        );
    }

    /// Plugin test suites are excluded from the embed, and nothing else is.
    ///
    /// The suites were 297K of the tree's 617K, carried by every installed
    /// binary for nothing. The second half of this test is the important half:
    /// an over-broad exclude glob that also dropped `init.lua` would leave
    /// every shipped plugin unloadable on an installed Crucible, and the size
    /// win would look like a success.
    #[test]
    fn the_embed_carries_plugin_code_but_not_plugin_tests() {
        let embedded: Vec<String> = BundledRuntime::iter().map(|p| p.to_string()).collect();
        assert!(!embedded.is_empty(), "the embed must not be empty");

        let tests: Vec<&String> = embedded.iter().filter(|p| p.contains("/tests/")).collect();
        assert!(
            tests.is_empty(),
            "plugin test files must not be compiled into the binary: {tests:?}"
        );

        for required in [
            "plugins/oci/init.lua",
            "plugins/oci/plugin.yaml",
            "plugins/oci/lua/container.lua",
            "plugins/graph-view/init.fnl",
            "plugins/web-search/lua/providers/ddg.lua",
            "defaults/init.lua",
        ] {
            assert!(
                embedded.iter().any(|p| p == required),
                "the exclude glob dropped '{required}', which a shipped plugin needs"
            );
        }
    }

    /// A stale copy from an older build must not survive, and an up-to-date one
    /// must not be rewritten on every launch.
    #[test]
    fn re_extracting_replaces_a_stale_tree_and_leaves_a_current_one_alone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        sync_bundled_runtime(tmp.path()).expect("first extract");

        let init = tmp.path().join("defaults/init.lua");
        std::fs::write(&init, "-- stale").expect("corrupt the copy");

        sync_bundled_runtime(tmp.path()).expect("second extract");
        assert_eq!(
            std::fs::read_to_string(&init).expect("read"),
            "-- stale",
            "an unchanged binary must not rewrite the tree it already wrote"
        );

        std::fs::remove_file(tmp.path().join(".stamp")).expect("drop the stamp");
        sync_bundled_runtime(tmp.path()).expect("third extract");
        assert_eq!(
            std::fs::read_to_string(&init).expect("read"),
            include_str!("../../../runtime/defaults/init.lua"),
            "a missing or mismatched stamp must force a full rewrite"
        );
    }

    /// A real tree on disk always wins; the compiled-in copy is the answer to
    /// "nothing is installed", not a competitor to what is.
    #[test]
    fn a_runtime_on_disk_suppresses_the_bundled_one() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("plugins")).expect("mkdir");

        assert!(
            !needs_bundled_runtime(&[tmp.path().to_path_buf()]),
            "an installed or dev tree must not be shadowed by an extracted copy"
        );
    }

    /// Candidate paths that do not exist — the installed layout for a dev
    /// binary, say — leave the bundled tree as the only answer, so extraction
    /// is warranted.
    #[test]
    fn nothing_on_disk_calls_for_the_bundled_tree() {
        let absent = PathBuf::from("/nonexistent/share/crucible/runtime");
        assert!(needs_bundled_runtime(&[absent]));
    }

    /// The extracted copy goes last, so any root that appears later — a distro
    /// package, a `cru setup` run — takes precedence the moment it exists.
    #[test]
    fn the_bundled_tree_is_the_lowest_priority_root() {
        let Some(bundled) = bundled_runtime_dir() else {
            return; // no data dir on this box
        };
        let roots = for_current_exe();
        assert_eq!(roots.last(), Some(&bundled));
    }

    /// Deciding to extract and actually extracting, joined.
    ///
    /// Both halves are tested above; this is the hop between them, which is
    /// the hop this whole class of defect goes missing at.
    #[test]
    fn a_binary_with_nothing_on_disk_materialises_the_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("runtime-x.y.z");
        let absent = PathBuf::from("/nonexistent/share/crucible/runtime");

        let root = ensure_bundled_runtime_at(Some(target.clone()), &[absent]);

        assert_eq!(root.as_ref(), Some(&target));
        assert!(target
            .join("plugins")
            .join("oci")
            .join("init.lua")
            .is_file());
    }

    /// The dev checkout and any installed tree win, and nothing is written.
    #[test]
    fn a_binary_with_a_tree_on_disk_materialises_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let on_disk = tmp.path().join("installed");
        let target = tmp.path().join("runtime-x.y.z");
        std::fs::create_dir_all(on_disk.join("plugins")).expect("mkdir");

        let root = ensure_bundled_runtime_at(Some(target.clone()), &[on_disk]);

        assert_eq!(root, None);
        assert!(!target.exists(), "nothing on disk needed replacing");
    }

    /// Asking where the runtime is must not create one.
    ///
    /// Resolution runs from test binaries, from `cru --help`, from anything
    /// that touches the config — none of which asked for 144K to appear in the
    /// user's data directory. Materialising is [`ensure_bundled_runtime`]'s
    /// job and nothing else's; this list only has to *name* the destination.
    #[test]
    fn resolving_roots_names_the_bundled_tree_without_creating_it() {
        let Some(bundled) = bundled_runtime_dir() else {
            return; // no data dir on this box; nothing to create
        };
        let existed = bundled.exists();

        let roots = for_current_exe();

        assert_eq!(
            bundled.exists(),
            existed,
            "resolving roots wrote to {}",
            bundled.display()
        );
        assert!(
            roots.contains(&bundled),
            "the bundled tree must still be reachable once something extracts it"
        );
    }

    /// An empty `~/.config/crucible/runtime` is not a runtime.
    ///
    /// `cru setup` creates the directory before it copies, so a run that dies
    /// partway leaves one behind; treating it as a hit would make the bundled
    /// tree unreachable for exactly the users most likely to need it.
    #[test]
    fn an_empty_directory_does_not_count_as_a_runtime() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(!looks_like_runtime(tmp.path()));
    }
}
