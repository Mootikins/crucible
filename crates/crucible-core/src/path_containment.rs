//! One chokepoint for "is this caller-supplied path inside the directory I am
//! willing to touch?".
//!
//! Every crate that accepts a path off the wire — daemon RPC, web routes, Lua
//! bindings — used to answer that question with its own validator. They drifted
//! into a dozen subtly different answers, several of which were wrong in ways
//! the others were not. This module is the single answer.
//!
//! # Use the return value
//!
//! [`resolve_within`] returns the **canonical** path: canonical up to the
//! deepest component that exists on disk, with any not-yet-created tail
//! appended to that verified prefix. The input path is never a valid thing to
//! hand to a syscall — the whole point is that the path which was checked and
//! the path which is used cannot differ.
//!
//! # An absolute `user_path` is accepted — read this before migrating
//!
//! This is the one place where the module is **more permissive** than two of
//! the validators it replaces. `agent_manager/attachments.rs::resolve_in_workspace`
//! and `canvas/containment.rs::resolve_file_ref` both refuse an absolute input
//! categorically; [`resolve_within`] narrows it against the root and, if it
//! strips, proceeds to prove containment by walking it.
//!
//! Why not just refuse: ACP agents legitimately send absolute in-workspace
//! paths — that is the shape of an `edit` tool call from Claude Code — so a
//! blanket refusal would break `acp/client/tools.rs` and the diff previews
//! that flow from it. And a categorical refusal is not the *security* control
//! it resembles: `strip_prefix` proves nothing (`$ROOT/../etc/passwd` strips
//! cleanly and is then refused by the `..` rule, not by the strip), while the
//! component walk that follows is what actually proves containment, and it
//! treats a narrowed absolute path and a relative path identically. Refusing
//! absolute inputs buys a smaller input surface, not a stronger proof.
//!
//! **If you are migrating a refuse-absolute validator to this module, you are
//! widening its accepted input by one shape.** That is a wire-contract change,
//! not a containment change: a client that could previously only name paths
//! relative to a root it was never told the location of can now name paths
//! that confirm where that root is on disk (`$ROOT/x` accepted, `/etc/x`
//! refused, which is a probe). If your caller's contract says "relative only",
//! keep your own `user_path.is_absolute()` refusal at your boundary *before*
//! calling — this module will not make that refusal on your behalf, and it
//! never did decide your wire format.
//!
//! # Which root policy lives here, and which does not
//!
//! [`resolve_within`] refuses two kinds of root: a **relative** one (it names
//! a different directory for every value the process CWD happens to hold — for
//! the daemon, a directory no operator chose) and the **filesystem root**
//! (every absolute path strips against `/` and every canonical target
//! `starts_with("/")`, so the function returns `Ok` for the whole filesystem
//! while looking like it checked).
//!
//! Both are refused because they are *structural*: the containment mechanism
//! itself stops functioning, provably, from the path alone. That test needs no
//! knowledge of who is running, what a project is, or what the product wants.
//!
//! `$HOME`, `/etc`, `/usr` and the other roots in
//! `crucible-daemon`'s `project_manager::forbidden_root_reason` are **not**
//! duplicated here, and crucible-daemon must not be imported to reach them.
//! They are a different kind of rule: a root of `$HOME` bounds containment
//! perfectly well — the mechanism works, and every path it admits really is
//! inside `$HOME`. What is wrong with it is that Crucible should not be
//! *handed* that root in the first place, which is an authorization question
//! (see "What this CANNOT close" below), answered against the invoking user's
//! home directory — knowledge crucible-core does not and should not have.
//! Sinking it here would also make the rule un-testable in its real form:
//! it would have to read `$HOME` from the environment inside a leaf function
//! called on every path.
//!
//! So: callers apply `forbidden_root_reason` (or their own policy) when a root
//! is *chosen* — at registration, at config load — and this module enforces
//! only the floor that no policy can waive. The one deliberate overlap is
//! `path.parent().is_none()`, which both check; a structural floor is worth
//! stating twice.
//!
//! # There is no symlink resolver here
//!
//! An earlier attempt hand-rolled a hop-counting link walker, and the escape
//! lived in that exotic machinery rather than in anything the tests thought to
//! probe. So: [`std::fs::symlink_metadata`] is used only as a *detector* ("is
//! this component a link at all?"), and every actual resolution is delegated to
//! [`std::path::Path::canonicalize`], i.e. to the kernel, which enforces
//! `ELOOP` itself. No hop budget, no chain walking. That class of bug is
//! designed out rather than fixed.
//!
//! # Only `NotFound` means "does not exist"
//!
//! The other earlier attempt wrote `if let Ok(meta) = symlink_metadata(p)`,
//! which silently skipped the symlink check for *every* error — so an
//! over-long path (`ENAMETOOLONG`, which is [`std::io::ErrorKind`]
//! `InvalidFilename`, **not** `NotFound`) sailed straight through. Here,
//! `ErrorKind::NotFound` is the one and only error that may be read as "the
//! rest of this path is yet to be created". Everything else — `ENOTDIR`,
//! `ENAMETOOLONG`, `EACCES`, and any errno this code has never heard of —
//! becomes [`Refusal::Undecidable`]. The catch-all arm is what makes the
//! function fail closed by construction.
//!
//! # Exotic spellings are normalized away before any syscall
//!
//! `lstat("evil.md/")` and `lstat("evil.md/.")` both fail with `ENOTDIR` when
//! `evil.md` is a symlink to a file, because the trailing separator demands the
//! final component be a directory. A guard that only inspects the raw byte path
//! therefore never sees the link. This module rebuilds the path from
//! [`std::path::Component::Normal`] values before touching the filesystem, so
//! the `lstat` is issued against `evil.md` and the link is seen.
//!
//! # What this CANNOT close
//!
//! Stated plainly, because a confident list that quietly omits one is how the
//! previous rounds of this work died.
//!
//! - **Hard links — no path-layer fix exists.** A hard link is a second
//!   directory entry for the same inode. `symlink_metadata` reports a plain
//!   regular file, `canonicalize` returns the in-root path, and containment
//!   passes — *correctly*, because the entry genuinely is inside the root.
//!   There is no path-layer signal to test. This module will bless
//!   `$KILN/notes.md` when it is a hard link to `~/.bashrc`, and a subsequent
//!   `fs::write` will rewrite the shared inode. The layer that must handle it
//!   is the **write syscall at the sink**: write to a temp file in the same
//!   directory and `rename()` over the target. `rename` replaces the directory
//!   entry rather than the inode, which breaks the hard link — and closes the
//!   TOCTOU below at the same time. That belongs to the sink and must not be
//!   folded in here.
//! - **TOCTOU — narrowed, not closed.** Between the returned path and the
//!   caller's `open`, anyone with write access to the deepest existing
//!   directory can plant a symlink at the not-yet-created tail. Everything
//!   above that directory is proven; the tail is not. Only `O_NOFOLLOW` or
//!   `openat2(RESOLVE_BENEATH)` at the sink closes it.
//! - **Authorization.** This contains you to a root. It has no opinion on
//!   whether you should ever have been handed that root.
//! - **Subprocesses.** `bash`, `rg` and `git` reach any path with no path layer
//!   involved. Every guarantee here is advisory for a caller that can spawn.
//! - **Execution.** Containing a Lua path still executes attacker-authored Lua
//!   that is legitimately inside the root. Containment is necessary, not
//!   sufficient.
//! - **Root swapped mid-walk.** The root is canonicalized once; if the root
//!   itself is replaced afterwards the walk proceeds against a stale canonical
//!   root. Same class as the TOCTOU above.
//! - **Linux-first.** [`std::path::Component::Prefix`] is refused, but NTFS
//!   alternate data streams, 8.3 short names, and trailing dots/spaces are not
//!   modeled. Do not read this as Windows coverage.

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// Why a path was refused.
///
/// Every variant is a refusal. There is deliberately no variant meaning
/// "unchecked" — a control that can be configured into dormancy is the failure
/// mode this module exists to end.
///
/// # `Display` is the wire form; `Debug` is the log form
///
/// The design has each crate write its own `From<Refusal>` into a wire error,
/// which means `Display` is what a remote caller reads. So `Display` carries
/// **no path and no errno**: only which class of refusal occurred. Filesystem
/// layout — where a kiln lives, which of `$HOME/.ssh/id_rsa` and
/// `$HOME/.ssh/id_ed25519` exists — is exactly what an attacker probing a path
/// API is trying to learn, and a refusal that names the resolved path hands it
/// over on every probe.
///
/// What is safe to surface, and why:
///
/// - **The variant's class.** The caller supplied the path; telling it "that
///   escaped" or "that root is misconfigured" reveals nothing it did not
///   already know, and the operator/attacker distinction is the difference
///   between an operator fixing a config and one hunting a phantom intruder.
/// - **Not the path.** Not even the caller's own spelling: on the daemon the
///   "caller" is an agent, and a refusal echoed into a transcript is a channel
///   out. A caller that wants to show the user their own input already has it
///   and should re-attach it itself.
/// - **Not the errno.** `NotFound` versus `PermissionDenied` on a path the
///   caller may not reach is an existence oracle in one bit.
///
/// The detail is not lost, only moved: every variant keeps its `PathBuf` as a
/// public field, so logs render `?refusal` (`Debug`, which carries the paths)
/// or read the fields directly, and the `#[source]` chain still holds the
/// underlying [`std::io::Error`]. Log with `?`, wire with `%`.
#[derive(Debug, thiserror::Error)]
pub enum Refusal {
    /// The root itself does not exist or cannot be canonicalized. This is a
    /// configuration fault, kept distinct from a caller fault so operators are
    /// not sent hunting for an attacker who is not there.
    #[error("the containment root is unavailable")]
    RootUnavailable {
        root: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// No roots were configured. Deny-all, and distinct from "allowed".
    #[error("no containment roots are configured; refusing every path")]
    NoRoots,

    /// The root is relative, so it names a different directory for every value
    /// the process CWD happens to hold. See [`resolve_within`].
    #[error("the containment root is not absolute")]
    RootNotAbsolute { root: PathBuf },

    /// The root is the filesystem root, against which containment is
    /// vacuously true. See [`resolve_within`].
    #[error("the containment root is too broad to bound anything")]
    RootTooBroad { root: PathBuf },

    /// A relative path was offered to [`resolve_within_any`], which cannot
    /// answer which root it belongs to.
    #[error("a relative path cannot be resolved against a set of roots")]
    AmbiguousRelative { user_path: PathBuf },

    /// Lexically illegal before any syscall: `..`, a NUL byte, a Windows
    /// prefix, or an absolute path that is not under the root at all.
    #[error("the path escapes its containment root")]
    Traversal { user_path: PathBuf },

    /// A component is a symlink whose target resolves outside the root.
    #[error("a symlink in the path resolves outside its containment root")]
    SymlinkEscape { at: PathBuf },

    /// A component is a symlink that cannot be resolved — dangling, or a cycle
    /// the kernel stopped with `ELOOP`. Containment is unprovable, so it is
    /// refused rather than guessed at.
    #[error("a symlink in the path cannot be resolved, so containment is unprovable")]
    UnresolvableLink { at: PathBuf },

    /// `symlink_metadata` failed with something other than `NotFound`.
    ///
    /// This variant is the whole point of the module's error handling: an
    /// unexpected errno refuses instead of being read as "does not exist".
    #[error("cannot decide whether the path is contained")]
    Undecidable {
        at: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Resolve `user_path` inside `root`, returning the canonical path the caller
/// must use.
///
/// `root` must be **absolute** and must not be the filesystem root; see
/// [`Refusal::RootNotAbsolute`] and [`Refusal::RootTooBroad`], and the module
/// docs for the argument about which root policy lives here.
///
/// `user_path` may be relative (resolved against `root`) or absolute (narrowed
/// against `root` first); see the module docs for why absolute is accepted and
/// what that means for a caller migrating off a refuse-absolute validator.
///
/// See the module docs for what this does and does not guarantee. The input is
/// never valid to pass to a syscall; use the return value.
pub fn resolve_within(root: &Path, user_path: &Path) -> Result<PathBuf, Refusal> {
    // Refused BEFORE `canonicalize`, which would otherwise resolve a relative
    // root against the process CWD and quietly succeed — binding every
    // containment decision to a directory no operator chose, no caller can
    // see, and any `set_current_dir` anywhere in the process can move. A root
    // must name one fixed place. This is checked on the caller's spelling
    // rather than the canonical form because canonicalization is precisely
    // what would erase the fault.
    if !root.is_absolute() {
        return Err(Refusal::RootNotAbsolute {
            root: root.to_path_buf(),
        });
    }

    // Canonicalized once, then reused for every containment comparison below,
    // so `starts_with` is always canonical-against-canonical. Validators that
    // sometimes return a canonical path and sometimes a raw one are why the
    // callers of those validators disagreed about what they had been handed.
    let root_canon = root
        .canonicalize()
        .map_err(|source| Refusal::RootUnavailable {
            root: root.to_path_buf(),
            source,
        })?;

    // `/` makes this function a no-op that looks like a check: every absolute
    // path strips against it, every canonical symlink target `starts_with` it,
    // so the walk below can only ever return `Ok`. Refused on the CANONICAL
    // root, because `/srv/..` and a symlink to `/` both spell it without
    // looking like it. Only the filesystem root has no parent.
    if root_canon.parent().is_none() {
        return Err(Refusal::RootTooBroad {
            root: root.to_path_buf(),
        });
    }

    // A NUL cannot survive a syscall anyway, but catching it here keeps the
    // verdict a lexical refusal instead of an `Undecidable` errno the caller
    // has to interpret.
    if user_path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(Refusal::Traversal {
            user_path: user_path.to_path_buf(),
        });
    }

    // Narrowing, and only narrowing. `strip_prefix` is purely lexical and
    // proves nothing about containment — the walk below is what proves it.
    // Both spellings of the root are tried because callers legitimately build
    // absolute paths from the uncanonicalized root they were configured with.
    let rel: &Path = if user_path.is_absolute() {
        user_path
            .strip_prefix(&root_canon)
            .or_else(|_| user_path.strip_prefix(root))
            .map_err(|_| Refusal::Traversal {
                user_path: user_path.to_path_buf(),
            })?
    } else {
        user_path
    };

    // Rebuild the path from `Normal` components before any syscall is issued.
    // This is what makes `evil.md/`, `evil.md/.` and `evil.md/./.` all stat as
    // plain `evil.md`: the trailing-separator spellings that make `lstat` fail
    // ENOTDIR — and so skip a naive guard — do not survive to the kernel.
    let mut segments: Vec<&OsStr> = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(segment) => segments.push(segment),
            // Interior `.` is already dropped by `Components`; a leading one is
            // harmless and simply contributes no segment.
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Refusal::Traversal {
                    user_path: user_path.to_path_buf(),
                })
            }
        }
    }

    // Walk one rebuilt component at a time. Zero segments means the caller
    // named the root itself, and the loop simply returns it.
    let mut cur = root_canon.clone();
    for (index, segment) in segments.iter().enumerate() {
        let next = cur.join(segment);
        match std::fs::symlink_metadata(&next) {
            Ok(meta) if meta.file_type().is_symlink() => match next.canonicalize() {
                // Legitimate in-root symlinks keep working, and the walk
                // continues from the resolved target rather than the alias.
                Ok(target) if target.starts_with(&root_canon) => cur = target,
                Ok(_) => return Err(Refusal::SymlinkEscape { at: next }),
                // Dangling or ELOOP. The kernel could not resolve it and this
                // module deliberately owns no resolver to second-guess it with,
                // so containment is unprovable and the path is refused. Note
                // `Path::exists()` is false for a dangling link, which is
                // exactly how an existence-based validator falls through to its
                // lenient branch and hands back the raw symlink.
                Err(_) => return Err(Refusal::UnresolvableLink { at: next }),
            },
            Ok(_) => cur = next,
            // The ONE error that may be read as "does not exist". Everything
            // from here down is yet to be created, and every remaining segment
            // is `Component::Normal`, so appending them to an already-canonical,
            // already-contained prefix cannot escape.
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Ok(segments[index..].iter().fold(cur, |acc, s| acc.join(s)))
            }
            // ENOTDIR, ENAMETOOLONG, EACCES, and any errno this code has never
            // heard of. Fail closed by construction: an unrecognised error is a
            // refusal, never a skip.
            Err(source) => return Err(Refusal::Undecidable { at: next, source }),
        }
    }
    Ok(cur)
}

