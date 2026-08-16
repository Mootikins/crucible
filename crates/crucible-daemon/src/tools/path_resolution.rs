//! Resolving a caller-supplied path into the forms a containment check can
//! trust.
//!
//! The rule this module exists to enforce: **never judge a path on one form.**
//!
//! The predecessor canonicalized the deepest existing ancestor and re-appended
//! the remaining components verbatim, `..` included. That is a
//! canonicalization that fails open — `{root}/not-yet/../../secret` came back
//! with its `..` intact, `starts_with(root)` said yes, and the kernel then
//! walked up out of the root with the very same string. The shape has shipped
//! CVEs against three vendors (Cursor CVE-2026-50549, OpenClaw
//! GHSA-575v-8hfq-m3mc, Anthropic's own `server-filesystem` CVE-2025-53110),
//! so it is replaced rather than patched: re-appending an un-normalized tail is
//! structural to the technique, and hardening it in place rebuilds the bug.
//!
//! What replaces it is two forms:
//!
//! - [`absolutize`] — pure lexical normalization that clamps `..` at the root.
//!   Touches no filesystem, so it is defined on paths that do not exist yet.
//!   Deliberately: a check that can only judge existing paths is a check the
//!   agent dodges by naming a directory it is about to create.
//! - [`ResolvedPath`] — that lexical form paired with the canonical one, walked
//!   back through the deepest existing ancestor so symlinks are seen. The order
//!   is the fix: normalize *first*, resolve the already-normalized path
//!   *second*, so there is no `..` left for the ancestor walk to re-append.
//!   Both forms are then checked, because neither subsumes the other — the
//!   lexical one is blind to symlinks, and the canonical one answers where a
//!   path lands rather than what it names, which is what a denied root needs.
//!
//! The verdict those two forms are judged into — and the root set they are
//! judged against — live next door in [`super::containment`].

use std::path::{Component, Path, PathBuf};

/// Pure lexical normalization: drop `.`, pop on `..`, clamp at the root.
///
/// `/a/../../b` is `/b`, not `/../b` — an absolute path can never be
/// normalized into an ancestor of its own root. Relative paths keep a leading
/// `..` they cannot pop, because dropping it would silently rewrite
/// `../secret` into `secret`, a different path entirely.
///
/// This is a pre-check and an error-message helper. It is **not** the
/// boundary: it cannot see a symlink. Pair it with the canonical form via
/// [`ResolvedPath`].
pub(crate) fn absolutize(path: &Path) -> PathBuf {
    let mut out: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                out.clear();
                out.push(component);
            }
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                // The clamp. `..` at the root is the root.
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // Nothing to pop, and no root to clamp against: a relative
                // path that genuinely names an ancestor. Keep it, so the
                // caller's check sees an upward path rather than a rewritten
                // downward one.
                None | Some(Component::ParentDir | Component::CurDir) => out.push(component),
            },
            Component::Normal(_) => out.push(component),
        }
    }
    out.into_iter().collect()
}

/// Canonicalize as much of `lexical` as exists, then re-attach the tail.
///
/// Zed's `canonicalize_with_ancestors`: walk up until a prefix canonicalizes,
/// so a path whose intermediate components do not exist is still judged where
/// it actually lands rather than being waved through unresolved.
///
/// Re-attaching a tail is only safe because `lexical` has been through
/// [`absolutize`] first, which is exactly what the predecessor did not do: it
/// walked the RAW path, so the tail it re-attached could still contain `..`
/// and the result named somewhere other than where it had checked. Every
/// component here is `Normal`, and `canonicalize` never yields `..`, so the
/// result is normalized by construction — asserted below rather than left to
/// a comment, because that assumption is the entire fix.
fn resolve_existing_ancestors(lexical: &Path) -> PathBuf {
    resolve_existing_ancestors_within(lexical, MAX_DANGLING_LINK_HOPS)
}

/// Link hops spent on targets that do not exist.
///
/// `canonicalize` has the kernel's budget (40 on Linux) and never reaches this
/// walk — only a link whose target is *missing* gets here, so a cycle is the
/// one way to spend many hops. Small enough to end one, large enough that no
/// honest chain of not-yet-created links hits it.
const MAX_DANGLING_LINK_HOPS: usize = 16;

fn resolve_existing_ancestors_within(lexical: &Path, hops: usize) -> PathBuf {
    debug_assert!(
        !lexical
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "resolve_existing_ancestors requires an absolutized path, got {}",
        lexical.display()
    );
    let mut existing = lexical.to_path_buf();
    let mut remainder: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(canonical) = existing.canonicalize() {
            let mut result = canonical;
            for part in remainder.iter().rev() {
                result.push(part);
            }
            return result;
        }

        // A DANGLING symlink is the one thing the ancestor walk cannot see.
        // `canonicalize` fails on it for the same reason it fails on a name
        // that does not exist, so the arm below re-appends the link's OWN
        // name — and both resolved forms then say "inside the root" while
        // `fs::write` follows the link and creates the target outside it.
        // Nothing here needs `bash` or a prior write: a git checkout can carry
        // the symlink, and the target's non-existence is the whole trick.
        //
        // Following it is what keeps this module's promise that the path
        // checked is the path used, and what leaves the symlink escape a
        // reachable OUTCOME rather than a silent permit — design §5, the shape
        // behind CVE-2026-39861 and CVE-2026-50549.
        if hops > 0 {
            if let Some(followed) = follow_link(&existing, &remainder) {
                return resolve_existing_ancestors_within(&followed, hops - 1);
            }
        }

        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                remainder.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            // No ancestor canonicalized at all. The lexical form is all there
            // is, and it is already clamped.
            _ => return lexical.to_path_buf(),
        }
    }
}

