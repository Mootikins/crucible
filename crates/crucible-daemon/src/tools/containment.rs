//! The set of filesystem roots a tool set may reach.
//!
//! This is an **allowlist, and it is default-deny**: a path is refused unless
//! some root admits it. That is the inversion, and it is the whole point.
//!
//! The predecessor was default-allow-minus-a-denylist — `denied_roots` was the
//! primary mechanism and `allowed_roots` was a wide net around it. Every escape
//! found in two adversarial passes took one of two forms against that model:
//! **out-ranking** a denial (attach a kiln *deeper* than the denied root, or
//! inflate a path's depth by traversing a directory that does not exist yet) or
//! **side-stepping** it (an empty root matching every path at depth 0). Neither
//! is expressible against an allowlist, because there is nothing to out-rank —
//! a root that is not admitted is simply not admitted.
//!
//! Denials survive, but demoted to what Zed and Codex use them for: **carve-outs
//! inside an allowed root**, for the case where a root the session legitimately
//! needs happens to enclose a subtree it must not reach (a kiln at the data
//! root encloses the sessions root; a workspace that used to be a kiln still
//! holds `.crucible/sessions`). Two rules keep that from re-opening the door:
//!
//! 1. **A scope root that lands inside a denial is dropped**, not ranked
//!    against it ([`RootSet::scoped`]). This is where out-ranking died. The
//!    only roots that may sit inside a denial are the ones passed to
//!    [`RootSet::carve_out`], which the daemon derives — never the caller.
//! 2. **Roots are normalized once, at construction**, through
//!    [`ResolvedPath`]. A root spelled `{data}/not-yet/../sessions/{victim}`
//!    is *stored* as `{sessions_root}/{victim}` and therefore dropped by (1),
//!    rather than dodging the comparison because its `..` survived into it.
//!
//! Deepest-match-wins still decides between an allow and a deny, exactly as
//! Codex resolves overlapping policy (specificity first, ties to the more
//! restrictive mode) — but after (1) it is reachable only for a genuine nested
//! exception, which is a session reaching its own storage directory inside the
//! sessions root that hides every other session's.
//!
//! ## Reads and writes are not the same question
//!
//! [`Access`] splits them, because the threat models differ and we had the
//! priority inverted. A *write* to a denied or [protected](RootSet::protect)
//! root is refused outright, ahead of the ranking and with the carve-outs
//! dropped — so the sessions root is write-denied full stop, including to the
//! session whose own storage directory lives inside it. A *read* keeps the
//! carve-out and the ranking.
//!
//! ## What this layer can and cannot claim
//!
//! The write side is a real structural property of this process: a
//! [`Containment::WriteProtected`] verdict cannot be out-ranked, and no
//! configuration reaches these roots — the permission chain above decides
//! *whether a tool call runs*, and this decides *what a path is*, so the
//! widest possible allow lands on a refusal anyway.
//!
//! The read side is weaker and should be described the way Hermes describes
//! theirs rather than the way we described ours. It holds for the tool
//! families that route through [`super::fs_scope::FsScope`], which is all of
//! them today; it does not hold against `bash`, which reaches the filesystem
//! through a shell this process does not mediate, and it does not hold against
//! in-process code that calls `std::fs` on a path it built itself. Until the
//! design's §4 Landlock backstop lands, "a session cannot read another
//! session's transcript" is defense in depth, not a guarantee.

use super::path_resolution::ResolvedPath;
use std::path::{Path, PathBuf};

