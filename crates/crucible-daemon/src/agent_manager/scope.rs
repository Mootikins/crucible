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
use crate::tools::containment::RootSet;
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
pub(crate) fn session_containment(session: &Session, sessions_root: &Path) -> RootSet {
    let scope: Vec<PathBuf> = session
        .kilns
        .iter()
        .cloned()
        .chain(std::iter::once(session.workspace.clone()))
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
        kiln: &Path,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        let kiln = kiln.to_path_buf();
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
        kiln: &Path,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<Session, AgentError> {
        self.mutate_scope(session_id, event_tx, |session| {
            let before = session.kilns.len();
            session.kilns.retain(|k| k != kiln);
            Ok(session.kilns.len() != before)
        })
        .await
    }

    /// Set or clear the session's workspace. `None` detaches: the workspace
    /// falls back to the kiln path (the same state a workspace-less create
    /// produces — see `Session::new`). Rejected for ACP sessions, whose
    /// external agent process runs in the workspace it was spawned with.
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
            let new_workspace = workspace.unwrap_or_else(|| {
                session
                    .default_kiln()
                    .map(Path::to_path_buf)
                    .unwrap_or_default()
            });
            if session.workspace == new_workspace {
                return Ok(false);
            }
            session.workspace = new_workspace;
            Ok(true)
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
    use crate::tools::containment::RootSet;
    use crate::tools::fs_scope::FsScope;
    use crucible_core::session::SessionType;
    use crucible_core::Session;
    use std::path::{Path, PathBuf};

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
        let session = Session::new(SessionType::Chat, vec![]);
        let own = sessions_root.join(&*session.id);

        let roots = session_containment(&session, sessions_root);

        assert!(reaches(&roots, &own.join("session.jsonl")));
        assert!(!reaches(
            &roots,
            Path::new("/data/sessions/chat-other/session.jsonl")
        ));
        assert!(!reaches(&roots, Path::new("/etc/shadow")));

        // And the same for a set that holds an empty path rather than no path
        // — what a pre-flatten `meta.json` with `"kiln": ""` deserializes to.
        let mut legacy = Session::new(SessionType::Chat, vec![]);
        legacy.kilns.push(PathBuf::new());
        let roots = session_containment(&legacy, sessions_root);

        assert!(reaches(
            &roots,
            &sessions_root.join(&*legacy.id).join("session.jsonl")
        ));
        assert!(!reaches(&roots, Path::new("/etc/shadow")));
    }

    /// Migration is best-effort — kilns that appear mid-run are never scanned
    /// and `migrate_one` skips on failure — so the pre-relocation transcript
    /// directory inside each kiln has to be denied, not assumed empty. The
    /// workspace gets the same treatment: a project directory that used to be
    /// someone's kiln still holds one, and it is a scope root like any other.
    #[test]
    fn every_scope_roots_legacy_transcript_directory_is_denied() {
        let sessions_root = Path::new("/data/sessions");
        let mut session = Session::new(
            SessionType::Chat,
            vec![PathBuf::from("/kilns/a"), PathBuf::from("/kilns/b")],
        );
        session.workspace = PathBuf::from("/repo");

        let roots = session_containment(&session, sessions_root);

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
        let mut session = Session::new(SessionType::Chat, vec![PathBuf::from("/kilns/a")]);
        session.workspace = PathBuf::from("/repo");

        let roots = session_containment(&session, sessions_root);

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
        let mut session = Session::new(SessionType::Chat, vec![PathBuf::from("/data")]);
        session.workspace = PathBuf::from("/repo");
        let own = session.storage_path(sessions_root);

        let roots = session_containment(&session, sessions_root);

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
        let mut session = Session::new(SessionType::Chat, vec![PathBuf::from("/kilns/a")]);
        session.workspace = PathBuf::from("/repo");
        let own = session.storage_path(sessions_root);

        let roots = session_containment(&session, sessions_root);

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
        let session = Session::new(SessionType::Chat, vec![PathBuf::from("/kilns/a")]);

        let roots = session_containment(&session, sessions_root);

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
        let session = Session::new(SessionType::Chat, vec![runtime.clone()]);

        let roots = session_containment(&session, sessions_root);
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

        let session = Session::new(SessionType::Chat, vec![kiln.clone()]);
        let roots = session_containment(&session, &sessions_root);

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
        let session = Session::new(SessionType::Chat, vec![victim.clone()]);

        let roots = session_containment(&session, sessions_root);

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
