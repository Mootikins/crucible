//! The one door every tool family walks through to reach the filesystem.
//!
//! [`super::containment`] decides *whether* a path is reachable. This module
//! decides *how a tool asks*, and it exists because the answer to the first
//! question is worthless if a tool can decline to ask it.
//!
//! ## Why a shared check was not enough
//!
//! The predecessor was a shared function — `validate_path_within_kiln` — and
//! the note, search and kiln tools all called it. It still leaked, because a
//! shared *check* enforces whatever its caller happens to pass and knows only
//! what its caller happens to tell it:
//!
//! - It knew "inside the kiln", so a kiln that *enclosed* the sessions root
//!   (which is every kiln-less session, whose kiln is the data root) handed
//!   `read_note` a path with no `.crucible` component in it and no reason to
//!   refuse. `read_file` refused the same file, because the workspace tools
//!   asked a different question.
//! - It judged the path a caller *wrote*, so the tools that produce paths by
//!   walking the filesystem — `list_notes`, `property_search`, `get_kiln_info`,
//!   the bare-filename lookup behind `read_note` — never reached it at all.
//!
//! Hermes ran this experiment for us: they extracted exactly this helper
//! (`tools/path_security.py::validate_within_dir`) because the check had
//! drifted across five tools, and their own docstring concedes "this is NOT a
//! security boundary". A shared function depends on every future call site
//! electing to call it, which is a property no reviewer can verify once and
//! rely on.
//!
//! ## What replaces it
//!
//! A tool family no longer holds a root path. It holds an [`FsScope`], and the
//! only thing an [`FsScope`] will give it is a [`ContainedPath`] — a newtype
//! whose field is private to this module, with no public constructor, no
//! `From<PathBuf>`, and no way to build one from a string that has not been
//! through [`FsScope::resolve`]. Every signature downstream that takes a
//! `&ContainedPath` therefore carries a proof, checked by the compiler, that
//! containment was consulted.
//!
//! Writing needs a stronger proof, so it has its own type. A write must clear
//! everything a read clears plus the hardcoded protected set
//! ([`super::protected`]) and the write-denied roots, and only
//! [`FsScope::resolve_for_write`] produces the [`WritablePath`] the writing
//! tools take. There is no `From<ContainedPath>`, deliberately: the whole
//! value of the split is that `grep resolve_for_write` enumerates the write
//! surface, and a conversion would let a call site opt out silently.
//!
//! Enumeration goes through the same door: [`FsScope::walk_files`] and
//! [`FsScope::read_dir`] apply the rule to what the walk YIELDS, so pointing a
//! walker at an allowed root cannot rake in a denied subtree beneath it. This
//! is the in-process half of the design's §2. `cap-std`'s `Dir` (one `openat2`
//! with `RESOLVE_BENEATH`, which also closes check-then-open TOCTOU) is the
//! follow-up that replaces the body of this module without changing its shape:
//! the callers already ask a handle instead of joining a root.
//!
//! ## What it still cannot do
//!
//! `ContainedPath::as_path` exists, because ripgrep and `walkdir` take paths.
//! Nothing stops code in this process calling `std::fs` on a path it built
//! itself — cap-std's maintainer states the same limitation of `Dir`, and it
//! is why the design pairs this layer with Landlock. What this layer buys is
//! that the ergonomic route and the safe route are the same route.
//!
//! `bash` is the honest hole and it is a big one: a shell command reaches the
//! filesystem through a process this one does not mediate, so neither the root
//! set nor the protected set applies to it. Everything said here about
//! transcripts and protected paths is therefore a claim about the *file
//! tools*, and the kernel backstop is what would make it a claim about the
//! session. Until then this is defense in depth, in Hermes's sense of the
//! phrase, and it should be described that way rather than as a boundary.

use super::containment::{Access, Containment, RootSet};
use super::path_resolution::ResolvedPath;
use super::protected;
use std::path::{Component, Path, PathBuf};