/// The verdict on a path, with symlink escape as its own outcome.
///
/// Zed models this as `ResolvedProjectPath::SymlinkEscape { canonical_target }`
/// and routes it to a decision rather than folding it into a generic refusal.
/// We follow that: "inside by name, outside once resolved" is a different fact
/// about the request than "outside by name", it is the fact a reviewer most
/// needs to see, and a distinct variant is what makes it assertable in a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Containment {
    /// Both forms clear containment. Carries the canonical path.
    Permitted(PathBuf),
    /// Outside the allowed roots however the path is read.
    Outside,
    /// The path names a location inside containment, but resolving it through
    /// the filesystem lands somewhere outside.
    SymlinkEscape { canonical_target: PathBuf },
    /// A WRITE to a root that is write-denied outright — a transcript
    /// directory, or a tree the daemon later loads and executes. Carries the
    /// root that refused, so the message can name it.
    ///
    /// Distinct from [`Self::Outside`] because it is not a ranking result: it
    /// is checked before the allow/deny ranking and nothing below can win
    /// against it. See [`Access::Write`].
    WriteProtected { root: PathBuf },
}

/// What the caller intends to do with the path.
///
/// The distinction exists because the industry has converged on the opposite
/// of the assumption we started from. Anthropic blocks *writes* to
/// `~/.claude/projects/**.jsonl` as defense in depth against prompt-injection
/// tampering and states plainly that reading a transcript is not blocked: a
/// transcript is replayed into a future context, so editing one is the attack.
/// The same asymmetry holds for everything the daemon executes — reading a
/// plugin's source is ordinary work, writing one is code execution.
///
/// Reads therefore keep the carve-out exceptions; writes do not get them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Access {
    Read,
    Write,
}

/// Which of a path's two resolved forms a comparison is being made on.
///
/// Comparisons are made on ONE form at a time, applied to the candidate and to
/// the roots alike. Mixing them would make the depth ranking incomparable — a
/// root's lexical spelling can be arbitrarily deeper than where it actually
/// lands — and a distorted ranking is a denial that loses.
#[derive(Debug, Clone, Copy)]
enum Form {
    Lexical,
    Canonical,
}

impl Form {
    fn of(self, path: &ResolvedPath) -> &Path {
        match self {
            Self::Lexical => path.lexical(),
            Self::Canonical => path.canonical(),
        }
    }
}

/// An allowlist of filesystem roots, with carve-outs subtracted from it.
///
/// Public only so that the daemon's public constructors can carry one from the
/// session that owns it to the tool set that answers to it — every way of
/// BUILDING a non-empty set stays `pub(crate)`, because a root set assembled
/// outside this crate would be containment chosen by the thing being contained.
#[derive(Debug, Clone)]
pub enum RootSet {
    /// No boundary: every path on the host is reachable.
    ///
    /// This is not a containment policy, it is the absence of one, and it
    /// exists only for the construction sites that predate containment — the
    /// daemon-global tool set behind the Lua bridge and the `tools.execute`
    /// RPC, which answer to the permission gate instead. **No session
    /// dispatcher may be built with it**; `session_containment` always returns
    /// a [`Self::Rooted`], and `agent_manager` has no branch that skips it.
    Ambient,
    /// Default-deny. A path is permitted only if some root in `allowed` admits
    /// it and no root in `denied` out-ranks that admission.
    ///
    /// An EMPTY `allowed` denies everything. That is the fail-closed reading of
    /// a session with no kiln and no workspace, and it is the rule the kiln
    /// design states as "an empty kiln set degrades capabilities, never
    /// containment".
    Rooted(Roots),
}

/// The roots of a [`RootSet::Rooted`].
///
/// A struct with private fields rather than an inline variant payload, so that
/// `RootSet` can be public — a caller outside this crate may carry one from the
/// session that owns it to the tool set it contains, and may not look inside or
/// assemble one.
#[derive(Debug, Clone)]
pub struct Roots {
    allowed: Vec<ResolvedPath>,
    denied: Vec<ResolvedPath>,
    /// Roots re-admitted INSIDE a denial, for reads only. Kept apart from
    /// `allowed` rather than merged into it so that [`Access::Write`] can drop
    /// them wholesale — a set a write can never see is stronger than a rule a
    /// future edit has to remember.
    carved: Vec<ResolvedPath>,
    /// Write-denied, read-allowed: the trees the daemon loads and executes.
    /// Not ranked against anything — see [`Containment::WriteProtected`].
    protected: Vec<ResolvedPath>,
}

