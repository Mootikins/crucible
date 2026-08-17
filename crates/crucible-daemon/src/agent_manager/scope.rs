//! Mid-session scope mutations: the session's kiln set and workspace.
//!
//! Detach is always safe (it only shrinks future retrieval/tool scope);
//! attach-side trust validation lives in the RPC handlers, which have the
//! LLM config. Each mutation claims the session's `request_state` slot for
//! its whole duration via `RequestSlotGuard` — the same gate `send_message`
//! uses — so a mutation and a turn are mutually exclusive in both
//! directions: a send racing a mutation gets `ConcurrentRequest`, and a
//! mutation racing a send gets `ConcurrentRequest`. This closes the window
//! where a send could build and cache an agent from the pre-mutation
//! session *after* the caches were invalidated. On commit a mutation
//! invalidates BOTH the agent handle and the tool dispatcher — each bakes
//! in workspace/kiln state at build time (system prompt, WorkspaceTools,
//! kiln MCP tools), while precognition/search already read the session
//! fresh every turn.

use super::*;
use crate::event_emitter::emit_event;
use crate::kiln_registry::KilnRegistry;
use crate::tools::containment::RootSet;
use crucible_core::config::KilnName;
use crucible_core::Session;
use std::path::{Path, PathBuf};

/// The filesystem containment for a session's workspace tools: a default-deny
/// allowlist of the roots the session was given, with the transcript subtrees
/// they enclose carved back out.
///
/// **Scope** is what the caller chose — the session's kilns and its workspace.
/// Every one of those is a caller-supplied path, which is why it is an
/// allowlist entry and not a base to subtract from: a scope root that lands
/// inside a denial is dropped by [`RootSet::scoped`] rather than out-ranking
/// it. That is the whole of the fix for "attach a kiln one level deeper than
/// the denied root", including the `..`-through-a-missing-directory spelling of
/// it, because roots are normalized before the comparison.
///
/// **Denied** is every place a *transcript* lives: the flat sessions root
/// named here, and — because migration is not guaranteed to have emptied them —
/// the legacy in-kiln `.crucible/sessions`, which [`RootSet`] denies by SHAPE at
/// any depth rather than being named per root here. The shape is the point: it
/// used to be `{root}/.crucible/sessions` for each scope root, which missed
/// every project filed *beneath* a kiln or workspace — those have one too and
/// are not themselves roots, so they inherited the enclosing root's permit.
///
/// **Carved out** is the session's own storage directory, which lives inside
/// the denied sessions root. It is the one genuine nested exception, and the
/// daemon derives it from the session id it minted — nothing a caller says can
/// add to it.
///
/// **Protected** is write-denied without being read-denied: the runtime trees
/// the daemon loads and executes Lua plugins from, and the personal config
/// directory holding agent cards and skills. See
/// [`crate::tools::protected::daemon_roots`]. A denial would be wrong — an
/// agent asked about a plugin should read it — and the write must be
/// unreachable by any allow rule, so it is not ranked against anything.
///
/// A session with no kilns and no workspace therefore reaches exactly its own
/// storage directory, and reaches it read-only. An empty kiln set degrades
/// capabilities; it must never degrade containment.
///
/// Note what has NO write exception: the sessions root. Reads of the session's
/// own directory are carved out because that is where its spilled tool output
/// lands, but nothing writes there through a tool. A transcript is replayed
/// into a future context, so tampering with one is the attack that matters —
/// Anthropic blocks writes to `~/.claude/projects/**.jsonl` for exactly this
/// reason while leaving reads open.
///
/// The kilns arrive as *names*, and are resolved here through the registry —
/// the one place a name becomes a directory. A name with no entry resolves to
/// nothing and therefore grants nothing: it is not a narrower root, it is no
/// root at all. That has to be a drop rather than a fallback, because the
/// fallback shapes available (the data root, the empty path) are both roots
/// that enclose every transcript on the box.
pub(crate) fn session_containment(
    session: &Session,
    sessions_root: &Path,
    registry: &KilnRegistry,
) -> RootSet {
    let scope: Vec<PathBuf> = registry
        .paths_for(&session.kilns)
        .into_iter()
        .chain(session.workspace.iter().cloned())
        .collect();
    // The legacy in-kiln transcript directory is denied by SHAPE
    // (`RootSet`'s `.crucible/sessions` rule) rather than named per scope root:
    // a project filed beneath a kiln or workspace has one too and is not itself
    // a root, so the literal missed it.
    let denied = std::iter::once(sessions_root.to_path_buf());
    RootSet::scoped(scope, denied)
        .protect(crate::tools::protected::daemon_roots())
        .carve_out(std::iter::once(session.storage_path(sessions_root)))
}