/// The kiln's control directory: `kiln.toml`, the SQLite index, transcripts
/// migration did not reach, and the plugin and skill definitions the daemon
/// later loads and EXECUTES. That last part makes it a protected path in
/// Claude Code's sense (CVE-2026-25725: sandboxed code wrote a config the host
/// process then honored), so kiln-scoped tools may not reach it in either
/// direction, whatever the allowlist says.
const CONTROL_DIR: &str = ".crucible";

/// A path that has cleared containment.
///
/// The field is private to this module and there is no constructor outside it:
/// the only way to hold one is to have asked an [`FsScope`]. That is the whole
/// point — a function taking `&ContainedPath` cannot be handed a raw
/// caller-supplied string by a future tool that forgot the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContainedPath(PathBuf);

impl ContainedPath {
    /// The path itself, for the APIs that take `&Path` (`walkdir`, ripgrep,
    /// `std::fs`). Handing this out is safe in the sense that matters: the
    /// check already happened, and it happened on the form returned here.
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn exists(&self) -> bool {
        self.0.exists()
    }

    pub(crate) fn display(&self) -> std::path::Display<'_> {
        self.0.display()
    }
}

/// A path that has cleared containment **and** write protection.
///
/// Same construction rule as [`ContainedPath`] and for the same reason, one
/// step stronger: a function taking `&WritablePath` cannot be handed the
/// result of a read check. The two types are what makes the read/write split
/// reviewable — `grep resolve_for_write` enumerates every write in the tool
/// surface, and a new tool that writes what it resolved with [`FsScope::resolve`]
/// does not compile.
///
/// It is deliberately NOT convertible from [`ContainedPath`]: `From` would
/// re-open by ergonomics what the type system just closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WritablePath(PathBuf);

impl WritablePath {
    /// The path itself, for `std::fs`. Same honest limit as
    /// [`ContainedPath::as_path`]: the check happened, on this form.
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn exists(&self) -> bool {
        self.0.exists()
    }
}

/// How a scope's holder is allowed to name a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Naming {
    /// Absolute paths accepted; relative ones anchor at the scope's anchor.
    /// The anchor is not itself a grant — whether the holder may read it is a
    /// question only the root set answers. This is the workspace family
    /// (`read_file`, `write_file`, `glob`, `grep`).
    Anywhere,
    /// Kiln-relative only: absolute paths and `..` are refused by name, the
    /// anchor is a boundary of its own, and [`CONTROL_DIR`] is unreachable at
    /// any depth. This is the note/search/kiln family, whose tools address
    /// notes rather than arbitrary files.
    UnderAnchor,
}

/// A tool family's filesystem capability: an anchor, the roots it may reach,
/// and the rules for turning a caller's string into a path.
#[derive(Debug, Clone)]
pub(crate) struct FsScope {
    /// Where relative paths are joined and results are reported relative to,
    /// exactly as written by the caller who built the scope.
    ///
    /// Held as a raw path deliberately: it is a *display and join base*, not a
    /// grant, and normalizing it here would change how `glob` reports results
    /// for the ambient tool sets. Everything that decides reachability uses
    /// [`Self::anchor_resolved`] instead.
    anchor: PathBuf,
    /// The anchor in the two forms a containment comparison needs.
    anchor_resolved: ResolvedPath,
    naming: Naming,
    /// The anchor as a boundary of its own — `Some` exactly when
    /// [`Naming::UnderAnchor`]. A kiln tool may not leave its kiln even when
    /// the session's root set would allow the destination, because "somewhere
    /// else the session can read" is still not a note in this kiln.
    anchor_roots: Option<RootSet>,
    /// The session's default-deny allowlist. [`RootSet::Ambient`] at the
    /// construction sites that predate containment (the daemon-global tool set
    /// behind the Lua bridge, and the MCP servers built outside a session),
    /// which answer to the permission gate instead.
    session: RootSet,
}

impl FsScope {
    /// The workspace family: absolute paths allowed, containment decides.
    pub(crate) fn workspace(anchor: impl Into<PathBuf>, session: RootSet) -> Self {
        Self::build(anchor.into(), Naming::Anywhere, session)
    }

    /// The kiln family: kiln-relative names only, the kiln is a boundary, and
    /// the control directory is protected.
    pub(crate) fn kiln(anchor: impl Into<PathBuf>, session: RootSet) -> Self {
        Self::build(anchor.into(), Naming::UnderAnchor, session)
    }

