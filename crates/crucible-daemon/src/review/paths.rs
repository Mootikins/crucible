//! Query-time path normalisation.
//!
//! Roots are stored exactly as `git rev-parse --show-toplevel` printed them:
//! physical, with every symlink already resolved. Queries arrive in whatever
//! spelling the caller happened to have — a workspace registered through a
//! symlink, a tool's relative argument, a client's `..`.
//!
//! Translating the **query** is the only safe place to reconcile the two.
//! Normalising at ingest would rewrite `RootBase.root`, which is the first
//! field hashed into every [`HunkId`](crucible_core::session::HunkId), so every
//! recorded decision would silently stop matching across a resume. Nothing
//! this module produces is ever hashed or persisted.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// How many unresolvable trailing components this will walk past before it
/// gives up and answers with what it has.
///
/// There is a limit at all because this is reached from strings the daemon
/// does not author: `review.comment`'s `path` and `root` arrive over the
/// network, and the gate resolves a model's write-tool argument on every tool
/// call. Walking one component per level is unbounded work on an unbounded
/// input, and the recursive spelling this replaces was unbounded *stack* —
/// which a 2 MiB tokio worker exhausts at roughly 26k components. A Rust stack
/// overflow is a guard-page abort, not a catchable unwind, so that took the
/// whole daemon and every live session with it.
///
/// Well above `PATH_MAX` in components, so no path the filesystem could ever
/// hold reaches it.
const MAX_WALK: usize = 1024;

/// The physical spelling of `path`, resolving symlinks as far as the
/// filesystem can confirm them.
///
/// [`std::fs::canonicalize`] alone is not enough, because a path this is asked
/// about need not exist: a file a tool is about to create, and a file a
/// deletion hunk describes, are both absent from the worktree at the moment
/// the question is asked. So the deepest *existing* ancestor is canonicalised
/// and the remainder re-attached.
///
/// Best-effort by construction — there is no error case. A path with nothing
/// left to re-attach (`/`, `.`, `..`, a bare Windows prefix) comes back
/// unchanged, because the input is then the closest thing to an answer that
/// exists. Callers that must not accept an unresolved `..` check the result
/// against their own root rather than trusting this to have removed it: a `..`
/// whose entire prefix is missing is unresolvable by anyone — and that stays
/// true of a path long enough to hit [`MAX_WALK`], which comes back with its
/// unresolved prefix intact rather than silently flattened.
pub(crate) fn resolve(path: &Path) -> PathBuf {
    let mut trailing: Vec<&OsStr> = Vec::new();
    let mut current = path;
    let mut resolved = loop {
        if let Ok(real) = std::fs::canonicalize(current) {
            break real;
        }
        // `file_name` is `None` exactly for the components that cannot be
        // re-attached to a resolved parent, `..` among them.
        let (Some(name), Some(parent)) = (current.file_name(), current.parent()) else {
            break current.to_path_buf();
        };
        if trailing.len() >= MAX_WALK {
            break current.to_path_buf();
        }
        trailing.push(name);
        current = parent;
    };
    for name in trailing.iter().rev() {
        resolved.push(name);
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn a_symlinked_directory_resolves_to_its_physical_path() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("a.txt"), "x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert_eq!(
            resolve(&link.join("a.txt")),
            std::fs::canonicalize(real.join("a.txt")).unwrap()
        );
    }

    /// The gate is asked about files that do not exist yet — a `write_file`
    /// creating one, a deletion hunk describing one that is gone. Falling back
    /// to the raw input for those would leave the symlink unresolved and the
    /// gate matching nothing.
    #[test]
    fn a_path_that_does_not_exist_yet_resolves_through_its_existing_ancestor() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert_eq!(
            resolve(&link.join("not/created/yet.txt")),
            std::fs::canonicalize(&real)
                .unwrap()
                .join("not/created/yet.txt")
        );
    }

    #[test]
    fn an_existing_physical_path_is_unchanged() {
        let dir = TempDir::new().unwrap();
        let real = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(resolve(&real), real);
    }

    /// `..` inside a prefix that exists is resolved by `canonicalize`; the
    /// residual is a `..` whose whole prefix is missing, which nothing can
    /// resolve. It must come back visibly intact rather than silently dropped,
    /// so the caller's containment check can still refuse it.
    #[test]
    fn dot_dot_over_an_existing_prefix_is_resolved() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            resolve(&nested.join("../..")),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[test]
    fn an_unresolvable_dot_dot_survives_rather_than_being_dropped() {
        let escaped = resolve(Path::new("/no/such/root/../etc/passwd"));
        assert!(
            escaped
                .components()
                .any(|c| c == std::path::Component::ParentDir),
            "{escaped:?} lost the traversal a caller has to refuse"
        );
    }

    /// The recursive spelling of this function overflowed a 2 MiB stack at
    /// roughly 26k components, and a Rust stack overflow is a guard-page abort
    /// rather than a catchable unwind — so a `POST /review/comment` carrying a
    /// long enough `path`, or a model emitting one as a write-tool argument,
    /// killed the daemon and every live session with it.
    ///
    /// Run on a thread sized like a tokio worker on purpose. The test harness's
    /// own stack is larger, so measuring there would answer a question nobody
    /// asked: the hazard is the runtime this actually executes on. The
    /// assertion is simply that it returns — an overflow aborts the process and
    /// no assertion is reached.
    #[test]
    fn an_absurdly_deep_path_answers_instead_of_aborting_the_process() {
        const TOKIO_WORKER_STACK: usize = 2 * 1024 * 1024;
        std::thread::Builder::new()
            .stack_size(TOKIO_WORKER_STACK)
            .spawn(|| {
                let deep = format!("/{}", "a/".repeat(60_000));
                let resolved = resolve(Path::new(&deep));
                assert!(resolved.is_absolute(), "{resolved:?}");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// Past the walk limit the answer is necessarily partial, but it must stay
    /// *visibly* partial: a caller's containment check is the only thing
    /// standing between a `..` and a stored path that escapes its root, and it
    /// can only refuse what it can still see.
    #[test]
    fn a_path_past_the_walk_limit_keeps_its_unresolved_traversal() {
        let deep = format!("/no/such/root/../{}x", "a/".repeat(MAX_WALK + 10));
        let resolved = resolve(Path::new(&deep));
        assert!(
            resolved
                .components()
                .any(|c| c == std::path::Component::ParentDir),
            "{resolved:?} lost the traversal a caller has to refuse"
        );
    }

    #[test]
    fn a_path_with_no_final_component_is_returned_unchanged() {
        assert_eq!(resolve(Path::new("/")), PathBuf::from("/"));
        assert_eq!(
            resolve(Path::new("/no/such/root/..")),
            PathBuf::from("/no/such/root/..")
        );
    }
}