/// The one concrete directory a session's *tools* anchor at.
///
/// Relative paths, `bash`, skills discovery, `@file` attachments and the
/// prompt's "Workspace:" line all need a directory even when the session has
/// none. Its own storage dir is the one place it certainly has, and it is
/// already inside the allowlist (read-only), so this is an ANCHOR, not a
/// grant — nothing here widens [`session_containment`], which is built from
/// the real scope and drops anything under the sessions root.
///
/// Spelled once so the fallback cannot drift between the dispatcher, the agent
/// build, the workspace snapshot and its restore. The previous spelling was the
/// empty path, which anchored a workspace-less session at the *daemon's* own
/// working directory — where `read_project_config` picked up whatever repo the
/// daemon happened to be started in.
pub(crate) fn session_tool_root(session: &Session, sessions_root: &Path) -> PathBuf {
    session
        .workspace
        .clone()
        .unwrap_or_else(|| session.storage_path(sessions_root))
}

impl AgentManager {
    /// Evict everything that baked the old scope in at build time.
    ///
    /// One lock over both, so this is atomic rather than two windows a racing
    /// build could land between — which is what the module doc above has been
    /// describing aspirationally.
    fn invalidate_scope_caches(&self, session_id: &str) {
        if let Some(slot) = self.existing_slot(session_id) {
            slot.invalidate_build();
        }
    }

    fn emit_scope_changed(
        &self,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
        session: &Session,
    ) {
        if let Some(tx) = event_tx {
            let data = serde_json::json!({
                "kilns": session.kilns,
                "workspace": session.workspace,
            });
            if !emit_event(
                tx,
                SessionEventMessage::new(&session.id, "scope_changed", data),
            ) {
                tracing::debug!("Failed to emit scope_changed event (no subscribers)");
            }
        }
    }

    /// Shared envelope for scope mutations: claim the request slot → load →
    /// apply the rule → (if changed) persist + invalidate caches + emit. The
    /// rule returns whether it changed anything; no-ops skip the write
    /// entirely. The slot claim is held for the whole body (guard dropped on
    /// return), so no `send_message` can interleave — a racing send hits the
    /// occupied slot and returns `ConcurrentRequest`, exactly as it would
    /// against another send.
    async fn mutate_scope(
        &self,
        session_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
        apply: impl FnOnce(&mut Session) -> Result<bool, AgentError>,
    ) -> Result<Session, AgentError> {
        let _slot = RequestSlotGuard::acquire(self.request_state.clone(), session_id)?;
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        if !apply(&mut session)? {
            return Ok(session);
        }

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;
        self.invalidate_scope_caches(session_id);
        self.emit_scope_changed(event_tx, &session);
        Ok(session)
    }

    /// Attach a kiln to the session's kiln set. Idempotent.
    pub async fn connect_kiln(
        &self,
        session_id: &str,
        kiln: &KilnName,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        let kiln = kiln.clone();
        self.mutate_scope(session_id, event_tx, move |session| {
            Ok(session.add_kiln(kiln))
        })
        .await
    }