impl RootSet {
    /// The allowlist `scope`, minus the subtrees `denied` carves out of it.
    ///
    /// Roots are normalized here, once, rather than at each call: a root is a
    /// *policy* input and must mean the same thing for the whole life of the
    /// tool set. Any scope root that lands inside a denial is dropped — see the
    /// module docs for why that, and not depth ranking, is what closes the
    /// out-ranking class.
    pub(crate) fn scoped(
        scope: impl IntoIterator<Item = PathBuf>,
        denied: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        let denied = resolve_roots(denied);
        let allowed = resolve_roots(scope)
            .into_iter()
            .filter(|root| !lands_inside_a_denial(root, &denied))
            .collect();
        Self::Rooted(Roots {
            allowed,
            denied,
            carved: Vec::new(),
            protected: Vec::new(),
        })
    }

    /// Mark roots write-denied without denying reads of them.
    ///
    /// The design's §6 set that is not expressible as a directory NAME: the
    /// runtime tree the daemon loads plugins from, and the personal config
    /// directory holding agent cards and skills. A denial would be wrong — an
    /// agent asked about a plugin should be able to read it — and an allow
    /// rule must not be able to buy the write, so this is checked ahead of the
    /// ranking rather than inside it.
    #[must_use]
    pub(crate) fn protect(self, roots: impl IntoIterator<Item = PathBuf>) -> Self {
        match self {
            Self::Ambient => Self::Ambient,
            Self::Rooted(mut r) => {
                r.protected.extend(resolve_roots(roots));
                Self::Rooted(r)
            }
        }
    }

    /// Re-admit roots that sit INSIDE a denial.
    ///
    /// The one legitimate nested exception, and the reason deepest-match-wins
    /// is still needed: a session's own storage directory lives inside the
    /// sessions root that hides every other session's. Applied after
    /// [`Self::scoped`]'s filtering, and never to caller-supplied paths — the
    /// daemon derives these from the session id it minted.
    pub(crate) fn carve_out(self, roots: impl IntoIterator<Item = PathBuf>) -> Self {
        match self {
            Self::Ambient => Self::Ambient,
            Self::Rooted(mut r) => {
                r.carved.extend(resolve_roots(roots));
                Self::Rooted(r)
            }
        }
    }

    /// Whether this set imposes no boundary at all.
    pub(crate) fn is_ambient(&self) -> bool {
        matches!(self, Self::Ambient)
    }