/// What `link` points at with `remainder` re-attached, or `None` when `link` is
/// not a symlink.
///
/// A relative target resolves against the link's own directory, which is what
/// the kernel does. The result goes back through [`absolutize`] so the
/// recursive call keeps its precondition — a target may be spelled with `..`.
fn follow_link(link: &Path, remainder: &[std::ffi::OsString]) -> Option<PathBuf> {
    if !link
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return None;
    }
    let target = link.read_link().ok()?;
    let mut followed = if target.is_absolute() {
        target
    } else {
        link.parent()
            .map_or_else(|| target.clone(), |dir| dir.join(&target))
    };
    for part in remainder.iter().rev() {
        followed.push(part);
    }
    Some(absolutize(&followed))
}

/// A caller-supplied path in both the forms containment must be judged on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedPath {
    lexical: PathBuf,
    canonical: PathBuf,
}

impl ResolvedPath {
    /// Resolve `path`, anchoring a relative one at the process working
    /// directory so both forms are absolute and comparable.
    pub(crate) fn resolve(path: &Path) -> Self {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
        };
        let lexical = absolutize(&absolute);
        let canonical = resolve_existing_ancestors(&lexical);
        Self { lexical, canonical }
    }

    /// The `..`-clamped form. Blind to symlinks, defined on paths that do not
    /// exist.
    pub(crate) fn lexical(&self) -> &Path {
        &self.lexical
    }

    /// The form after resolving through the deepest existing ancestor. This is
    /// the path a permitted operation should actually open, so that the path
    /// checked is the path used.
    pub(crate) fn canonical(&self) -> &Path {
        &self.canonical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex's `parent_dir_above_root_stays_at_root`, which is the property the
    /// predecessor lacked: `..` may not climb past the root, and the result of
    /// normalizing an absolute path is always still under it.
    #[test]
    fn parent_dir_above_root_stays_at_root() {
        assert_eq!(absolutize(Path::new("/../../path")), PathBuf::from("/path"));
        assert_eq!(absolutize(Path::new("/..")), PathBuf::from("/"));
        assert_eq!(absolutize(Path::new("/a/../../b")), PathBuf::from("/b"));
    }

    #[test]
    fn normalization_drops_current_dir_and_pops_parent_dir() {
        assert_eq!(
            absolutize(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c"),
            "`.` is dropped and `..` pops the component before it"
        );
        assert_eq!(
            absolutize(Path::new("/a/b/c/../../d")),
            PathBuf::from("/a/d")
        );
    }

    /// A relative path has no root to clamp against, so a `..` it cannot pop
    /// must survive. Dropping it would turn `../secret` into `secret` — a path
    /// that points somewhere else and, inside a workspace, somewhere allowed.
    #[test]
    fn a_relative_parent_dir_with_nothing_to_pop_is_kept() {
        assert_eq!(
            absolutize(Path::new("../secret")),
            PathBuf::from("../secret")
        );
        assert_eq!(absolutize(Path::new("a/../../b")), PathBuf::from("../b"));
        assert_eq!(absolutize(Path::new("../../x")), PathBuf::from("../../x"));
    }

    /// Normalization must not need the path to exist — the agent can create
    /// the missing component afterwards, which is exactly how the predecessor
    /// was finished off.
    #[test]
    fn a_path_that_does_not_exist_yet_is_still_normalized() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let dodge = root.join("not-yet").join("..").join("sessions");

        assert_eq!(
            ResolvedPath::resolve(&dodge).lexical(),
            root.join("sessions"),
            "a `..` through a missing directory must be clamped, not carried"
        );
        assert_eq!(
            ResolvedPath::resolve(&dodge).canonical(),
            root.join("sessions"),
            "and the canonical form must agree rather than re-appending it"
        );
    }

    /// The second pass earns its keep: clamping `not-yet/..` exposes
    /// `sessions`, which the first ancestor walk never looked at because it
    /// stopped short of it.
    #[cfg(unix)]
    #[test]
    fn a_symlink_exposed_by_clamping_a_parent_dir_is_still_resolved() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let real = root.join("real");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, root.join("link")).unwrap();

        let through = root.join("not-yet").join("..").join("link").join("leaf");

        assert_eq!(
            ResolvedPath::resolve(&through).canonical(),
            real.join("leaf"),
            "the symlink behind the clamped `..` must be resolved, not left literal"
        );
    }
}
