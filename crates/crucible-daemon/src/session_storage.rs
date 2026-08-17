//! Session persistence to the daemon-owned sessions root.
//!
//! Sessions are stored one directory per id at `{sessions_root}/{session_id}/`,
//! where `sessions_root` is `{data_home}/sessions` — resolved once at daemon
//! bind and threaded in as a value, never read from the process-global
//! `crucible_home()`.
//!
//! Contents:
//! - `meta.json` - Session metadata
//! - `session.jsonl` - Event log (append-only)
//! - `session.md` - Human-readable markdown conversation log
//!
//! Sessions used to live inside their owning kiln
//! (`{kiln}/.crucible/sessions/{id}`), which welded a filing decision to a
//! knowledge decision and shipped every conversation along with a shared kiln.
//! [`crate::session_migration`] relocates those on daemon start.

use crate::kiln_registry::{KilnRegistry, KilnRegistryContext};
use crate::session_manager::SessionError;
use async_trait::async_trait;
use chrono::Utc;
use crucible_core::config::KilnName;
use crucible_core::session::{Session, SessionId, SessionSummary};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Trait for session persistence.
///
/// Implementations provide different storage backends for persisting
/// sessions to disk or other storage systems.
#[async_trait]
pub trait SessionStorage: Send + Sync {
    /// Root directory holding one subdirectory per session.
    ///
    /// On the trait rather than on the file backend because callers that need
    /// a session's directory for something this trait does not do (workflow
    /// snapshots, review journals, recording files) used to reach around it to
    /// a `FileSessionStorage` associated function, which is what let the layout
    /// be decided in half a dozen places at once.
    fn sessions_root(&self) -> &Path;

    /// Save a session to storage.
    ///
    /// Creates the session directory if needed and writes session metadata.
    async fn save(&self, session: &Session) -> Result<(), SessionError>;

    /// Load a session from storage.
    ///
    /// Returns `SessionError::NotFound` if the session doesn't exist.
    async fn load(&self, session_id: &SessionId) -> Result<Session, SessionError>;

    /// List every persisted session.
    ///
    /// Returns an empty vec if the sessions root doesn't exist.
    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError>;

    /// Append an event to the session's JSONL log.
    ///
    /// Events are appended as single lines to enable streaming reads.
    async fn append_event(&self, session: &Session, event: &str) -> Result<(), SessionError>;

    /// Append a human-readable entry to the session's markdown log.
    ///
    /// Creates the markdown file with frontmatter on first call.
    /// Subsequent calls append timestamped entries.
    async fn append_markdown(
        &self,
        session: &Session,
        role: &str,
        content: &str,
    ) -> Result<(), SessionError>;

    /// Load events from the session's JSONL log with pagination.
    ///
    /// Returns events in chronological order (oldest first).
    /// Use `offset` to skip events and `limit` to cap the number returned.
    async fn load_events(
        &self,
        session_id: &SessionId,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError>;

    /// Count total events in the session's JSONL log.
    async fn count_events(&self, session_id: &SessionId) -> Result<usize, SessionError>;
}

/// File-based session storage rooted at a single directory.
#[derive(Debug, Clone)]
pub struct FileSessionStorage {
    sessions_root: PathBuf,
    /// The name↔path mapping for kilns.
    ///
    /// This layer, and only this layer, holds one — which is why it is the only
    /// layer that may turn a persisted path into a [`KilnName`]. The empty
    /// default is the fail-closed one: a storage built without a registry
    /// resolves no persisted kiln, so a session loaded through it reaches
    /// nothing it cannot justify.
    registry: Arc<KilnRegistry>,
}

impl FileSessionStorage {
    /// Create file-based storage rooted at `sessions_root`, resolving kilns
    /// against an empty registry.
    ///
    /// Use [`FileSessionStorage::root_for`] to derive it from the daemon's
    /// data root, and [`FileSessionStorage::with_registry`] to give it the
    /// daemon's real registry.
    pub fn new(sessions_root: PathBuf) -> Self {
        let registry = KilnRegistry::empty(KilnRegistryContext::new(
            sessions_root.clone(),
            None,
            sessions_root.clone(),
        ));
        Self {
            sessions_root,
            registry: Arc::new(registry),
        }
    }

    /// Resolve persisted kiln paths against `registry`.
    #[must_use]
    pub fn with_registry(mut self, registry: Arc<KilnRegistry>) -> Self {
        self.registry = registry;
        self
    }

    /// The registry this storage resolves persisted kilns against.
    pub fn kiln_registry(&self) -> &Arc<KilnRegistry> {
        &self.registry
    }