    /// Judge `path` against the set, on both of its resolved forms.
    ///
    /// Validating the normalized source AND again after resolving through the
    /// deepest existing ancestor is OpenClaw's GHSA-575v-8hfq-m3mc fix, and it
    /// is required in both directions: the lexical form cannot see a symlink,
    /// and the canonical form of a path whose parents do not exist is a
    /// reconstruction rather than an observation. Requiring both to clear means
    /// the weaker of the two decides, which is the fail-closed reading.
    /// Takes an already-resolved path rather than resolving one: resolution
    /// walks the filesystem, and a caller judging one path against two root
    /// sets — which is what [`super::fs_scope::FsScope`] does for the anchor
    /// boundary and the session's allowlist — must resolve once and ask twice.
    /// Resolving per set would also let the two verdicts be about different
    /// files if a link moved in between.
    pub(crate) fn judge_resolved(&self, resolved: &ResolvedPath, access: Access) -> Containment {
        let Self::Rooted(Roots {
            allowed,
            denied,
            carved,
            protected,
        }) = self
        else {
            return Containment::Permitted(resolved.canonical().to_path_buf());
        };

        // Write protection runs BEFORE the ranking and is not part of it.
        // Ranking is how a nested exception legitimately wins, and a write
        // must have no such move available: the sessions root is write-denied
        // "full stop", including to the session whose own storage lives in it.
        if access == Access::Write {
            if let Some(root) = covers_any(denied.iter().chain(protected), resolved) {
                return Containment::WriteProtected { root };
            }
        }

        // Reads see the carve-outs; writes do not, so a read exception cannot
        // be mistaken for a write one by a later edit. Passed as a second
        // slice rather than concatenated: this runs once per walked entry in
        // `FsScope::walk_files`, and a per-entry allocation over a kiln-sized
        // tree is not free.
        let carved: &[ResolvedPath] = match access {
            Access::Read => carved,
            Access::Write => &[],
        };

        // A transcript directory is a SHAPE, not a list of instances, and it
        // sits outside the ranking for the same reason a denial that can be
        // out-ranked is not a denial. See [`enters_transcript_dir`].
        //
        // The read carve-out is the single thing that precedes it, because the
        // REAL sessions root has this very shape: `data_home` is `~/.crucible`,
        // so a session's own storage directory is `~/.crucible/sessions/{id}`
        // and the rule would otherwise deny a session the spilled tool output
        // the carve-out exists to hand back. Checked against `carved` after it
        // has been emptied for writes, so a write can never reach this branch
        // and the exception cannot widen into one.
        if covers_any(carved.iter(), resolved).is_none()
            && [resolved.lexical(), resolved.canonical()]
                .into_iter()
                .any(enters_transcript_dir)
        {
            return Containment::Outside;
        }

        let by_name = form_permits(allowed, carved, denied, resolved.lexical(), Form::Lexical);
        let when_resolved = form_permits(
            allowed,
            carved,
            denied,
            resolved.canonical(),
            Form::Canonical,
        );

        match (by_name, when_resolved) {
            (true, true) => Containment::Permitted(resolved.canonical().to_path_buf()),
            // Inside by name, outside once resolved — something on the path is
            // a link out of containment. Reported as its own outcome so the
            // refusal can say where it lands.
            (true, false) => Containment::SymlinkEscape {
                canonical_target: resolved.canonical().to_path_buf(),
            },
            _ => Containment::Outside,
        }
    }
}

/// Whether `path` passes through a legacy in-kiln transcript directory —
/// `.crucible/sessions`, at ANY depth.
///
/// This was a per-scope-root literal: `session_containment` denied
/// `<root>/.crucible/sessions` for each root the session was given. That is the
/// confused-deputy shape the rest of this module is about — an item inheriting
/// its container's blanket answer. A project filed BENEATH a kiln or workspace
/// has its own `.crucible/sessions`, is not itself a scope root, and so was
/// admitted by the enclosing root's permit; so was any nested kiln, and so was
/// a sibling checkout vendored into one.
///
/// `PROTECTED_DIRS` already matches `.crucible` structurally, but only for
/// writes. This is the read half, and reads are what the transcript threat
/// model is about in the other direction: a transcript names every path,
/// command and secret a previous session touched.
///
/// Both components, adjacent: `.crucible/` alone holds `notes/` and `index/`
/// that ordinary work reads, and denying the whole control directory here would
/// duplicate `FsScope::enters_control_dir`, which is scoped to kiln-anchored
/// tools on purpose.
///
/// The shape is not only the LEGACY layout's. `data_home` is `~/.crucible`, so
/// the flat sessions root is `~/.crucible/sessions` and matches this component
/// for component — which is why [`RootSet::judge_resolved`] lets the read
/// carve-out answer first. Everything else under either spelling is denied,
/// including the rest of the flat root, which `denied` also names outright.
fn enters_transcript_dir(path: &Path) -> bool {
    let mut components = path.components();
    while let Some(component) = components.next() {
        if component.as_os_str() == ".crucible"
            && components
                .clone()
                .next()
                .is_some_and(|next| next.as_os_str() == "sessions")
        {
            return true;
        }
    }
    false
}

/// Normalize a list of roots, dropping the empty path.
///
/// `""` is not a narrow root, it is a broken one: `ResolvedPath::resolve("")`
/// anchors it at the daemon's working directory, so keeping it would silently
/// grant whatever directory the daemon happens to have been started in. (Before
/// resolution it was worse — `Path::starts_with("")` is true of every path at
/// depth 0, which is containment off rather than merely wrong.) A session with
/// no kiln and no workspace must come out of this with FEWER roots, not a
/// surprise one.
fn resolve_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<ResolvedPath> {
    roots
        .into_iter()
        .filter(|root| !root.as_os_str().is_empty())
        .map(|root| ResolvedPath::resolve(&root))
        .collect()
}