/// First root that contains `user_path`, which **must be absolute**.
///
/// An empty slice is **deny-all** ([`Refusal::NoRoots`]), never "unchecked".
///
/// # Why relative paths are refused here
///
/// A relative path is not *contained* by a root, it is *interpreted* by one:
/// `notes/a.md` names a different file under each root and is a legal name
/// under all of them. Asking which root contains it is a category error, and
/// the answer the loop would give is always `roots[0]` — a clean relative path
/// cannot fail against the first root, because a component that does not exist
/// simply becomes the not-yet-created tail. That made this function a dormant
/// control in the most dangerous way available: a caller passing three roots
/// got a check against one and no signal that the other two were unreachable.
///
/// So the ambiguity is refused ([`Refusal::AmbiguousRelative`]) instead of
/// resolved by position. A caller holding a relative path already knows which
/// root it belongs to — that is what made it relative — and should name that
/// root by calling [`resolve_within`] directly. Only an absolute path is a
/// question multiple roots can each genuinely answer, and against an absolute
/// path every root is a real containment test.
pub fn resolve_within_any(roots: &[PathBuf], user_path: &Path) -> Result<PathBuf, Refusal> {
    // Deny-all comes first so it stays the headline verdict for a caller that
    // configured nothing, whatever shape of path it then offers.
    if roots.is_empty() {
        return Err(Refusal::NoRoots);
    }
    if !user_path.is_absolute() {
        return Err(Refusal::AmbiguousRelative {
            user_path: user_path.to_path_buf(),
        });
    }

    let mut first_refusal: Option<Refusal> = None;
    for root in roots {
        match resolve_within(root, user_path) {
            Ok(resolved) => return Ok(resolved),
            Err(refusal) => {
                // Roots are ordered by preference, so the primary root's verdict
                // is the useful diagnostic; later roots only ever add noise.
                if first_refusal.is_none() {
                    first_refusal = Some(refusal);
                }
            }
        }
    }
    // Unreachable after the empty-slice check above: a loop that did not
    // return `Ok` recorded a refusal. Kept as a deny rather than an
    // `unwrap`/`unreachable!` so that if the reasoning is ever wrong the
    // function still fails closed instead of panicking a daemon thread.
    Err(first_refusal.unwrap_or(Refusal::NoRoots))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    /// A canonical root plus a sibling directory that is deliberately outside
    /// it, so "escapes the root" is a real place on disk rather than a string.
    struct Fixture {
        _tmp: TempDir,
        root: PathBuf,
        outside: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        // The tempdir's own path can be non-canonical (a symlinked TMPDIR), so
        // canonicalize once here; otherwise every assertion below would be
        // comparing two different spellings of the same directory.
        let base = tmp.path().canonicalize().expect("canonicalize tempdir");
        let root = base.join("kiln");
        let outside = base.join("outside");
        std::fs::create_dir(&root).expect("create root");
        std::fs::create_dir(&outside).expect("create outside");
        std::fs::write(outside.join("secret.txt"), b"secret").expect("write secret");
        Fixture {
            _tmp: tmp,
            root,
            outside,
        }
    }

    // ---- accepting cases -------------------------------------------------

    #[test]
    fn the_root_itself_resolves_to_the_canonical_root() {
        let f = fixture();
        assert_eq!(resolve_within(&f.root, Path::new("")).unwrap(), f.root);
    }

    #[test]
    fn a_bare_dot_resolves_to_the_canonical_root() {
        let f = fixture();
        assert_eq!(resolve_within(&f.root, Path::new(".")).unwrap(), f.root);
    }

    #[test]
    fn an_existing_file_in_the_root_resolves_under_the_root() {
        let f = fixture();
        std::fs::write(f.root.join("note.md"), b"hi").unwrap();
        assert_eq!(
            resolve_within(&f.root, Path::new("note.md")).unwrap(),
            f.root.join("note.md")
        );
    }

    /// Deliberate, and the one place this module is more permissive than the
    /// refuse-absolute validators it replaces — ACP agents send absolute
    /// in-workspace paths, so refusing them outright would break
    /// `acp/client/tools.rs`. See the module docs: `strip_prefix` only
    /// narrows, the walk still proves containment, and a caller whose wire
    /// contract is relative-only must keep its own refusal at its boundary.
    #[test]
    fn an_absolute_path_inside_the_root_is_narrowed_and_accepted() {
        let f = fixture();
        std::fs::write(f.root.join("note.md"), b"hi").unwrap();
        let abs = f.root.join("note.md");
        assert_eq!(resolve_within(&f.root, &abs).unwrap(), abs);
    }

    #[test]
    fn a_not_yet_created_file_in_an_existing_directory_is_accepted() {
        let f = fixture();
        std::fs::create_dir(f.root.join("notes")).unwrap();
        assert_eq!(
            resolve_within(&f.root, Path::new("notes/new.md")).unwrap(),
            f.root.join("notes/new.md")
        );
    }

    #[test]
    fn not_yet_created_nested_directories_are_accepted() {
        let f = fixture();
        assert_eq!(
            resolve_within(&f.root, Path::new("a/b/c/new.md")).unwrap(),
            f.root.join("a/b/c/new.md")
        );
    }

    #[test]
    fn unicode_and_spaces_survive_resolution_unchanged() {
        let f = fixture();
        let rel = "notes/日本語 ファイル — draft.md";
        assert_eq!(
            resolve_within(&f.root, Path::new(rel)).unwrap(),
            f.root.join(rel)
        );
    }

    #[test]
    fn a_symlink_inside_the_root_resolves_to_its_target() {
        let f = fixture();
        std::fs::create_dir(f.root.join("real")).unwrap();
        std::fs::write(f.root.join("real/note.md"), b"hi").unwrap();
        symlink(f.root.join("real"), f.root.join("alias")).unwrap();

        // The returned path is the resolved target, not the spelling the caller
        // supplied — this is why callers must use the return value.
        assert_eq!(
            resolve_within(&f.root, Path::new("alias/note.md")).unwrap(),
            f.root.join("real/note.md")
        );
    }

    #[test]
    fn a_root_reached_through_a_symlink_still_accepts_paths_spelled_through_it() {
        let f = fixture();
        let aliased_root = f.root.parent().unwrap().join("kiln-alias");
        symlink(&f.root, &aliased_root).unwrap();
        std::fs::write(f.root.join("note.md"), b"hi").unwrap();

        let spelled = aliased_root.join("note.md");
        let resolved = resolve_within(&aliased_root, &spelled).unwrap();
        assert_eq!(resolved, f.root.join("note.md"));
        assert_ne!(resolved, spelled, "the caller must use the returned path");
    }

    // ---- lexical refusals ------------------------------------------------

    #[test]
    fn an_absolute_path_outside_the_root_is_refused() {
        let f = fixture();
        let err = resolve_within(&f.root, &f.outside.join("secret.txt")).unwrap_err();
        assert!(matches!(err, Refusal::Traversal { .. }), "got {err:?}");
    }

    #[test]
    fn a_parent_traversal_is_refused() {
        let f = fixture();
        let err = resolve_within(&f.root, Path::new("../outside/secret.txt")).unwrap_err();
        assert!(matches!(err, Refusal::Traversal { .. }), "got {err:?}");
    }

    #[test]
    fn a_parent_traversal_buried_mid_path_is_refused() {
        let f = fixture();
        std::fs::create_dir(f.root.join("sub")).unwrap();
        let err = resolve_within(&f.root, Path::new("sub/../../outside/secret.txt")).unwrap_err();
        assert!(matches!(err, Refusal::Traversal { .. }), "got {err:?}");
    }

    #[test]
    fn an_absolute_path_that_climbs_back_out_of_the_root_is_refused() {
        let f = fixture();
        // Lexically prefixed by the root, so a naive `starts_with` check passes.
        let err = resolve_within(&f.root, &f.root.join("../outside/secret.txt")).unwrap_err();
        assert!(matches!(err, Refusal::Traversal { .. }), "got {err:?}");
    }

    #[test]
    fn the_roots_own_parent_is_refused() {
        let f = fixture();
        let parent = f.root.parent().unwrap().to_path_buf();
        let err = resolve_within(&f.root, &parent).unwrap_err();
        assert!(matches!(err, Refusal::Traversal { .. }), "got {err:?}");
    }

    #[test]
    fn a_nul_byte_in_the_path_is_refused() {
        let f = fixture();
        let err = resolve_within(&f.root, Path::new("note\0.md")).unwrap_err();
        assert!(matches!(err, Refusal::Traversal { .. }), "got {err:?}");
    }

    // ---- symlink refusals, including the trailing-separator spellings ----

    #[test]
    fn a_symlink_pointing_outside_the_root_is_refused() {
        let f = fixture();
        symlink(f.outside.join("secret.txt"), f.root.join("evil.md")).unwrap();
        let err = resolve_within(&f.root, Path::new("evil.md")).unwrap_err();
        assert!(matches!(err, Refusal::SymlinkEscape { .. }), "got {err:?}");
    }

    #[test]
    fn a_trailing_slash_does_not_defeat_the_symlink_check() {
        let f = fixture();
        symlink(f.outside.join("secret.txt"), f.root.join("evil.md")).unwrap();
        // Handed to the kernel raw, `lstat("evil.md/")` fails ENOTDIR and a
        // guard that treats errors as "absent" waves it through.
        let err = resolve_within(&f.root, Path::new("evil.md/")).unwrap_err();
        assert!(matches!(err, Refusal::SymlinkEscape { .. }), "got {err:?}");
    }

    #[test]
    fn a_trailing_dot_does_not_defeat_the_symlink_check() {
        let f = fixture();
        symlink(f.outside.join("secret.txt"), f.root.join("evil.md")).unwrap();
        let err = resolve_within(&f.root, Path::new("evil.md/.")).unwrap_err();
        assert!(matches!(err, Refusal::SymlinkEscape { .. }), "got {err:?}");
    }

    #[test]
    fn repeated_trailing_dots_do_not_defeat_the_symlink_check() {
        let f = fixture();
        symlink(f.outside.join("secret.txt"), f.root.join("evil.md")).unwrap();
        let err = resolve_within(&f.root, Path::new("evil.md/./.")).unwrap_err();
        assert!(matches!(err, Refusal::SymlinkEscape { .. }), "got {err:?}");
    }

    #[test]
    fn a_directory_symlink_pointing_outside_the_root_is_refused_before_its_tail() {
        let f = fixture();
        symlink(&f.outside, f.root.join("escape")).unwrap();
        // The tail does not exist yet; the escape must be caught at the link,
        // not deferred to a "to be created" verdict.
        let err = resolve_within(&f.root, Path::new("escape/planted.md")).unwrap_err();
        assert!(matches!(err, Refusal::SymlinkEscape { .. }), "got {err:?}");
    }

    #[test]
    fn a_dangling_symlink_is_refused_as_unresolvable() {
        let f = fixture();
        symlink(f.outside.join("does-not-exist"), f.root.join("dangling.md")).unwrap();
        // `Path::exists()` is false here, which is exactly how a validator that
        // asks "does it exist?" falls through to its lenient branch.
        assert!(!f.root.join("dangling.md").exists());
        let err = resolve_within(&f.root, Path::new("dangling.md")).unwrap_err();
        assert!(
            matches!(err, Refusal::UnresolvableLink { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn a_symlink_cycle_is_refused_as_unresolvable() {
        let f = fixture();
        symlink(f.root.join("b"), f.root.join("a")).unwrap();
        symlink(f.root.join("a"), f.root.join("b")).unwrap();
        let err = resolve_within(&f.root, Path::new("a")).unwrap_err();
        assert!(
            matches!(err, Refusal::UnresolvableLink { .. }),
            "got {err:?}"
        );
    }

    // ---- errno cases that must not collapse into "does not exist" --------

    #[test]
    fn an_over_long_component_is_refused_as_undecidable() {
        let f = fixture();
        // ENAMETOOLONG surfaces as ErrorKind::InvalidFilename, NOT NotFound.
        // Reading every stat error as "absent" waves this through unchecked.
        let long = "a".repeat(300);
        let err = resolve_within(&f.root, Path::new(&long)).unwrap_err();
        assert!(matches!(err, Refusal::Undecidable { .. }), "got {err:?}");
    }

    #[test]
    fn a_regular_file_used_as_a_directory_is_refused_as_undecidable() {
        let f = fixture();
        std::fs::write(f.root.join("note.md"), b"hi").unwrap();
        // ENOTDIR. After lexical normalization this is a genuine `note.md/child`
        // rather than a trailing-separator spelling trick.
        let err = resolve_within(&f.root, Path::new("note.md/child")).unwrap_err();
        assert!(matches!(err, Refusal::Undecidable { .. }), "got {err:?}");
    }

    #[test]
    fn an_unreadable_directory_is_refused_as_undecidable() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let locked = f.root.join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Root ignores the mode bits, so assert whichever precondition actually
        // holds. Branching on the observed reality keeps this from silently
        // asserting nothing when the suite runs privileged.
        let privileged = std::fs::read_dir(&locked).is_ok();
        let result = resolve_within(&f.root, Path::new("locked/note.md"));

        // Restore before asserting so a failure cannot break TempDir cleanup.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        if privileged {
            assert_eq!(result.unwrap(), f.root.join("locked/note.md"));
        } else {
            let err = result.unwrap_err();
            assert!(matches!(err, Refusal::Undecidable { .. }), "got {err:?}");
        }
    }

    #[test]
    fn a_missing_root_is_refused_as_root_unavailable() {
        let f = fixture();
        let err = resolve_within(&f.root.join("nope"), Path::new("note.md")).unwrap_err();
        assert!(
            matches!(err, Refusal::RootUnavailable { .. }),
            "got {err:?}"
        );
    }

    // ---- roots that cannot bound anything --------------------------------

    /// A relative root is resolved against whatever the process CWD happens
    /// to be at the moment of the call — for the daemon, a directory no
    /// operator chose and no caller can see. `.` is used because it
    /// canonicalizes successfully from any CWD, so the refusal under test is
    /// the relative-ness itself and not an incidental `RootUnavailable`.
    #[test]
    fn a_relative_root_is_refused_rather_than_bound_to_the_process_cwd() {
        let err = resolve_within(Path::new("."), Path::new("note.md")).unwrap_err();
        assert!(
            matches!(err, Refusal::RootNotAbsolute { .. }),
            "got {err:?}"
        );

        let err =
            resolve_within(Path::new("some/relative/root"), Path::new("note.md")).unwrap_err();
        assert!(
            matches!(err, Refusal::RootNotAbsolute { .. }),
            "got {err:?}"
        );
    }

    /// `/` is the total bypass: every absolute path strips against it and
    /// every symlink target `starts_with("/")`, so the function returns `Ok`
    /// for the entire filesystem while looking like it performed a check.
    #[test]
    fn the_filesystem_root_is_refused_as_a_containment_root() {
        let err = resolve_within(Path::new("/"), Path::new("etc/passwd")).unwrap_err();
        assert!(matches!(err, Refusal::RootTooBroad { .. }), "got {err:?}");

        let err = resolve_within(Path::new("/"), Path::new("/etc/passwd")).unwrap_err();
        assert!(matches!(err, Refusal::RootTooBroad { .. }), "got {err:?}");
    }

    /// Spelled the long way round, `/x/..` canonicalizes to `/` — so the
    /// check has to be on the canonical root, not on the caller's spelling.
    #[test]
    fn a_root_that_canonicalizes_to_the_filesystem_root_is_refused() {
        let f = fixture();
        let up_to_slash = {
            // Climb from the (absolute) fixture root to `/` without writing a
            // literal `/…/..` that assumes any particular tmpdir depth.
            let mut p = f.root.clone();
            while p.parent().is_some() {
                p = p.join("..");
                let canon = p.canonicalize().unwrap();
                if canon.parent().is_none() {
                    break;
                }
            }
            p
        };
        assert_eq!(up_to_slash.canonicalize().unwrap(), Path::new("/"));

        let err = resolve_within(&up_to_slash, Path::new("etc/passwd")).unwrap_err();
        assert!(matches!(err, Refusal::RootTooBroad { .. }), "got {err:?}");
    }

    // ---- what a refusal is allowed to say --------------------------------

    /// `Display` is what crates put on the wire through their own
    /// `From<Refusal>`, so it must not carry filesystem layout. The paths
    /// stay in `Debug` (and in the `#[source]` chain), which is what logs
    /// should render.
    ///
    /// Adding a `Refusal` variant means adding it to `path_bearing` below —
    /// `NoRoots` is the only variant that legitimately carries no path.
    #[test]
    fn refusal_display_never_leaks_a_path_but_debug_still_carries_it() {
        const SENTINEL: &str = "sentinel-secret-dir";
        let leaky = PathBuf::from(format!("/home/someone/{SENTINEL}/id_rsa"));
        let path_bearing = vec![
            Refusal::RootUnavailable {
                root: leaky.clone(),
                source: std::io::Error::from(ErrorKind::NotFound),
            },
            Refusal::RootNotAbsolute {
                root: leaky.clone(),
            },
            Refusal::RootTooBroad {
                root: leaky.clone(),
            },
            Refusal::AmbiguousRelative {
                user_path: leaky.clone(),
            },
            Refusal::Traversal {
                user_path: leaky.clone(),
            },
            Refusal::SymlinkEscape { at: leaky.clone() },
            Refusal::UnresolvableLink { at: leaky.clone() },
            Refusal::Undecidable {
                at: leaky.clone(),
                source: std::io::Error::from(ErrorKind::PermissionDenied),
            },
        ];

        for refusal in &path_bearing {
            let shown = refusal.to_string();
            assert!(
                !shown.contains(SENTINEL),
                "Display leaked a path: {shown} (from {refusal:?})"
            );
            // Guards the assertion above against passing because the sentinel
            // never made it into the value in the first place — a variant that
            // dropped its path would otherwise "pass" for the wrong reason.
            assert!(
                format!("{refusal:?}").contains(SENTINEL),
                "the fixture path never reached this variant, so the Display \
                 assertion proved nothing: {refusal:?}"
            );
        }

        // Carries no path by construction, so only the wire form is asserted.
        assert!(!Refusal::NoRoots.to_string().contains(SENTINEL));
    }

    /// The errno is withheld too: `NotFound` versus `PermissionDenied` on a
    /// path the caller may not reach is an existence oracle in one bit. It
    /// stays reachable through the `#[source]` chain for logs.
    #[test]
    fn refusal_display_never_leaks_the_errno() {
        use std::error::Error;

        let refusal = Refusal::Undecidable {
            at: PathBuf::from("/home/someone/.ssh/id_rsa"),
            source: std::io::Error::from(ErrorKind::PermissionDenied),
        };
        let denied = std::io::Error::from(ErrorKind::PermissionDenied).to_string();
        let missing = std::io::Error::from(ErrorKind::NotFound).to_string();

        let shown = refusal.to_string();
        assert!(
            !shown.contains(&denied),
            "Display leaked the errno: {shown}"
        );
        assert!(
            !shown.contains(&missing),
            "Display leaked the errno: {shown}"
        );
        assert_eq!(
            refusal.source().map(ToString::to_string),
            Some(denied),
            "the errno must remain available to logs through the source chain"
        );
    }

    // ---- resolve_within_any ----------------------------------------------

    #[test]
    fn no_roots_denies_every_path() {
        let f = fixture();
        // Deny-all, never "unchecked": the dormant-control failure mode.
        let err = resolve_within_any(&[], &f.root.join("note.md")).unwrap_err();
        assert!(matches!(err, Refusal::NoRoots), "got {err:?}");

        let err = resolve_within_any(&[], Path::new("note.md")).unwrap_err();
        assert!(matches!(err, Refusal::NoRoots), "got {err:?}");
    }

    #[test]
    fn an_absolute_path_under_a_later_root_is_accepted() {
        let f = fixture();
        std::fs::write(f.outside.join("note.md"), b"hi").unwrap();
        let roots = vec![f.root.clone(), f.outside.clone()];
        assert_eq!(
            resolve_within_any(&roots, &f.outside.join("note.md")).unwrap(),
            f.outside.join("note.md")
        );
    }

    /// The dormant-control case. A lexically clean RELATIVE path is accepted
    /// by `roots[0]` unconditionally — a not-yet-existing component simply
    /// becomes the unverified tail — so every later root is unreachable and a
    /// caller passing three roots gets a check against one.
    #[test]
    fn a_relative_path_is_not_silently_bound_to_the_first_root() {
        let f = fixture();
        // The file exists only under the SECOND root. Under the old
        // semantics this still returned `Ok(<first root>/note.md)`: a path
        // that does not exist, from a root the caller never meant.
        std::fs::write(f.outside.join("note.md"), b"hi").unwrap();
        let roots = vec![f.root.clone(), f.outside.clone()];

        let err = resolve_within_any(&roots, Path::new("note.md")).unwrap_err();
        assert!(
            matches!(err, Refusal::AmbiguousRelative { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn a_path_under_none_of_the_roots_is_refused() {
        let f = fixture();
        let elsewhere = f.root.parent().unwrap().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let roots = vec![f.root.clone(), f.outside.clone()];
        let err = resolve_within_any(&roots, &elsewhere.join("note.md")).unwrap_err();
        assert!(matches!(err, Refusal::Traversal { .. }), "got {err:?}");
    }

    #[test]
    fn an_escaping_symlink_is_refused_by_every_root() {
        let f = fixture();
        symlink(
            f.root.parent().unwrap().join("secret"),
            f.root.join("evil.md"),
        )
        .unwrap();
        std::fs::write(f.root.parent().unwrap().join("secret"), b"s").unwrap();
        let roots = vec![f.root.clone(), f.outside.clone()];
        // Absolute, because that is the only shape this function accepts —
        // and the shape that makes both roots a real check rather than one.
        let err = resolve_within_any(&roots, &f.root.join("evil.md")).unwrap_err();
        assert!(matches!(err, Refusal::SymlinkEscape { .. }), "got {err:?}");
    }

    /// Every root is consulted for an absolute path, and the refusal reported
    /// is the first (preferred) root's — not the last root to be tried.
    #[test]
    fn the_reported_refusal_comes_from_the_first_root() {
        let f = fixture();
        let missing_root = f.root.parent().unwrap().join("never-created");
        let roots = vec![missing_root, f.root.clone()];

        // Under `f.outside`, so no root contains it: the loop runs to the end
        // and must still report the primary root's verdict.
        let err = resolve_within_any(&roots, &f.outside.join("note.md")).unwrap_err();
        assert!(
            matches!(err, Refusal::RootUnavailable { .. }),
            "got {err:?}"
        );

        // The same broken first root must not stop a later root accepting.
        std::fs::write(f.root.join("note.md"), b"hi").unwrap();
        assert_eq!(
            resolve_within_any(&roots, &f.root.join("note.md")).unwrap(),
            f.root.join("note.md")
        );
    }
}