    /// Turn the path-shaped kiln set read off disk into the session's names.
    ///
    /// **Lookup only.** A persisted path that no registry entry claims must not
    /// register itself: auto-registration is a live, first-party request on
    /// this machine, and a `meta.json` is neither — it can be hand-edited, and
    /// it is the one door through which a path the floor never saw could mint a
    /// kiln. Such a path contributes no kiln (no root, no classification, no
    /// search source, no prompt line) and is carried forward untouched so that
    /// re-registering the entry restores it.
    fn resolve_persisted_kilns(&self, session: &mut Session) {
        let mut names: Vec<KilnName> = Vec::new();
        let mut unresolved: Vec<PathBuf> = Vec::new();
        for path in session.take_persisted_kiln_paths() {
            match self.registry.name_for(&path) {
                Some(name) if names.contains(name) => {}
                Some(name) => names.push(name.clone()),
                None => {
                    tracing::warn!(
                        session_id = %session.id,
                        kiln = %path.display(),
                        "Persisted kiln matches no registry entry; it is not a kiln for this session"
                    );
                    unresolved.push(path);
                }
            }
        }
        session.kilns = names;
        session.set_unresolved_kiln_paths(unresolved);
    }

    /// The path-shaped kiln set to write back out: every resolvable name's
    /// registered path, followed by the paths that resolved to nothing.
    ///
    /// The unresolved tail is what keeps a save from *erasing* a session's
    /// scope when the registry is empty — a config that failed to parse would
    /// otherwise silently delete the kilns of every session the daemon touches.
    fn persist_view(&self, session: &Session) -> Session {
        let mut paths: Vec<PathBuf> = Vec::new();
        for name in &session.kilns {
            if let Some(kiln) = self.registry.resolve(name).registered() {
                if !paths.contains(&kiln.path().to_path_buf()) {
                    paths.push(kiln.path().to_path_buf());
                }
            }
        }
        for path in session.unresolved_kiln_paths() {
            if !paths.contains(path) {
                paths.push(path.clone());
            }
        }
        let mut view = session.clone();
        view.set_persisted_kiln_paths(paths);
        view
    }

    /// The sessions root for a daemon data root: `{data_home}/sessions`.
    ///
    /// The single place the layout is spelled out, so relocating it again is a
    /// one-line change rather than a grep.
    pub fn root_for(data_home: &Path) -> PathBuf {
        data_home.join("sessions")
    }

    /// Storage directory for a session id.
    ///
    /// Takes the validated id rather than a `&str`: the join is what turns an
    /// identifier into filesystem reach, and [`SessionId`] is the only type
    /// that has been through the check.
    fn session_dir(&self, session_id: &SessionId) -> PathBuf {
        session_id.dir_under(&self.sessions_root)
    }

    /// Re-run the scope floor over a workspace that arrived from disk.
    ///
    /// `workspace` is a plain path deserialized unchecked, and it is chained
    /// straight into the session's containment allowlist alongside its kilns
    /// (`agent_manager::scope::session_containment`). Every *live* door that
    /// sets one — `session.create`, `session.set_workspace` — runs
    /// [`refuse_forbidden_scope`](crate::kiln_registry::refuse_forbidden_scope)
    /// first, but a `meta.json` reaches that
    /// allowlist without passing any of them: a file written before the gate
    /// existed, a file hand-edited, or a file whose target became forbidden
    /// afterwards (a symlink repointed at `/`, a directory that is now inside
    /// the sessions root). Reviving is therefore the same door, and it must
    /// ask the same question.
    ///
    /// A refused workspace is dropped rather than the load being failed: the
    /// transcript is still the user's, and refusing to open it would turn a
    /// bad path into lost history. The session is then anchored at its own
    /// storage directory by `scope::session_tool_root` — the one directory it
    /// certainly has, already read-only inside its own containment. Cleared
    /// rather than *set* to that directory, because a sessions-root path in
    /// the field itself would land in the review roots, in the Lua
    /// `session.workspace` a plugin reads, and in the workspace chip the web
    /// UI renders.
    fn refuse_persisted_workspace(&self, session: &mut Session) {
        let Some(workspace) = session.workspace.as_deref() else {
            return;
        };
        if let Err(reason) = crate::kiln_registry::refuse_forbidden_scope(
            "workspace",
            workspace,
            &self.sessions_root,
        ) {
            tracing::warn!(
                session_id = %session.id,
                reason = %reason,
                "Persisted workspace refused on load; the session is anchored at its own storage directory"
            );
            session.set_workspace(None);
        }
    }
}

#[async_trait]
impl SessionStorage for FileSessionStorage {
    fn sessions_root(&self) -> &Path {
        &self.sessions_root
    }