    /// Detach a kiln. Any kiln may go, including the one the session was
    /// created with — the set is flat and the session is not stored in it.
    pub async fn disconnect_kiln(
        &self,
        session_id: &str,
        kiln: &KilnName,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        self.mutate_scope(
            session_id,
            event_tx,
            |session| Ok(session.remove_kiln(kiln)),
        )
        .await
    }

    /// Set or clear the session's workspace. `None` detaches, and the session
    /// is then left with no workspace at all — the same state a workspace-less
    /// create produces (see `Session::new`). It used to fall back to the kiln
    /// path, which made "acting in this project" and "acting in this corpus"
    /// the same sentence and left no way to say the session had no project.
    /// Rejected for ACP sessions, whose external agent process runs in the
    /// workspace it was spawned with.
    pub async fn set_workspace(
        &self,
        session_id: &str,
        workspace: Option<PathBuf>,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        self.mutate_scope(session_id, event_tx, move |session| {
            let is_acp = session
                .agent
                .as_ref()
                .map(|a| a.agent_type == "acp")
                .unwrap_or(false);
            if is_acp {
                return Err(AgentError::NotSupported(
                    "ACP agents run in the workspace they were spawned with — start a new session to change it"
                        .to_string(),
                ));
            }
            let before = session.workspace.clone();
            session.set_workspace(workspace);
            Ok(session.workspace != before)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    // Scope-MUTATION behavior is covered end-to-end in
    // tests/rpc_session_scope_e2e.rs — the mutations need a real
    // SessionManager + storage, which the RPC test server provides. What is
    // unit-testable here is the containment set the mutations feed.
    use super::session_containment;
    use crate::kiln_registry::KilnRegistry;
    use crate::tools::containment::RootSet;
    use crate::tools::fs_scope::FsScope;
    use crucible_core::config::KilnName;
    use crucible_core::session::SessionType;
    use crucible_core::Session;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// The data root the fixture registry below is anchored at.
    ///
    /// Named rather than inlined because the tests *probe* it: a resolution
    /// that answered an unresolvable name with "the data root" — the shape of
    /// every fallback this design refuses — would put exactly this directory
    /// into the allowlist, and a probe somewhere else would never see it.
    const REGISTRY_DATA_ROOT: &str = "/nonexistent-data-root";

    /// A session whose kilns are `paths`, plus the registry that maps its names
    /// back to them.
    ///
    /// Every assertion in this module is about *directories*, so the names are
    /// incidental: each path is registered under its own basename. The registry
    /// is anchored at a data root none of these paths encloses, so the
    /// registration floor lets them through and what is under test stays
    /// `session_containment`'s ranking rather than the floor's.
    fn session_over(paths: &[&Path]) -> (Session, Arc<KilnRegistry>) {
        let names: Vec<String> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .and_then(KilnName::normalize)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("kiln-{i}"))
            })
            .collect();
        let entries: Vec<(&str, &Path)> = names
            .iter()
            .map(String::as_str)
            .zip(paths.iter().copied())
            .collect();
        let registry = crate::test_support::kiln_registry(Path::new(REGISTRY_DATA_ROOT), &entries);
        for (name, path) in &entries {
            assert!(
                registry
                    .resolve(&KilnName::parse(name).unwrap())
                    .path()
                    .is_some(),
                "precondition: {} must register as a kiln, or this test proves nothing \
                 about containment",
                path.display()
            );
        }
        let session = Session::new(
            SessionType::Chat,
            names
                .iter()
                .map(|n| KilnName::parse(n).unwrap())
                .collect::<Vec<_>>(),
        );
        (session, registry)
    }

    /// Ask the containment set through the same door the tools use. Reading
    /// the root vectors directly would pass whether or not anything honors
    /// them; `FsScope` is what every tool family actually holds.
    fn reaches(roots: &RootSet, path: &Path) -> bool {
        FsScope::workspace(PathBuf::new(), roots.clone())
            .resolve(&path.to_string_lossy())
            .is_ok()
    }