    fn build(anchor: PathBuf, naming: Naming, session: RootSet) -> Self {
        let anchor_resolved = ResolvedPath::resolve(&anchor);
        // Normalized once, at construction, like every other root: a scope is
        // a policy input and must mean the same thing for the life of the tool
        // set. A symlink swapped under the anchor afterwards fails closed
        // rather than being re-resolved into a new boundary.
        let anchor_roots = match naming {
            Naming::Anywhere => None,
            Naming::UnderAnchor => Some(RootSet::scoped(vec![anchor.clone()], vec![])),
        };
        Self {
            anchor,
            anchor_resolved,
            naming,
            anchor_roots,
            session,
        }
    }

    /// Replace the session root set, keeping the anchor and naming rules.
    ///
    /// Used by the builders that construct a tool family before the session's
    /// containment is known.
    #[must_use]
    pub(crate) fn with_containment(mut self, session: RootSet) -> Self {
        self.session = session;
        self
    }

    /// The join/display base, as written. Never join caller input onto this —
    /// [`Self::resolve`] is the only sanctioned way from a caller's string to
    /// a path, and it does this join itself.
    pub(crate) fn anchor(&self) -> &Path {
        &self.anchor
    }

    /// Whether this scope imposes no boundary beyond its anchor.
    pub(crate) fn is_ambient(&self) -> bool {
        self.session.is_ambient() && self.anchor_roots.is_none()
    }