    async fn save(&self, session: &Session) -> Result<(), SessionError> {
        let dir = self.session_dir(&session.id);
        fs::create_dir_all(&dir).await?;

        // Save session metadata as JSON. The kiln set goes out path-shaped —
        // see `persist_view` and `PersistedKilns`.
        let meta_path = dir.join("meta.json");
        let json = serde_json::to_string_pretty(&self.persist_view(session))?;
        fs::write(&meta_path, json).await?;

        Ok(())
    }

    async fn load(&self, session_id: &SessionId) -> Result<Session, SessionError> {
        let dir = self.session_dir(session_id);
        // Try meta.json first, fall back to legacy session.json for backward compatibility
        let meta_path = dir.join("meta.json");
        let legacy_path = dir.join("session.json");
        let path = if meta_path.exists() {
            meta_path
        } else {
            legacy_path
        };

        let json = fs::read_to_string(&path).await.map_err(|e| {
            // Distinguish between "not found" and other IO errors
            if e.kind() == std::io::ErrorKind::NotFound {
                SessionError::NotFound(session_id.to_string())
            } else {
                SessionError::IoError(format!(
                    "Failed to load session '{}' from {}: {}",
                    session_id,
                    path.display(),
                    e
                ))
            }
        })?;

        let raw: serde_json::Value = serde_json::from_str(&json).map_err(|e| {
            SessionError::IoError(format!(
                "Failed to parse session '{}' JSON: {}",
                session_id, e
            ))
        })?;
        // Read before the value is consumed: these are the pre-flatten
        // spellings, and their presence is what makes the rewrite below worth
        // a write.
        let pre_flatten = raw
            .as_object()
            .is_some_and(|o| o.contains_key("kiln") || o.contains_key("connected_kilns"));
        let mut session: Session = serde_json::from_value(raw).map_err(|e| {
            SessionError::IoError(format!(
                "Failed to parse session '{}' JSON: {}",
                session_id, e
            ))
        })?;

        self.refuse_persisted_workspace(&mut session);
        self.resolve_persisted_kilns(&mut session);

        // Same rule `list` applies, at the other door. A load is keyed on the
        // directory; every subsequent write — `save`, `append_event`,
        // `append_markdown` — is keyed on `session.id`. Handing back a session
        // whose id names a *different* directory therefore turns reading one
        // session into writing another, which is the whole of the attack
        // migration's stamp closes on the way in. Enforced here as well because
        // migration is not the only way a `meta.json` can come to disagree with
        // the directory holding it, and this is the sink rather than one door.
        if session.id != *session_id {
            return Err(SessionError::IoError(format!(
                "Session '{}' holds metadata naming '{}'; refusing to load a session that names another session's directory",
                session_id, session.id
            )));
        }

        // Rewritten in place, after the id check and never before it: writing
        // first would let a `meta.json` naming another session's directory
        // provoke a write into that directory. Only the pre-flatten spellings
        // earn the write — everything else is already in the shape `save`
        // produces, so an ordinary load stays read-only.
        if pre_flatten {
            if let Err(e) = self.save(&session).await {
                tracing::warn!(
                    session_id = %session.id,
                    error = %e,
                    "Could not rewrite a pre-flatten meta.json; it will be migrated again on the next load"
                );
            }
        }

        Ok(session)
    }

    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        if !self.sessions_root.exists() {
            return Ok(vec![]);
        }