    /// The same, through the door `write_file` and `create_note` use.
    fn can_write(roots: &RootSet, path: &Path) -> bool {
        FsScope::workspace(PathBuf::new(), roots.clone())
            .resolve_for_write(&path.to_string_lossy())
            .is_ok()
    }

    /// A tools-only agent is a legitimate shape, and it must come out of this
    /// with LESS reach, not unlimited reach. Its own storage dir is the whole
    /// allowlist — no `""` from the absent kiln, no `""` from the absent
    /// workspace, and no root that would out-rank the sessions-root denial.
    #[test]
    fn a_kilnless_session_reaches_only_its_own_storage_directory() {
        let sessions_root = Path::new("/data/sessions");
        let (session, registry) = session_over(&[]);
        let own = sessions_root.join(&*session.id);

        let roots = session_containment(&session, sessions_root, &registry);

        assert!(reaches(&roots, &own.join("session.jsonl")));
        assert!(!reaches(
            &roots,
            Path::new("/data/sessions/chat-other/session.jsonl")
        ));
        assert!(!reaches(&roots, Path::new("/etc/shadow")));

        // And the same for a set that holds a name NOTHING resolves. This is
        // where the empty path used to come in — a pre-flatten `meta.json` with
        // `"kiln": ""` — and it is now the general shape: an unresolvable name
        // must contribute no root at all, not a root spelled `""` that
        // `Path::starts_with` reads as every path.
        let mut unresolvable = Session::new(SessionType::Chat, vec![]);
        unresolvable
            .kilns
            .push(KilnName::parse("no-such-kiln").unwrap());
        assert_eq!(
            registry.resolve(&KilnName::parse("no-such-kiln").unwrap()),
            crate::kiln_registry::KilnResolution::Unknown,
            "precondition: the name really must be unresolvable"
        );
        let roots = session_containment(&unresolvable, sessions_root, &registry);

        assert!(reaches(
            &roots,
            &sessions_root.join(&*unresolvable.id).join("session.jsonl")
        ));
        // Every directory a fallback could plausibly have reached for, probed
        // by name. Asserting only on `/etc/shadow` is not enough: a resolution
        // that answered with the DATA ROOT would leave that probe failing and
        // the widening invisible, which is how this test passed against a
        // deliberately broken `paths_for`.
        let cwd = std::env::current_dir().expect("a working directory");
        for permitted in [
            PathBuf::from(REGISTRY_DATA_ROOT).join("anything"),
            PathBuf::from(REGISTRY_DATA_ROOT),
            sessions_root.join("chat-other").join("session.jsonl"),
            cwd.join("Cargo.toml"),
            PathBuf::from("/etc/shadow"),
        ] {
            assert!(
                !reaches(&roots, &permitted),
                "an unresolvable name must contribute no root at all, but {} is reachable",
                permitted.display()
            );
        }
    }

    /// An absent workspace contributes NO root — not the empty path.
    ///
    /// This is the empty-set-permits shape on the workspace axis, and the
    /// permit it buys is specific: `ResolvedPath::resolve("")` anchors a
    /// relative path at the process working directory, so a scope built with
    /// `unwrap_or_default()` hands every workspace-less session read access to
    /// whatever directory `cru daemon serve` was started in — a developer's
    /// checkout, most of the time. `Option<PathBuf>` is what makes that
    /// unspellable here; `RootSet`'s own empty-root filter is the second line,
    /// not the first.
    #[test]
    fn a_session_without_a_workspace_does_not_reach_the_daemons_working_directory() {
        let cwd = std::env::current_dir().expect("a working directory");
        let probe = cwd.join("Cargo.toml");
        // Preconditions, or this passes because the probe was unreachable for
        // reasons that have nothing to do with the workspace.
        assert!(probe.is_file(), "{} must exist", probe.display());
        let sessions_root = Path::new("/data/sessions");
        let kiln = PathBuf::from("/kilns/a");
        assert!(
            !cwd.starts_with(&kiln),
            "the probe must not sit inside the session's real scope"
        );

        let (session, registry) = session_over(&[&kiln]);
        assert_eq!(session.workspace, None, "precondition: no workspace");

        let roots = session_containment(&session, sessions_root, &registry);

        assert!(
            reaches(&roots, &kiln.join("Note.md")),
            "the kiln it does have stays in scope"
        );
        assert!(
            !reaches(&roots, &probe),
            "an absent workspace must not grant the daemon's own working directory"
        );
    }