    /// Strip the anchor off `path` for reporting, trying every form the anchor
    /// has. A walk rooted at the canonical anchor yields canonical paths while
    /// the anchor was written as a symlinked spelling (`/tmp` on macOS), and a
    /// single-form `strip_prefix` silently drops every result in that case.
    pub(crate) fn relativize<'p>(&self, path: &'p Path) -> &'p Path {
        [
            self.anchor_resolved.canonical(),
            self.anchor_resolved.lexical(),
            self.anchor.as_path(),
        ]
        .into_iter()
        .find_map(|base| path.strip_prefix(base).ok())
        .unwrap_or(path)
    }

    /// Turn a caller-supplied path into a [`ContainedPath`], or refuse it.
    ///
    /// The naming rules are a pre-check that exists for the error message —
    /// design §3, and Codex's `absolutize` for the same reason. They are not
    /// the boundary: [`Self::judge`] is, and it runs on both resolved forms
    /// whether or not the name looked innocent.
    pub(crate) fn resolve(&self, user_path: &str) -> Result<ContainedPath, rmcp::ErrorData> {
        self.resolve_access(user_path, Access::Read)
            .map(ContainedPath)
    }

    /// Turn a caller-supplied path into a [`WritablePath`], or refuse it.
    ///
    /// Everything [`Self::resolve`] checks, plus the two things a write has to
    /// answer for that a read does not: the hardcoded protected set
    /// ([`super::protected`]), which no configuration reopens, and the denied
    /// and protected roots taken absolutely rather than by ranking — so the
    /// sessions root is write-denied even to the session whose own storage
    /// directory the allowlist carved back out for reads.
    pub(crate) fn resolve_for_write(
        &self,
        user_path: &str,
    ) -> Result<WritablePath, rmcp::ErrorData> {
        self.resolve_access(user_path, Access::Write)
            .map(WritablePath)
    }

    fn resolve_access(&self, user_path: &str, access: Access) -> Result<PathBuf, rmcp::ErrorData> {
        let requested = Path::new(user_path);
        if self.naming == Naming::UnderAnchor {
            if requested.is_absolute() {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("Absolute paths are not allowed for security reasons: {user_path}"),
                    None,
                ));
            }
            if requested
                .components()
                .any(|c| matches!(c, Component::ParentDir))
            {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("Path traversal is not allowed for security reasons: {user_path}"),
                    None,
                ));
            }
        }

        let joined = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.anchor.join(requested)
        };

        // An ambient READ has no boundary to check and no path to resolve —
        // returned as written rather than canonicalized so `glob`'s
        // anchor-relative output stays stable for the tool sets that predate
        // containment. An ambient WRITE still falls through, because the
        // protected set below is not a boundary this scope can be without.
        if self.is_ambient() && access == Access::Read {
            return Ok(joined);
        }

        let resolved = ResolvedPath::resolve(&joined);

        // Ahead of every boundary and outside every policy. This is the rule
        // an allow cannot reach: it is checked for `Ambient` scopes too, which
        // is containment switched off, so there is no configuration of this
        // process under which a protected write succeeds.
        if access == Access::Write {
            if let Some(protection) = protected::write_protection(&resolved) {
                return Err(rmcp::ErrorData::invalid_params(
                    protection.message(user_path),
                    None,
                ));
            }
        }

        if self.is_ambient() {
            return Ok(joined);
        }

        if self.protects_control_dir() && self.enters_control_dir(&resolved) {
            return Err(rmcp::ErrorData::invalid_params(
                format!(
                    "'{user_path}' is inside the kiln's {CONTROL_DIR}/ control directory, which \
                     holds configuration, indexes and recorded transcripts rather than notes"
                ),
                None,
            ));
        }

        match self.judge_resolved(&resolved, access) {
            Verdict::Permitted(path) => Ok(path),
            Verdict::WriteProtected(root) => Err(rmcp::ErrorData::invalid_params(
                format!(
                    "Refusing to write '{user_path}': it is under '{}', which is write-protected \
                     for every session. Transcripts are replayed into future context and plugin \
                     trees are executed by the daemon, so both are read-only to tools — no \
                     permission rule can re-enable this.",
                    root.display()
                ),
                None,
            )),
            Verdict::LeftAnchor(escape) => Err(rmcp::ErrorData::invalid_params(
                match escape {
                    // Named separately from a plain refusal: "you asked for a
                    // path inside, and it points outside" is the fact worth
                    // surfacing, and folding it into a generic message is what
                    // made symlink handling untestable in CVE-2026-39861 and
                    // CVE-2026-50549.
                    Some(target) => format!(
                        "Path escapes kiln directory via a symlink to '{}': {user_path}",
                        target.display()
                    ),
                    None => format!("Path escapes kiln directory: {user_path}"),
                },
                None,
            )),
            Verdict::OutsideRoots(escape) => Err(rmcp::ErrorData::invalid_params(
                match escape {
                    Some(target) => format!(
                        "Path '{user_path}' resolves through a symlink to '{}', which is outside \
                         this session's allowed roots (workspace, kilns, session dir).",
                        target.display()
                    ),
                    None => format!(
                        "Path '{user_path}' is outside this session's allowed roots \
                         (workspace, kilns, session dir). Use a path inside the workspace."
                    ),
                },
                None,
            )),
        }
    }

    /// Resolve an optional folder parameter: `None` is the anchor itself,
    /// still judged — a kiln the session may not reach is not readable just
    /// because the caller declined to name a subdirectory.
    pub(crate) fn resolve_folder(
        &self,
        folder: Option<&str>,
    ) -> Result<ContainedPath, rmcp::ErrorData> {
        self.resolve(folder.unwrap_or(""))
    }

    /// Whether an already-absolute path — one this scope did not produce, such
    /// as a line of ripgrep output or a `glob` result — may be touched.
    pub(crate) fn admits(&self, path: &Path) -> bool {
        if self.is_ambient() {
            return true;
        }
        let resolved = ResolvedPath::resolve(path);
        if self.protects_control_dir() && self.enters_control_dir(&resolved) {
            return false;
        }
        matches!(
            self.judge_resolved(&resolved, Access::Read),
            Verdict::Permitted(_)
        )
    }

    /// Walk `root` recursively, yielding only the files this scope admits.
    ///
    /// A denied directory is PRUNED rather than merely filtered, so the walk
    /// neither descends into it nor reports its existence. This is the half
    /// `validate_path_within_kiln` could not cover: it judged the path a
    /// caller wrote, and a walker writes its own.
    pub(crate) fn walk_files<'a>(
        &'a self,
        root: &ContainedPath,
    ) -> impl Iterator<Item = walkdir::DirEntry> + 'a {
        walkdir::WalkDir::new(root.as_path())
            .follow_links(false)
            .into_iter()
            .filter_entry(move |entry| entry.depth() == 0 || self.admits(entry.path()))
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_type().is_file())
    }

    /// The immediate children of `dir` this scope admits.
    pub(crate) fn read_dir(&self, dir: &ContainedPath) -> std::io::Result<Vec<ContainedPath>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir.as_path())? {
            let path = entry?.path();
            if self.admits(&path) {
                out.push(ContainedPath(path));
            }
        }
        Ok(out)
    }

    fn protects_control_dir(&self) -> bool {
        self.naming == Naming::UnderAnchor
    }

    /// Whether either resolved form enters the control directory at any depth.
    ///
    /// Both forms, because they answer different questions and a symlink
    /// separates them: a name that never mentions `.crucible` can land in it,
    /// and a name that does mention it is an oracle for what is there even
    /// when it resolves elsewhere.
    fn enters_control_dir(&self, resolved: &ResolvedPath) -> bool {
        [resolved.lexical(), resolved.canonical()]
            .into_iter()
            .any(|form| {
                self.relativize(form)
                    .components()
                    .any(|c| c.as_os_str() == CONTROL_DIR)
            })
    }

    /// Judge a resolved path against the anchor boundary and the session's
    /// roots, taking the weaker of the two.
    ///
    /// The anchor boundary is asked as a READ regardless of `access`: it holds
    /// no denials and no protected roots (it is `RootSet::scoped(anchor, [])`),
    /// so a write verdict from it could only ever be the read one, and asking
    /// it for a write would let a future anchor rule report "write-protected"
    /// where the true fact is "outside your kiln".
    fn judge_resolved(&self, resolved: &ResolvedPath, access: Access) -> Verdict {
        if let Some(anchor_roots) = &self.anchor_roots {
            match anchor_roots.judge_resolved(resolved, Access::Read) {
                Containment::Permitted(_) => {}
                Containment::Outside => return Verdict::LeftAnchor(None),
                Containment::SymlinkEscape { canonical_target } => {
                    return Verdict::LeftAnchor(Some(canonical_target))
                }
                Containment::WriteProtected { root } => return Verdict::WriteProtected(root),
            }
        }
        match self.session.judge_resolved(resolved, access) {
            Containment::Permitted(path) => Verdict::Permitted(path),
            Containment::Outside => Verdict::OutsideRoots(None),
            Containment::SymlinkEscape { canonical_target } => {
                Verdict::OutsideRoots(Some(canonical_target))
            }
            Containment::WriteProtected { root } => Verdict::WriteProtected(root),
        }
    }
}