        let mut summaries = vec![];
        let mut entries = fs::read_dir(&self.sessions_root).await?;
        // A session that does not make it into the listing is unresumable, and
        // both of the ways that can happen used to be a bare `continue`: the
        // whole backlog could empty out with nothing in the log to say so. The
        // count is what distinguishes "you have no sessions" from "every
        // session failed to load".
        let mut unreadable = 0usize;
        let mut not_a_session = 0usize;

        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            // Anything the daemon did not file is not a session: migration's
            // `.migrating` staging directory, an editor's dotfile, a name that
            // is not valid UTF-8.
            let Some(session_id) = entry
                .file_name()
                .to_str()
                .and_then(|name| SessionId::parse(name).ok())
            else {
                not_a_session += 1;
                continue;
            };
            let session = match self.load(&session_id).await {
                Ok(session) => session,
                Err(e) => {
                    unreadable += 1;
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Session left out of the listing because it could not be loaded"
                    );
                    continue;
                }
            };
            // The directory name IS the id. A `meta.json` claiming a different
            // one is either corrupt or planted, and serving it would hand every
            // caller downstream — `session.cleanup` most of all — a path that
            // points at a directory other than the one this summary describes.
            if session.id != session_id {
                unreadable += 1;
                tracing::warn!(
                    directory = %session_id,
                    persisted_id = %session.id,
                    "Skipping a session whose meta.json names a different id than its directory"
                );
                continue;
            }
            summaries.push(SessionSummary::from(&session));
        }

        if unreadable > 0 {
            tracing::warn!(
                unreadable,
                listed = summaries.len(),
                "Sessions were left out of the listing; see the warnings above for each"
            );
        }
        if not_a_session > 0 {
            tracing::debug!(
                not_a_session,
                "Directories under the sessions root that are not sessions"
            );
        }

        Ok(summaries)
    }

    async fn append_event(&self, session: &Session, event: &str) -> Result<(), SessionError> {
        let dir = self.session_dir(&session.id);

        // Ensure directory exists
        fs::create_dir_all(&dir).await?;

        let jsonl_path = dir.join("session.jsonl");

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&jsonl_path)
            .await?;

        file.write_all(event.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        Ok(())
    }

    async fn append_markdown(
        &self,
        session: &Session,
        role: &str,
        content: &str,
    ) -> Result<(), SessionError> {
        let dir = self.session_dir(&session.id);

        // Ensure directory exists
        fs::create_dir_all(&dir).await?;

        let md_path = dir.join("session.md");

        // Create file with frontmatter if it doesn't exist
        if !md_path.exists() {
            let session_type_name = match session.session_type {
                crucible_core::session::SessionType::Chat => "Chat",
                crucible_core::session::SessionType::Agent => "Agent",
                crucible_core::session::SessionType::Workflow => "Workflow",
            };

            let kilns = session
                .kilns
                .iter()
                .map(KilnName::to_string)
                .collect::<Vec<_>>()
                .join(", ");

            let frontmatter = format!(
                "---\nsession_id: {}\ntype: {}\nkilns: [{}]\nworkspace: {}\nstarted: {}\n---\n\n# {} Session\n\n",
                session.id,
                session.session_type.as_prefix(),
                kilns,
                session.workspace.as_deref().unwrap_or(Path::new("")).display(),
                session.started_at.to_rfc3339(),
                session_type_name,
            );
            fs::write(&md_path, frontmatter).await?;
        }

        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let entry = format!("\n## {} - {}\n\n{}\n", role, timestamp, content);

        let mut file = fs::OpenOptions::new().append(true).open(&md_path).await?;

        file.write_all(entry.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    async fn load_events(
        &self,
        session_id: &SessionId,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        let jsonl_path = self.session_dir(session_id).join("session.jsonl");

        if !jsonl_path.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&jsonl_path).await?;

        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);

        let events: Vec<serde_json::Value> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .skip(offset)
            .take(limit)
            .filter_map(|line| match serde_json::from_str(line) {
                Ok(val) => Some(val),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        line_preview = %line.chars().take(100).collect::<String>(),
                        "Failed to parse session event, skipping"
                    );
                    None
                }
            })
            .collect();

        Ok(events)
    }

    async fn count_events(&self, session_id: &SessionId) -> Result<usize, SessionError> {
        let jsonl_path = self.session_dir(session_id).join("session.jsonl");

        if !jsonl_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&jsonl_path).await?;

        let count = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::session::SessionType;
    use tempfile::TempDir;

    /// A storage rooted in a fresh TempDir, plus the kiln path test sessions
    /// are given. The two are deliberately unrelated: storage location no
    /// longer follows from the kiln.
    fn storage_in(tmp: &TempDir) -> FileSessionStorage {
        // The registry is what turns a session's kiln NAMES into the paths
        // `meta.json` records and back again. A storage without one persists no
        // kilns at all, so a round-trip assertion over it would pass on an
        // empty set.
        let kilns: Vec<(&str, PathBuf)> = ["kiln", "kiln-a", "kiln-b", "other-kiln"]
            .into_iter()
            .map(|name| (name, tmp.path().join("kilns").join(name)))
            .collect();
        let borrowed: Vec<(&str, &Path)> = kilns
            .iter()
            .map(|(name, path)| (*name, path.as_path()))
            .collect();
        FileSessionStorage::new(FileSessionStorage::root_for(tmp.path()))
            .with_registry(crate::test_support::kiln_registry(tmp.path(), &borrowed))
    }

    fn session_in(tmp: &TempDir, session_type: SessionType) -> Session {
        let _ = tmp;
        Session::new(session_type, vec![crate::test_support::kiln_name("kiln")])
    }

    #[tokio::test]
    async fn test_session_storage_save_load() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        let session_id = session.id.clone();

        storage.save(&session).await.unwrap();

        let loaded = storage.load(&session_id).await.unwrap();
        assert_eq!(loaded.id, session_id);
        assert_eq!(loaded.session_type, SessionType::Chat);
    }

    /// A load is keyed on the directory and every write that follows is keyed
    /// on `session.id`, so a `meta.json` naming another session turns reading
    /// one directory into writing another. Migration stamps the id on the way
    /// in; this is the same rule at the sink, for metadata that came to
    /// disagree some other way.
    #[tokio::test]
    async fn loading_a_directory_whose_metadata_names_another_session_fails() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let victim = session_in(&tmp, SessionType::Chat);
        storage.save(&victim).await.unwrap();

        // A directory of its own, whose metadata claims to be the victim.
        let evil_id = SessionId::parse("chat-evil").unwrap();
        let mut evil = session_in(&tmp, SessionType::Chat);
        evil.id = victim.id.clone();
        let dir = evil_id.dir_under(&FileSessionStorage::root_for(tmp.path()));
        fs::create_dir_all(&dir).await.unwrap();
        fs::write(
            dir.join("meta.json"),
            serde_json::to_string_pretty(&evil).unwrap(),
        )
        .await
        .unwrap();

        assert!(
            storage.load(&evil_id).await.is_err(),
            "loading one session's directory handed back another session's id"
        );
    }

    #[tokio::test]
    async fn sessions_land_under_the_injected_root_not_the_kiln() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        let meta = tmp
            .path()
            .join("sessions")
            .join(&*session.id)
            .join("meta.json");
        assert!(meta.exists(), "meta.json should be at {}", meta.display());
        assert!(
            !tmp.path().join("kiln").join(".crucible").exists(),
            "nothing may be written inside the kiln"
        );
    }

    #[tokio::test]
    async fn test_session_storage_list() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        // Two sessions in different kilns still share one storage root.
        let session1 = Session::new(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln-a")],
        );
        let session2 = Session::new(
            SessionType::Agent,
            vec![crate::test_support::kiln_name("kiln-b")],
        );

        storage.save(&session1).await.unwrap();
        storage.save(&session2).await.unwrap();

        let summaries = storage.list().await.unwrap();
        assert_eq!(summaries.len(), 2);
    }

    /// The path↔name mapping, both directions, through the layer that owns it.
    ///
    /// `Session.kilns` holds names and `meta.json` holds paths, so a save that
    /// wrote names — or a load that handed the names back untranslated — would
    /// produce a file no other reader understands and a session whose kilns
    /// resolve to nothing.
    #[tokio::test]
    async fn a_sessions_kiln_names_are_persisted_as_paths_and_resolved_back() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);
        let session = session_in(&tmp, SessionType::Chat);
        let expected = tmp.path().join("kilns").join("kiln");

        storage.save(&session).await.unwrap();

        let raw = fs::read_to_string(storage.session_dir(&session.id).join("meta.json"))
            .await
            .unwrap();
        let on_disk: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            on_disk["kilns"],
            serde_json::json!([expected.to_string_lossy()]),
            "meta.json records the PATH the name resolves to: {raw}"
        );

        let loaded = storage.load(&session.id).await.unwrap();
        assert_eq!(
            loaded.kilns,
            vec![crate::test_support::kiln_name("kiln")],
            "and the load maps it back to the name"
        );
    }

    /// A pre-flatten `meta.json` loads, and the file is rewritten in place so
    /// the next reader sees the flat shape.
    ///
    /// The rewrite is the half a "it still parses" test leaves out: the legacy
    /// keys survive every read otherwise, and `session_migration`'s stamp
    /// deliberately preserves unknown fields, so `kiln` would keep winning over
    /// whatever the daemon decided.
    #[tokio::test]
    async fn a_pre_flatten_meta_json_loads_and_is_rewritten_in_place() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);
        let session = session_in(&tmp, SessionType::Chat);
        let session_id = session.id.clone();
        storage.save(&session).await.unwrap();

        // Plant the pre-flatten spelling the way a file written before the
        // flatten carries it.
        let meta_path = storage.session_dir(&session_id).join("meta.json");
        let mut planted: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&meta_path).await.unwrap()).unwrap();
        let kiln = tmp.path().join("kilns").join("kiln");
        let other = tmp.path().join("kilns").join("other-kiln");
        planted.as_object_mut().unwrap().remove("kilns");
        planted["kiln"] = serde_json::json!(kiln.to_string_lossy());
        planted["connected_kilns"] = serde_json::json!([other.to_string_lossy()]);
        fs::write(&meta_path, serde_json::to_string_pretty(&planted).unwrap())
            .await
            .unwrap();
        // Precondition: the legacy keys really are on disk, or this passes
        // because there was never anything to migrate.
        let before = fs::read_to_string(&meta_path).await.unwrap();
        assert!(
            before.contains("connected_kilns"),
            "precondition: the planted file must carry the legacy keys: {before}"
        );

        let loaded = storage.load(&session_id).await.unwrap();
        assert_eq!(
            loaded.kilns,
            vec![
                crate::test_support::kiln_name("kiln"),
                crate::test_support::kiln_name("other-kiln")
            ],
            "primary first, connected after, both resolved to names"
        );

        let after = fs::read_to_string(&meta_path).await.unwrap();
        assert!(
            !after.contains("connected_kilns") && !after.contains("\"kiln\":"),
            "the legacy keys must be gone from the file after the load: {after}"
        );
        let rewritten: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(
            rewritten["kilns"],
            serde_json::json!([kiln.to_string_lossy(), other.to_string_lossy()]),
            "and the flat, path-shaped array is what replaced them"
        );
    }

    /// A persisted kiln path that no registry entry claims is **not a kiln**:
    /// it contributes nothing to the loaded session. Asserted on the absence
    /// from `kilns`, not on the load merely not failing.
    ///
    /// And it must not be *erased*, which is the second half: an entry the user
    /// renamed, or a config that failed to parse, would otherwise have the next
    /// save silently delete the session's scope for good.
    #[tokio::test]
    async fn an_unregistered_persisted_kiln_grants_nothing_and_is_not_erased() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);
        let session = session_in(&tmp, SessionType::Chat);
        let session_id = session.id.clone();
        storage.save(&session).await.unwrap();

        let stranger = tmp.path().join("not-registered");
        let meta_path = storage.session_dir(&session_id).join("meta.json");
        let mut planted: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&meta_path).await.unwrap()).unwrap();
        planted["kilns"] = serde_json::json!([stranger.to_string_lossy()]);
        fs::write(&meta_path, serde_json::to_string_pretty(&planted).unwrap())
            .await
            .unwrap();
        // Precondition: the registry really does not know it, or the assertion
        // below holds for the wrong reason.
        assert!(
            storage.kiln_registry().name_for(&stranger).is_none(),
            "precondition: {} must be unregistered",
            stranger.display()
        );

        let loaded = storage.load(&session_id).await.unwrap();
        assert!(
            loaded.kilns.is_empty(),
            "an unregistered path must not become a kiln: {:?}",
            loaded.kilns
        );

        // Saving the session back must not drop the record of it.
        storage.save(&loaded).await.unwrap();
        let after: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&meta_path).await.unwrap()).unwrap();
        assert_eq!(
            after["kilns"],
            serde_json::json!([stranger.to_string_lossy()]),
            "the unresolved path is carried forward, not deleted"
        );
    }

    #[tokio::test]
    async fn test_session_storage_append_event() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        storage
            .append_event(&session, r#"{"type":"text","content":"hello"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text","content":"world"}"#)
            .await
            .unwrap();

        // Verify events were appended
        let jsonl_path = session.jsonl_path(storage.sessions_root());
        let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
        assert_eq!(content.lines().count(), 2);
    }

    #[tokio::test]
    async fn test_session_storage_append_event_includes_timestamp() {
        use crucible_core::protocol::SessionEventMessage;
        use serde_json::json;

        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Create an event with timestamp
        let event =
            SessionEventMessage::new(&session.id, "user_message", json!({"content": "hello"}))
                .with_timestamp();

        // Serialize and append
        let json_str = serde_json::to_string(&event).unwrap();
        storage.append_event(&session, &json_str).await.unwrap();

        // Read back and verify timestamp is present
        let jsonl_path = session.jsonl_path(storage.sessions_root());
        let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(content.lines().next().unwrap()).unwrap();

        // Verify timestamp field exists and is ISO8601 format
        assert!(
            parsed.get("timestamp").is_some(),
            "timestamp field missing from persisted event"
        );
        let timestamp_str = parsed.get("timestamp").unwrap().as_str().unwrap();
        // Basic ISO8601 validation: should contain T and Z
        assert!(
            timestamp_str.contains('T'),
            "timestamp not in ISO8601 format"
        );
        assert!(
            timestamp_str.ends_with('Z'),
            "timestamp should end with Z (UTC)"
        );
    }

    #[tokio::test]
    async fn test_session_storage_load_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let result = storage
            .load(&crate::test_support::sid("nonexistent-session"))
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_session_storage_list_empty() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let summaries = storage.list().await.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_session_storage_append_event_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        // Create session but don't save it first
        let session = session_in(&tmp, SessionType::Chat);

        // append_event should create the directory if needed
        storage
            .append_event(&session, r#"{"type":"text","content":"test"}"#)
            .await
            .unwrap();

        // Verify the directory and file were created
        assert!(session.jsonl_path(storage.sessions_root()).exists());
    }

    #[tokio::test]
    async fn test_session_storage_preserves_all_fields() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let other_kiln = crate::test_support::kiln_name("other-kiln");
        let workspace = tmp.path().join("workspace");

        let session = session_in(&tmp, SessionType::Agent)
            .with_workspace(Some(workspace.clone()))
            .with_kiln(other_kiln.clone())
            .with_title("Test Session");
        let session_id = session.id.clone();
        let kilns = session.kilns.clone();

        storage.save(&session).await.unwrap();

        let loaded = storage.load(&session_id).await.unwrap();
        assert_eq!(loaded.session_type, SessionType::Agent);
        assert_eq!(loaded.workspace, Some(workspace));
        assert_eq!(loaded.kilns, kilns);
        assert!(loaded.kilns.contains(&other_kiln));
        assert_eq!(loaded.title, Some("Test Session".to_string()));
    }

    /// A `meta.json` is not a door the scope floor guards: `workspace` is a
    /// plain path, deserialized unchecked, and `session_containment` chains it
    /// into the allowlist next to the kilns. A file that names another
    /// session's storage directory — written before the gate existed, or hand
    /// edited — would otherwise hand the revived session exactly the
    /// transcript the sessions-root denial exists to close, because an allowed
    /// root INSIDE the denial out-ranks it.
    #[tokio::test]
    async fn a_persisted_workspace_inside_the_sessions_root_is_refused_on_load() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);
        let victim = storage.sessions_root().join("chat-victim");
        std::fs::create_dir_all(&victim).unwrap();

        let mut session = session_in(&tmp, SessionType::Chat);
        let session_id = session.id.clone();
        storage.save(&session).await.unwrap();
        // Written straight to disk: `save` goes through the same type, so the
        // forbidden value has to be planted the way a hand-edited file would.
        session.workspace = Some(victim.clone());
        fs::write(
            storage.session_dir(&session_id).join("meta.json"),
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .await
        .unwrap();
        // Precondition: the plant really is on disk and really is forbidden —
        // otherwise this passes because nothing was ever there to refuse.
        let raw = fs::read_to_string(storage.session_dir(&session_id).join("meta.json"))
            .await
            .unwrap();
        assert!(
            raw.contains("chat-victim"),
            "precondition: the forbidden workspace must be in the file: {raw}"
        );
        assert!(
            crate::kiln_registry::refuse_forbidden_scope(
                "workspace",
                &victim,
                storage.sessions_root()
            )
            .is_err(),
            "precondition: the floor must consider this path forbidden"
        );

        let loaded = storage.load(&session_id).await.unwrap();

        assert_eq!(
            loaded.workspace, None,
            "a refused workspace must not survive the load"
        );
        let roots = crate::agent_manager::scope::session_containment(
            &loaded,
            storage.sessions_root(),
            storage.kiln_registry(),
        );
        assert!(
            crate::tools::fs_scope::FsScope::workspace(PathBuf::new(), roots)
                .resolve(&victim.join("session.jsonl").to_string_lossy())
                .is_err(),
            "and the refusal has to reach the containment set, not just the field"
        );
    }

    /// The floor is a floor, not a filter: an ordinary directory persisted as
    /// a workspace is still the session's workspace after a restart.
    #[tokio::test]
    async fn an_ordinary_persisted_workspace_survives_the_load() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);
        let workspace = tmp.path().join("project");
        std::fs::create_dir_all(&workspace).unwrap();

        let session = session_in(&tmp, SessionType::Chat).with_workspace(Some(workspace.clone()));
        let session_id = session.id.clone();
        storage.save(&session).await.unwrap();

        let loaded = storage.load(&session_id).await.unwrap();

        assert_eq!(loaded.workspace, Some(workspace));
    }

    #[tokio::test]
    async fn test_session_storage_append_markdown() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        storage
            .append_markdown(&session, "User", "Hello!")
            .await
            .unwrap();
        storage
            .append_markdown(&session, "Assistant", "Hi there!")
            .await
            .unwrap();

        // Verify markdown was created
        let content = tokio::fs::read_to_string(session.log_path(storage.sessions_root()))
            .await
            .unwrap();

        // Check frontmatter
        assert!(content.starts_with("---\n"));
        assert!(content.contains(&format!("session_id: {}", session.id)));
        assert!(content.contains("type: chat"));

        // Check entries
        assert!(content.contains("## User -"));
        assert!(content.contains("Hello!"));
        assert!(content.contains("## Assistant -"));
        assert!(content.contains("Hi there!"));
    }

    #[tokio::test]
    async fn test_session_storage_markdown_creates_frontmatter_once() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Agent);
        storage.save(&session).await.unwrap();

        storage
            .append_markdown(&session, "User", "First")
            .await
            .unwrap();
        storage
            .append_markdown(&session, "Agent", "Second")
            .await
            .unwrap();
        storage
            .append_markdown(&session, "User", "Third")
            .await
            .unwrap();

        let content = tokio::fs::read_to_string(session.log_path(storage.sessions_root()))
            .await
            .unwrap();

        // Should only have one frontmatter block
        let frontmatter_count = content.matches("---\n").count();
        assert_eq!(frontmatter_count, 2); // Opening and closing ---

        // Should have all entries
        assert!(content.contains("First"));
        assert!(content.contains("Second"));
        assert!(content.contains("Third"));
    }

    #[tokio::test]
    async fn test_session_storage_append_markdown_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        // Create session but don't save it first
        let session = session_in(&tmp, SessionType::Workflow);

        // append_markdown should create the directory if needed
        storage
            .append_markdown(&session, "System", "Starting workflow")
            .await
            .unwrap();

        // Verify the directory and file were created
        let md_path = session.log_path(storage.sessions_root());
        assert!(md_path.exists());

        let content = tokio::fs::read_to_string(&md_path).await.unwrap();
        assert!(content.contains("type: workflow"));
        assert!(content.contains("# Workflow Session"));
        assert!(content.contains("Starting workflow"));
    }

    #[tokio::test]
    async fn test_session_storage_load_events() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Append some events
        storage
            .append_event(&session, r#"{"type":"text","content":"first"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text","content":"second"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text","content":"third"}"#)
            .await
            .unwrap();

        // Load all events
        let events = storage.load_events(&session.id, None, None).await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["content"], "first");
        assert_eq!(events[2]["content"], "third");

        // Load with pagination
        let events = storage
            .load_events(&session.id, Some(2), Some(1))
            .await
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["content"], "second");
        assert_eq!(events[1]["content"], "third");
    }

    #[tokio::test]
    async fn test_session_storage_count_events() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Empty initially
        let count = storage.count_events(&session.id).await.unwrap();
        assert_eq!(count, 0);

        // Append events
        storage
            .append_event(&session, r#"{"type":"text"}"#)
            .await
            .unwrap();
        storage
            .append_event(&session, r#"{"type":"text"}"#)
            .await
            .unwrap();

        let count = storage.count_events(&session.id).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_session_storage_load_events_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        // Load events for session with no JSONL file
        let events = storage
            .load_events(&crate::test_support::sid("nonexistent"), None, None)
            .await
            .unwrap();
        assert!(events.is_empty());

        let count = storage
            .count_events(&crate::test_support::sid("nonexistent"))
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_session_storage_load_events_with_malformed_json() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Write a mix of valid and malformed JSON lines directly
        let content = r#"{"type":"text","content":"valid1"}
{invalid json here
{"type":"text","content":"valid2"}
not json at all
{"type":"text","content":"valid3"}
{"unclosed": "brace"
"#;
        tokio::fs::write(session.jsonl_path(storage.sessions_root()), content)
            .await
            .unwrap();

        // Load events - should skip malformed lines and return only valid ones
        let events = storage.load_events(&session.id, None, None).await.unwrap();

        // Should have 3 valid events (the malformed lines are skipped with warning)
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["content"], "valid1");
        assert_eq!(events[1]["content"], "valid2");
        assert_eq!(events[2]["content"], "valid3");
    }

    #[tokio::test]
    async fn test_session_storage_load_events_all_malformed() {
        let tmp = TempDir::new().unwrap();
        let storage = storage_in(&tmp);

        let session = session_in(&tmp, SessionType::Chat);
        storage.save(&session).await.unwrap();

        // Write only malformed JSON
        let content = r#"{invalid json
not json at all
{"unclosed": "brace"
"#;
        tokio::fs::write(session.jsonl_path(storage.sessions_root()), content)
            .await
            .unwrap();

        // Load events - should return empty vec when all lines are malformed
        let events = storage.load_events(&session.id, None, None).await.unwrap();

        assert!(events.is_empty());
    }
}