/// Whether a scope root lands inside any denial — the test [`RootSet::scoped`]
/// drops on.
///
/// Every form of the root is compared against every form of every denial, and
/// the cross-product is deliberate here where the depth ranking in
/// [`form_permits`] refuses it. The two are answering different questions. A
/// *drop* asks "is there any reading under which this root reaches into the
/// denial", and both readings are reachable: a root NAMED inside the denied
/// sessions root turns it into an oracle for which session ids exist even when
/// it resolves elsewhere, and a root that merely RESOLVES into it reads the
/// transcripts under an innocent name. A *ranking* asks "which of two enclosing
/// roots is more specific", and mixing forms there would compare depths that
/// are not commensurable — a root's lexical spelling can be arbitrarily deeper
/// than where it lands, and a distorted ranking is a denial that loses.
fn lands_inside_a_denial(root: &ResolvedPath, denied: &[ResolvedPath]) -> bool {
    let forms = [root.lexical(), root.canonical()];
    denied.iter().any(|denial| {
        let denial_forms = [denial.lexical(), denial.canonical()];
        forms
            .iter()
            .any(|form| denial_forms.iter().any(|denial| form.starts_with(denial)))
    })
}

/// The first root in `roots` that encloses `candidate` under ANY reading of
/// either — the unranked, fail-closed comparison a write denial needs.
///
/// The cross-product is the same one [`lands_inside_a_denial`] uses and for
/// the same reason: a *denial* asks "is there any reading under which this
/// reaches in", where a *ranking* asks which of two enclosing roots is more
/// specific and must not mix incommensurable depths. A path that merely
/// RESOLVES into a protected tree writes the file the daemon will execute; a
/// path that merely NAMES one creates the very symlink the corollary exists
/// for.
fn covers_any<'a>(
    roots: impl Iterator<Item = &'a ResolvedPath>,
    candidate: &ResolvedPath,
) -> Option<PathBuf> {
    let forms = [candidate.lexical(), candidate.canonical()];
    roots
        .filter(|root| {
            let root_forms = [root.lexical(), root.canonical()];
            forms
                .iter()
                .any(|form| root_forms.iter().any(|root| form.starts_with(root)))
        })
        .map(|root| root.canonical().to_path_buf())
        .next()
}

/// The depth of the deepest root in `roots` that encloses `candidate`, on one
/// form.
fn deepest<'a>(
    roots: impl Iterator<Item = &'a ResolvedPath>,
    candidate: &Path,
    form: Form,
) -> Option<usize> {
    roots
        .map(|root| form.of(root))
        .filter(|root| candidate.starts_with(root))
        .map(|root| root.components().count())
        .max()
}