/// Which boundary refused, so the refusal can say something true.
///
/// "Outside the kiln" and "outside what this session may read" are different
/// facts, and a kiln tool can hit either: a note tool reaching for `../etc` has
/// left its kiln, while one reaching for a transcript under a kiln that
/// encloses the sessions root has not.
enum Verdict {
    Permitted(PathBuf),
    LeftAnchor(Option<PathBuf>),
    OutsideRoots(Option<PathBuf>),
    /// A write to a root that is write-denied for every session — a transcript
    /// directory or a tree the daemon executes.
    WriteProtected(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn kiln_scope(dir: &TempDir) -> FsScope {
        FsScope::kiln(dir.path().to_path_buf(), RootSet::Ambient)
    }

    /// The naming rules, which are the pre-check the note tools have always
    /// had. They stay because a good message beats a generic one, not because
    /// they are the boundary.
    #[test]
    fn a_kiln_scope_refuses_absolute_paths_and_upward_traversal() {
        let dir = TempDir::new().unwrap();
        let scope = kiln_scope(&dir);

        for path in ["/etc/passwd", "/root/.ssh/id_rsa"] {
            let err = scope.resolve(path).unwrap_err();
            assert!(
                err.message.contains("Absolute paths are not allowed"),
                "{path}: {}",
                err.message
            );
        }
        for path in ["../etc/passwd", "notes/../../etc/passwd", ".."] {
            let err = scope.resolve(path).unwrap_err();
            assert!(
                err.message.contains("Path traversal"),
                "{path}: {}",
                err.message
            );
        }
        assert!(scope.resolve("notes/real-note.md").is_ok());
        assert!(scope.resolve("").is_ok(), "the kiln root itself");
        assert!(scope.resolve(".").is_ok());
    }

    /// Declining to name a subdirectory is not a way past the door.
    ///
    /// The shape that produces an unreachable anchor is a kiln the allowlist
    /// DROPPED — one naming another session's storage directory. `list_notes`
    /// and `property_search` both reach the filesystem with no folder at all,
    /// so "no folder" has to be a path like any other rather than a shortcut
    /// back to the raw anchor.
    #[test]
    fn a_kiln_scope_whose_anchor_is_not_admitted_refuses_the_folderless_form() {
        let dir = TempDir::new().unwrap();
        let sessions_root = dir.path().join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::write(victim.join("session.md"), "SECRET").unwrap();

        // The kiln IS the victim's storage directory, so `RootSet::scoped`
        // drops it and the allowlist comes out empty.
        let scope = FsScope::kiln(
            victim.clone(),
            RootSet::scoped(vec![victim], vec![sessions_root]),
        );

        assert!(
            scope.resolve_folder(None).is_err(),
            "the folderless form must be judged, not waved through"
        );
        assert!(scope.resolve_folder(Some("session.md")).is_err());
    }

    /// A kiln scope may not leave its kiln however the exit is spelled, and a
    /// symlink out is reported as its own outcome rather than merged into the
    /// generic refusal.
    #[cfg(unix)]
    #[test]
    fn a_kiln_scope_refuses_a_symlink_out_of_the_kiln() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("evil_link")).unwrap();