    /// Migration is best-effort — kilns that appear mid-run are never scanned
    /// and `migrate_one` skips on failure — so the pre-relocation transcript
    /// directory inside each kiln has to be denied, not assumed empty. The
    /// workspace gets the same treatment: a project directory that used to be
    /// someone's kiln still holds one, and it is a scope root like any other.
    #[test]
    fn every_scope_roots_legacy_transcript_directory_is_denied() {
        let sessions_root = Path::new("/data/sessions");
        let (mut session, registry) = session_over(&[Path::new("/kilns/a"), Path::new("/kilns/b")]);
        session.set_workspace(Some(PathBuf::from("/repo")));

        let roots = session_containment(&session, sessions_root, &registry);

        for root in ["/kilns/a", "/kilns/b", "/repo"] {
            let root = Path::new(root);
            assert!(
                reaches(&roots, &root.join("Note.md")),
                "{} must stay readable",
                root.display()
            );
            assert!(
                !reaches(
                    &roots,
                    &root
                        .join(".crucible")
                        .join("sessions")
                        .join("chat-old")
                        .join("session.jsonl")
                ),
                "{}'s legacy in-kiln transcripts must be denied",
                root.display()
            );
        }
    }

    /// A project filed BENEATH a kiln or workspace has its own
    /// `.crucible/sessions` and is not itself a scope root, so the per-root
    /// literal never named it and the enclosing root's blanket permit admitted
    /// it. Same confused-deputy shape as the rest of this design: an item
    /// inheriting its container's answer.
    #[test]
    fn a_nested_projects_transcript_directory_is_denied_though_it_is_not_a_scope_root() {
        let sessions_root = Path::new("/data/sessions");
        let (mut session, registry) = session_over(&[Path::new("/kilns/a")]);
        session.set_workspace(Some(PathBuf::from("/repo")));

        let roots = session_containment(&session, sessions_root, &registry);

        for nested in [
            "/kilns/a/projects/inner/.crucible/sessions/chat-old/session.jsonl",
            "/repo/vendor/dep/.crucible/sessions/chat-old/session.jsonl",
            "/repo/a/b/c/d/.crucible/sessions/chat-old/meta.json",
        ] {
            assert!(
                !reaches(&roots, Path::new(nested)),
                "{nested} is a transcript directory wherever it sits"
            );
        }
        assert!(
            reaches(&roots, Path::new("/kilns/a/projects/inner/Note.md")),
            "and a nested project is otherwise ordinary content"
        );
        assert!(
            reaches(&roots, Path::new("/repo/vendor/dep/.crucible/kiln.toml")),
            "the rule is `.crucible/sessions`, not the whole control directory — \
             the kiln-anchored tools have their own broader rule for that"
        );
    }