/// One form's verdict: admitted by some allowed root (or by a carve-out, when
/// the access has them), and not out-ranked by a deeper denial.
fn form_permits(
    allowed: &[ResolvedPath],
    carved: &[ResolvedPath],
    denied: &[ResolvedPath],
    candidate: &Path,
    form: Form,
) -> bool {
    match (
        deepest(allowed.iter().chain(carved), candidate, form),
        deepest(denied.iter(), candidate, form),
    ) {
        (Some(allowed), Some(denied)) => allowed > denied,
        (Some(_), None) => true,
        // Default-deny: no root admits it.
        (None, _) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    /// Resolve-then-judge, which is what [`super::super::fs_scope::FsScope`]
    /// does for callers. Spelled out here so the tests below read as
    /// path-in/verdict-out.
    fn judge(roots: &RootSet, path: &Path) -> Containment {
        roots.judge_resolved(&ResolvedPath::resolve(path), Access::Read)
    }

    fn permits(roots: &RootSet, path: &Path) -> bool {
        matches!(judge(roots, path), Containment::Permitted(_))
    }

    fn writable(roots: &RootSet, path: &Path) -> bool {
        matches!(
            roots.judge_resolved(&ResolvedPath::resolve(path), Access::Write),
            Containment::Permitted(_)
        )
    }

    /// The base case of an allowlist, and the one a denylist gets backwards:
    /// with nothing admitted, nothing is reachable. A session with no kiln and
    /// no workspace is a legitimate shape, and it must come out of this with
    /// less reach — never with unlimited reach.
    #[test]
    fn an_allowlist_with_no_roots_denies_every_path() {
        let roots = RootSet::scoped(vec![], vec![]);
        let dir = tmp();

        assert_eq!(
            judge(&roots, Path::new("/etc/shadow")),
            Containment::Outside
        );
        assert_eq!(judge(&roots, Path::new("/")), Containment::Outside);
        assert_eq!(judge(&roots, dir.path()), Containment::Outside);
    }

    /// The out-ranking class, at its source.
    ///
    /// Deepest-match-wins is a sound way to resolve a *nested exception* and a
    /// catastrophic way to resolve a *conflict*: a root inside a denial is
    /// deeper than it by construction, so under ranking alone the denial always
    /// loses to it. Attaching another session's storage directory as a kiln was
    /// exactly that move. The root is dropped instead, so the denial has
    /// nothing to lose to.
    #[test]
    fn a_root_that_lands_inside_a_denial_is_dropped_instead_of_out_ranking_it() {
        let dir = tmp();
        let sessions_root = dir.path().join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();

        let roots = RootSet::scoped(vec![victim.clone()], vec![sessions_root.clone()]);

        assert_eq!(
            judge(&roots, &victim.join("session.jsonl")),
            Containment::Outside,
            "a scope root inside the sessions root must not re-open it"
        );
    }

    /// The same move with the depth inflated by a directory that does not exist
    /// yet. Normalizing roots at construction is what makes this the previous
    /// case rather than a new one — the root is *stored* as the sessions-root
    /// path it names.
    #[test]
    fn a_root_whose_traversal_lands_inside_a_denial_is_dropped() {
        let dir = tmp();
        let sessions_root = dir.path().join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();

        let dodge = dir
            .path()
            .join("not-yet")
            .join("..")
            .join("sessions")
            .join("chat-victim");
        let roots = RootSet::scoped(vec![dodge.clone()], vec![sessions_root.clone()]);

        assert_eq!(
            judge(&roots, &dodge.join("session.jsonl")),
            Containment::Outside,
            "a `..` through a missing directory must not smuggle a root into the denial"
        );
        assert_eq!(
            judge(&roots, &victim.join("session.jsonl")),
            Containment::Outside,
            "nor under the name it actually resolves to"
        );
    }

    /// And with the depth hidden behind a symlink instead. The drop rule is
    /// decided on both forms of the root, because a root that merely *resolves*
    /// into a denial is as dangerous as one that names it.
    #[cfg(unix)]
    #[test]
    fn a_root_that_reaches_a_denial_through_a_symlink_is_dropped() {
        let dir = tmp();
        let sessions_root = dir.path().join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();
        let innocent = dir.path().join("innocent-notes");
        std::os::unix::fs::symlink(&victim, &innocent).unwrap();

        let roots = RootSet::scoped(vec![innocent.clone()], vec![sessions_root.clone()]);

        assert_eq!(
            judge(&roots, &innocent.join("session.jsonl")),
            Containment::Outside,
            "a symlinked root landing in the denial must be dropped like a named one"
        );
    }

    /// The other direction of the drop rule, and the one only the root's
    /// lexical form can catch: a root NAMED inside the denial that resolves
    /// somewhere innocent. Admitting it would put a sessions-root path back
    /// into the allowlist, and answering at all tells the caller which session
    /// ids exist.
    #[cfg(unix)]
    #[test]
    fn a_root_named_inside_a_denial_is_dropped_even_when_it_resolves_somewhere_allowed() {
        let dir = tmp();
        let sessions_root = dir.path().join("sessions");
        let innocent = dir.path().join("notes");
        std::fs::create_dir_all(&sessions_root).unwrap();
        std::fs::create_dir_all(&innocent).unwrap();
        let decoy = sessions_root.join("chat-decoy");
        std::os::unix::fs::symlink(&innocent, &decoy).unwrap();
        std::fs::write(innocent.join("note.md"), "note").unwrap();

        let roots = RootSet::scoped(vec![decoy.clone()], vec![sessions_root.clone()]);

        assert_eq!(
            judge(&roots, &decoy.join("note.md")),
            Containment::Outside,
            "a root spelled inside the denied root must be dropped however it resolves"
        );
    }

    /// The nested exception deepest-match-wins still exists for: a session's
    /// own storage directory lives inside the sessions root that hides every
    /// other session's. It is admitted because the daemon carved it out by
    /// name, not because it happened to be deeper.
    #[test]
    fn a_carve_out_reaches_inside_the_denial_that_encloses_it() {
        let dir = tmp();
        let data_root = dir.path().join("crucible");
        let sessions_root = data_root.join("sessions");
        let own = sessions_root.join("chat-mine");
        let other = sessions_root.join("chat-theirs");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&other).unwrap();

        let roots = RootSet::scoped(vec![data_root.clone()], vec![sessions_root.clone()])
            .carve_out(vec![own.clone()]);

        assert!(
            permits(&roots, &own.join("session.jsonl")),
            "a session must still reach its own storage"
        );
        assert_eq!(
            judge(&roots, &other.join("session.jsonl")),
            Containment::Outside,
            "the carve-out must not widen past itself"
        );
        assert!(
            permits(&roots, &data_root.join("Note.md")),
            "denying the sessions root must not cost the kiln around it"
        );
        assert_eq!(
            judge(&roots, &sessions_root),
            Containment::Outside,
            "the denied root itself must not be enumerable"
        );
    }

    /// [`RootSet::carve_out`] is the one deliberate exemption from the drop
    /// rule, so its contract needs pinning: a carve-out re-admits what is
    /// *inside* a denial, it does not repeal the denial itself.
    ///
    /// Where specificity does not separate an allow from a deny, the deny wins
    /// — Codex resolves overlapping policy the same way, most specific first
    /// and ties to the more restrictive mode. A carve-out that is no narrower
    /// than the denial it sits in has not named an exception; it has named the
    /// rule, and honoring it would hand back the whole sessions root.
    #[test]
    fn a_carve_out_no_narrower_than_its_denial_does_not_repeal_it() {
        let dir = tmp();
        let sessions_root = dir.path().join("sessions");
        std::fs::create_dir_all(sessions_root.join("chat-victim")).unwrap();

        let roots = RootSet::scoped(vec![dir.path().to_path_buf()], vec![sessions_root.clone()])
            .carve_out(vec![sessions_root.clone()]);

        assert_eq!(
            judge(
                &roots,
                &sessions_root.join("chat-victim").join("session.jsonl")
            ),
            Containment::Outside,
            "a carve-out equal to the denied root must not re-open it"
        );
    }

    /// The carve-out is a READ exception, and only a read exception.
    ///
    /// Anthropic blocks writes to `~/.claude/projects/**.jsonl` as defense in
    /// depth against prompt-injection tampering, and leaves reads open: a
    /// transcript is replayed into a future context, so editing one is the
    /// attack and reading one is not. A session must reach its own storage
    /// directory — that is where its spilled tool output lands — and it must
    /// never rewrite the record of what it did.
    #[test]
    fn a_session_may_read_its_own_storage_directory_but_not_write_it() {
        let dir = tmp();
        let data_root = dir.path().join("crucible");
        let sessions_root = data_root.join("sessions");
        let own = sessions_root.join("chat-mine");
        std::fs::create_dir_all(&own).unwrap();

        let roots = RootSet::scoped(vec![data_root.clone()], vec![sessions_root.clone()])
            .carve_out(vec![own.clone()]);

        let spilled = own.join("tools").join("bash-1.txt");
        assert!(
            permits(&roots, &spilled),
            "the session's own spilled tool output must stay readable"
        );
        assert!(
            !writable(&roots, &spilled),
            "a carve-out re-admits reads only — the sessions root is write-denied"
        );
        assert!(
            !writable(&roots, &own.join("session.jsonl")),
            "least of all the transcript itself"
        );
        assert!(
            writable(&roots, &data_root.join("Note.md")),
            "write-denying the sessions root must not cost the kiln around it"
        );
    }

    /// Write protection is not ranked, so nothing can out-rank it: a root
    /// carved deeper than the denial, which is how a read exception is
    /// legitimately expressed, buys no write.
    #[test]
    fn a_deeper_carve_out_does_not_out_rank_the_write_denial() {
        let dir = tmp();
        let sessions_root = dir.path().join("sessions");
        let deep = sessions_root.join("chat-mine").join("tools").join("nested");
        std::fs::create_dir_all(&deep).unwrap();

        let roots = RootSet::scoped(vec![dir.path().to_path_buf()], vec![sessions_root])
            .carve_out(vec![deep.clone()]);

        assert!(permits(&roots, &deep.join("out.txt")));
        assert!(!writable(&roots, &deep.join("out.txt")));
    }

    /// [`RootSet::carve_out`] grants reads, and reads are the whole of what it
    /// grants — wherever it points.
    ///
    /// Its only caller today aims it inside the sessions-root denial, where
    /// the unranked write check refuses anyway, so the two rules overlap and
    /// either alone would look sufficient. They are not the same rule: this
    /// one is a property of the *builder*, and it is what stops the next
    /// carve-out — aimed somewhere with no denial around it — from silently
    /// being a write grant as well.
    #[test]
    fn a_carve_out_grants_reads_and_never_writes() {
        let dir = tmp();
        let kiln = dir.path().join("kiln");
        let extra = dir.path().join("elsewhere");
        std::fs::create_dir_all(&kiln).unwrap();
        std::fs::create_dir_all(&extra).unwrap();

        let roots = RootSet::scoped(vec![kiln.clone()], vec![]).carve_out(vec![extra.clone()]);

        assert!(permits(&roots, &extra.join("f.txt")));
        assert!(
            !writable(&roots, &extra.join("f.txt")),
            "a carve-out is a read exception even where nothing denies it"
        );
        assert!(
            writable(&roots, &kiln.join("f.txt")),
            "an ordinary allowed root is still writable"
        );
    }

    /// Roots that are write-protected without being read-denied: the runtime
    /// tree the daemon loads plugins from, and the personal config directory.
    /// Reading a plugin's source is ordinary work; writing one is arbitrary
    /// code execution in the daemon on its next start.
    #[test]
    fn a_protected_root_is_write_denied_while_it_stays_readable() {
        let dir = tmp();
        let runtime = dir.path().join("runtime");
        std::fs::create_dir_all(runtime.join("plugins")).unwrap();

        let roots =
            RootSet::scoped(vec![dir.path().to_path_buf()], vec![]).protect(vec![runtime.clone()]);

        let plugin = runtime.join("plugins").join("oci").join("init.lua");
        assert!(permits(&roots, &plugin), "plugin sources stay readable");
        assert!(!writable(&roots, &plugin));
        assert!(writable(&roots, &dir.path().join("note.md")));
    }

    /// `""` names no directory, and resolution anchors a relative path at the
    /// daemon's working directory — so keeping an empty root would grant
    /// whichever directory the daemon was started in, chosen by nobody. A
    /// kiln-less, workspace-less session carries `""` in both roles, so this is
    /// the shape that reaches here, not a hypothetical.
    #[test]
    fn an_empty_root_does_not_become_the_daemons_working_directory() {
        let cwd = std::env::current_dir().unwrap();
        let roots = RootSet::scoped(vec![PathBuf::new()], vec![]);

        assert_eq!(
            judge(&roots, &cwd.join("Cargo.toml")),
            Containment::Outside,
            "an empty root must admit nothing, least of all the daemon's cwd"
        );
    }
}