        let err = kiln_scope(&dir)
            .resolve("evil_link/secret.txt")
            .unwrap_err();

        assert!(
            err.message.contains("escapes kiln directory"),
            "{}",
            err.message
        );
        assert!(
            err.message.contains("symlink"),
            "a link out is a distinct fact and the message must say so: {}",
            err.message
        );
    }

    /// The control directory is not a note, at any depth, however it is
    /// spelled, and for writes as much as reads.
    #[test]
    fn a_kiln_scope_refuses_the_control_directory() {
        let dir = TempDir::new().unwrap();
        let scope = kiln_scope(&dir);

        for path in [
            ".crucible/kiln.toml",
            ".crucible/sessions/chat-victim/session.jsonl",
            ".crucible",
            "notes/.crucible/anything",
        ] {
            assert!(
                scope.resolve(path).is_err(),
                "{path} is inside the kiln's control directory and must be refused"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_cannot_lead_a_kiln_scope_into_the_control_directory() {
        let dir = TempDir::new().unwrap();
        let legacy = dir.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("session.jsonl"), "SECRET").unwrap();
        std::os::unix::fs::symlink(dir.path().join(".crucible"), dir.path().join("shortcut"))
            .unwrap();

        let scope = kiln_scope(&dir);

        assert!(
            scope.resolve("shortcut/sessions/session.jsonl").is_err(),
            "a symlink into the control directory must be refused like the name itself"
        );
        assert!(
            scope.resolve("shortcut/sessions/planted.md").is_err(),
            "and so must a leaf that does not exist yet — the branch a write creates"
        );
    }

    /// The hole a shared *check* could not close: the kiln encloses the
    /// sessions root, so nothing about the NAME of a transcript path says it is
    /// forbidden. Only the session's root set knows, and now the kiln scope
    /// asks it.
    #[test]
    fn a_kiln_scope_honors_the_sessions_denial_the_kiln_encloses() {
        let dir = TempDir::new().unwrap();
        let sessions_root = dir.path().join("sessions");
        let victim = sessions_root.join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::write(victim.join("session.jsonl"), "SECRET").unwrap();
        std::fs::write(dir.path().join("note.md"), "note").unwrap();

        let scope = FsScope::kiln(
            dir.path().to_path_buf(),
            RootSet::scoped(vec![dir.path().to_path_buf()], vec![sessions_root.clone()]),
        );

        assert!(
            scope.resolve("sessions/chat-victim/session.jsonl").is_err(),
            "a transcript with no control-directory component in its name must still be refused"
        );
        assert!(
            scope.resolve("note.md").is_ok(),
            "denying the sessions root must not cost the kiln around it"
        );
    }

    /// Containment applies to what a WALK yields, not only to what a caller
    /// writes. `list_notes`, `property_search` and the bare-filename lookup
    /// behind `read_note` all produce their own paths.
    #[test]
    fn a_walk_yields_nothing_from_a_denied_subtree() {
        let dir = TempDir::new().unwrap();
        let sessions_root = dir.path().join("sessions");
        std::fs::create_dir_all(sessions_root.join("chat-victim")).unwrap();
        std::fs::write(
            sessions_root.join("chat-victim").join("session.md"),
            "SECRET",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".crucible")).unwrap();
        std::fs::write(dir.path().join(".crucible").join("kiln.toml"), "x").unwrap();
        std::fs::write(dir.path().join("note.md"), "note").unwrap();

        let scope = FsScope::kiln(
            dir.path().to_path_buf(),
            RootSet::scoped(vec![dir.path().to_path_buf()], vec![sessions_root]),
        );
        let root = scope.resolve("").unwrap();

        let walked: Vec<String> = scope
            .walk_files(&root)
            .map(|e| scope.relativize(e.path()).display().to_string())
            .collect();

        assert_eq!(
            walked,
            vec!["note.md".to_string()],
            "the walk must yield the kiln's notes and nothing from a denied subtree"
        );
    }

    /// The same rule for the non-recursive listing, which reads one directory
    /// rather than walking.
    #[test]
    fn a_directory_listing_hides_a_denied_child() {
        let dir = TempDir::new().unwrap();
        let sessions_root = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_root).unwrap();
        std::fs::write(dir.path().join("note.md"), "note").unwrap();

        let scope = FsScope::kiln(
            dir.path().to_path_buf(),
            RootSet::scoped(vec![dir.path().to_path_buf()], vec![sessions_root]),
        );
        let root = scope.resolve("").unwrap();

        let listed: Vec<String> = scope
            .read_dir(&root)
            .unwrap()
            .iter()
            .map(|path| scope.relativize(path.as_path()).display().to_string())
            .collect();

        assert_eq!(listed, vec!["note.md".to_string()]);
    }

    /// A workspace scope's anchor is where relative paths land, NOT a grant.
    /// Folding it into the allowlist is how a detached workspace put `""`
    /// there.
    #[test]
    fn a_workspace_scope_does_not_admit_its_own_anchor() {
        let dir = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scope = FsScope::workspace(
            dir.path().to_path_buf(),
            RootSet::scoped(vec![elsewhere.path().to_path_buf()], vec![]),
        );

        assert!(
            scope.resolve("file.txt").is_err(),
            "the anchor must not admit itself"
        );
        assert!(scope
            .resolve(&elsewhere.path().join("ok.txt").to_string_lossy())
            .is_ok());
    }

    /// A workspace scope with the whole tempdir admitted, which is the widest
    /// policy a session can legitimately be given.
    fn open_workspace(dir: &TempDir) -> FsScope {
        FsScope::workspace(
            dir.path().to_path_buf(),
            RootSet::scoped(vec![dir.path().to_path_buf()], vec![]),
        )
    }

    /// CVE-2026-25725, in our shape. The daemon loads and EXECUTES what lives
    /// in these directories on its next start — `.crucible/project.toml`,
    /// `.crucible/plugins/*.lua`, a git hook, another harness's settings — so
    /// an agent that can write one has escaped containment through a trusted
    /// process rather than through the filesystem.
    ///
    /// Every path here is one that DOES NOT EXIST YET. That is the whole of
    /// the CVE: bubblewrap protected `.claude/settings.json` only where it
    /// already existed, so sandboxed code created it.
    #[test]
    fn a_write_to_a_path_the_daemon_later_executes_is_refused_before_it_exists() {
        let dir = TempDir::new().unwrap();
        let scope = open_workspace(&dir);

        for path in [
            ".crucible/project.toml",
            ".crucible/kiln.toml",
            ".crucible/plugins/evil/init.lua",
            ".crucible/agents/evil.md",
            ".crucible/skills/evil/SKILL.md",
            ".crucible/hooks/on_start.lua",
            ".git/hooks/post-checkout",
            ".git/config",
            ".claude/settings.json",
            ".codex/config.toml",
            ".opencode/skills/evil/SKILL.md",
            "nested/project/.git/hooks/pre-commit",
        ] {
            assert!(
                !dir.path().join(path).exists(),
                "{path} must not exist — non-existence is what the CVE exploited"
            );
            assert!(
                scope.resolve_for_write(path).is_err(),
                "a write to {path} must be refused"
            );
        }
    }

    /// Reading is not the interesting attack — Anthropic's own position, and
    /// the reason this is a WRITE protection. An agent must still be able to
    /// read the config it is asked about.
    #[test]
    fn a_protected_path_stays_readable() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git").join("config"), "[core]").unwrap();
        let scope = open_workspace(&dir);

        assert!(
            scope.resolve(".git/config").is_ok(),
            "the protected set is write-deny; reads are deliberately untouched"
        );
        assert!(scope.resolve_for_write(".git/config").is_err());
    }

    /// "No allow rule from any scope can re-open it." [`RootSet::Ambient`] is
    /// the widest policy this process can express — it is containment turned
    /// off — and it must still refuse. That is what makes the protection
    /// unreachable by the permission chain above it, which can at most produce
    /// a blanket allow.
    #[test]
    fn the_widest_possible_policy_still_refuses_a_protected_write() {
        let dir = TempDir::new().unwrap();
        let scope = FsScope::workspace(dir.path().to_path_buf(), RootSet::Ambient);

        assert!(scope.resolve("anything.txt").is_ok());
        assert!(
            scope.resolve_for_write(".crucible/project.toml").is_err(),
            "containment being off must not turn protected-path writes on"
        );
    }

    /// Claude Code's symlink corollary: a symlink AT a protected path protects
    /// what it POINTS TO. Without it the agent creates
    /// `.crucible -> ./cfg` once, then writes `cfg/project.toml` forever — the
    /// protected name never appears in the path it writes, and the daemon
    /// follows the link on its next start.
    #[cfg(unix)]
    #[test]
    fn a_symlink_at_a_protected_path_protects_what_it_points_to() {
        let dir = TempDir::new().unwrap();
        let decoy = dir.path().join("cfg");
        std::fs::create_dir_all(&decoy).unwrap();
        std::os::unix::fs::symlink(&decoy, dir.path().join(".crucible")).unwrap();
        let scope = open_workspace(&dir);

        assert!(
            scope.resolve_for_write("cfg/project.toml").is_err(),
            "the target of a symlink at a protected path is protected too"
        );
        assert!(
            scope
                .resolve_for_write("cfg/nested/plugins/init.lua")
                .is_err(),
            "and so is everything beneath it"
        );
        assert!(
            scope.resolve_for_write("notes/note.md").is_ok(),
            "a path the link does not reach stays writable"
        );
    }

    /// The other direction: a symlink pointing INTO a protected directory.
    /// Caught by judging the resolved form as well as the written one.
    #[cfg(unix)]
    #[test]
    fn a_symlink_into_a_protected_directory_is_refused_for_writes() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::os::unix::fs::symlink(dir.path().join(".claude"), dir.path().join("shortcut"))
            .unwrap();
        let scope = open_workspace(&dir);

        assert!(scope.resolve_for_write("shortcut/settings.json").is_err());
    }

    /// A workspace scope takes absolute paths — that is the difference from a
    /// kiln scope, and the reason the two are separate constructors rather
    /// than one with a flag the caller might get wrong.
    #[test]
    fn a_workspace_scope_accepts_an_absolute_path_inside_its_roots() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scope = FsScope::workspace(
            dir.path().to_path_buf(),
            RootSet::scoped(vec![dir.path().to_path_buf()], vec![]),
        );

        let resolved = scope
            .resolve(&dir.path().join("file.txt").to_string_lossy())
            .unwrap();
        assert!(resolved.exists());
    }
}