    /// The transcript threat model is tampering, not reading.
    ///
    /// A transcript is replayed into a future context, so an agent that can
    /// edit one can plant an instruction its successor obeys. Anthropic blocks
    /// writes to `~/.claude/projects/**.jsonl` for this reason and states
    /// plainly that reading is not blocked; we had the read denial and no
    /// write protection at all. The sessions root is now write-denied to every
    /// session including the one whose own directory is inside it — the read
    /// carve-out exists so a session can pick up its own spilled tool output,
    /// and nothing needs to write there through a tool.
    #[test]
    fn no_session_can_write_anywhere_under_the_sessions_root() {
        let sessions_root = Path::new("/data/sessions");
        let (mut session, registry) = session_over(&[Path::new("/data")]);
        session.set_workspace(Some(PathBuf::from("/repo")));
        let own = session.storage_path(sessions_root);

        let roots = session_containment(&session, sessions_root, &registry);

        assert!(
            reaches(&roots, &own.join("tools").join("bash-1.txt")),
            "a session must still read its own spilled tool output"
        );
        for target in [
            own.join("session.jsonl"),
            own.join("meta.json"),
            own.join("tools").join("bash-1.txt"),
            sessions_root.join("chat-other").join("session.jsonl"),
            sessions_root.join("anything.txt"),
        ] {
            assert!(
                !can_write(&roots, &target),
                "{} must not be writable",
                target.display()
            );
        }
        assert!(
            can_write(&roots, Path::new("/repo/src/main.rs")),
            "the workspace stays writable"
        );
    }

    /// The same carve-out, against the sessions root production actually uses.
    ///
    /// `data_home` is `~/.crucible`, so the real root is
    /// `~/.crucible/sessions` — which matches the legacy in-kiln transcript
    /// SHAPE component for component. A shape rule that answers ahead of the
    /// ranking therefore denies a session its own spilled tool output on every
    /// real installation, while every test above passes because a synthetic
    /// `/data/sessions` root has no `.crucible` in it. The fixture is the
    /// whole test: the rule and the carve-out only collide at the real path.
    #[test]
    fn a_session_reads_its_own_output_under_the_sessions_root_production_uses() {
        let sessions_root = Path::new("/home/u/.crucible/sessions");
        let (mut session, registry) = session_over(&[Path::new("/kilns/a")]);
        session.set_workspace(Some(PathBuf::from("/repo")));
        let own = session.storage_path(sessions_root);

        let roots = session_containment(&session, sessions_root, &registry);

        assert!(
            reaches(&roots, &own.join("tools").join("bash-1.txt")),
            "the carve-out is why the sessions root is a read denial with an \
             exception rather than a wall; the shape rule must not out-rank it"
        );
        // The denial the carve-out sits inside is unchanged.
        assert!(
            !reaches(
                &roots,
                &sessions_root.join("chat-other").join("session.jsonl")
            ),
            "another session's transcript stays unreachable"
        );
        assert!(
            !can_write(&roots, &own.join("session.jsonl")),
            "the carve-out is a READ exception and nothing more"
        );
    }

    /// The legacy in-kiln transcript directory gets the same treatment, since
    /// migration is best-effort and a kiln that predates it still holds one.
    #[test]
    fn a_legacy_in_kiln_transcript_directory_is_not_writable() {
        let sessions_root = Path::new("/data/sessions");
        let (session, registry) = session_over(&[Path::new("/kilns/a")]);

        let roots = session_containment(&session, sessions_root, &registry);

        assert!(can_write(&roots, Path::new("/kilns/a/Note.md")));
        assert!(!can_write(
            &roots,
            Path::new("/kilns/a/.crucible/sessions/chat-old/session.jsonl")
        ));
    }

    /// The install-dependent half of the protected set has to be WIRED, not
    /// merely to exist. A developer whose workspace is the Crucible checkout
    /// has `runtime/plugins/*/init.lua` — Lua the daemon executes on its next
    /// start — inside an allowed root, and that is the ordinary case here
    /// rather than a contrived one.
    ///
    /// Driven from `daemon_roots()` itself rather than a fixed path, because
    /// where the runtime tree lives depends on how Crucible was installed;
    /// what is asserted is that whatever it answers reaches the session.
    #[test]
    fn the_runtime_tree_the_daemon_executes_is_write_denied_but_readable() {
        let sessions_root = Path::new("/data/sessions");
        let roots_of_the_daemon = crate::tools::protected::daemon_roots();
        let runtime = roots_of_the_daemon
            .first()
            .expect("the daemon always names at least one runtime root")
            .clone();
        let (session, registry) = session_over(&[&runtime]);

        let roots = session_containment(&session, sessions_root, &registry);
        let plugin = runtime.join("plugins").join("oci").join("init.lua");

        assert!(
            reaches(&roots, &plugin),
            "a plugin's source must stay readable — this is not a denial"
        );
        assert!(
            !can_write(&roots, &plugin),
            "but writing one is code execution in the daemon on its next start"
        );
    }

    /// A tree named on the config `runtimepath` is a tree the daemon loads and
    /// EXECUTES Lua from, exactly like the shipped runtime — and
    /// `docs/Help/Extending/Creating Plugins.md` documents putting a KILN on it
    /// (`runtimepath = ["~/kilns/work"]   # loads ~/kilns/work/plugins/`).
    /// A kiln is a session scope root, so the tree is inside the allowlist and
    /// writable: an agent with nothing but `write_file` plants
    /// `<kiln>/plugins/evil/init.lua` (or `<kiln>/defaults/init.lua`) and the
    /// daemon runs it with host privileges on its next start. That is
    /// CVE-2026-25725's shape, and neither path exists beforehand.
    #[test]
    fn a_runtimepath_tree_the_daemon_executes_is_write_denied_but_readable() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_root = tmp.path().join("sessions");
        let kiln = tmp.path().join("kilns").join("work");
        std::fs::create_dir_all(kiln.join("plugins")).unwrap();

        // Precondition: the daemon really does load plugins from this tree.
        let searched = crate::daemon_plugins::daemon_plugin_paths(std::slice::from_ref(&kiln));
        assert!(
            searched.iter().any(|(dir, _)| dir == &kiln.join("plugins")),
            "precondition: the daemon searches <runtimepath>/plugins: {searched:?}"
        );
        // And executes `<runtimepath>/defaults/init.lua` on every session VM.
        assert!(
            crate::runtime_defaults::defaults_candidates(std::slice::from_ref(&kiln), None)
                .contains(&kiln.join("defaults").join("init.lua")),
            "precondition: the session VM runs <runtimepath>/defaults/init.lua"
        );

        let (session, registry) = session_over(&[&kiln]);
        let roots = session_containment(&session, &sessions_root, &registry);

        let plugin = kiln.join("plugins").join("evil").join("init.lua");
        let defaults = kiln.join("defaults").join("init.lua");
        assert!(
            reaches(&roots, &plugin),
            "a plugin's source must stay readable — this is not a denial"
        );
        for lua in [&plugin, &defaults] {
            assert!(!lua.exists(), "{} must not exist yet", lua.display());
            assert!(
                !can_write(&roots, lua),
                "{} is Lua the daemon executes on its next start",
                lua.display()
            );
        }
        assert!(
            can_write(&roots, &kiln.join("Note.md")),
            "protecting the plugin tree must not cost the kiln's notes"
        );
    }

    /// The out-ranking escape, end to end at the layer that builds the roots: a
    /// kiln naming another session's storage directory does not become an
    /// allowed root just because it is deeper than the denial.
    #[test]
    fn a_kiln_inside_the_sessions_root_does_not_become_an_allowed_root() {
        let sessions_root = Path::new("/data/sessions");
        let victim = sessions_root.join("chat-victim");
        // Registered against a data root that is NOT `/data`, so the
        // registration floor lets it through — production refuses this at
        // registration, and what is under test is that containment refuses it
        // again if anything ever reaches here by another door.
        let (session, registry) = session_over(&[&victim]);

        let roots = session_containment(&session, sessions_root, &registry);

        assert!(!reaches(&roots, &victim.join("session.jsonl")));
        assert!(
            reaches(
                &roots,
                &sessions_root.join(&*session.id).join("session.jsonl")
            ),
            "the session's own storage stays reachable"
        );
    }
}
