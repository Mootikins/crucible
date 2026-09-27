//! Daemon-owned notifications with a scope.
//!
//! `cru.log.notify` in any VM sends a `NotifyRequest` here through a sync
//! channel. The hub resolves the scope, keeps the newest `RING` entries in
//! `<data_home>/notifications.json`, and tells every matching live session
//! with a `notification_added` event that carries the body. A notification
//! with no scope goes out on the wildcard.
//!
//! A session that closes a shared notification hides it for itself only.
//! The file keeps the hidden ids of each session next to the ring, so they
//! live as long as the ring does, across a daemon restart too.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use crucible_core::config::KilnName;
use crucible_core::session::{Session, SessionState};
use crucible_core::types::{Notification, NotificationScope};
use crucible_lua::{NotificationSink, NotifyRequest};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::project_manager::ProjectManager;
use crate::protocol::SessionEventMessage;
use crate::registry_store::RegistryStore;
use crate::session_manager::SessionManager;
use crate::subscription::WILDCARD_SESSION;

/// The file under the data root that keeps the ring.
pub const NOTIFICATIONS_FILE: &str = "notifications.json";
/// How many notifications the ring keeps. The newest wins.
pub const RING: usize = 200;
/// How many `cru.log.notify` calls may wait for the drain. A full queue
/// makes the next call raise in Lua, so the daemon never grows without a
/// limit.
pub const NOTIFY_QUEUE: usize = 1024;
const FILE_VERSION: u32 = 1;

/// The ring on disk. Newest first, so `truncate` drops the oldest.
#[derive(Serialize, Deserialize)]
struct NotificationFile {
    version: u32,
    items: VecDeque<Notification>,
    /// The shared notifications that each session closed, by session id.
    /// Every id names an entry of `items`, so one set holds at most `RING`
    /// ids. A file from before this field reads as no hidden notification.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    hidden: BTreeMap<String, BTreeSet<String>>,
}

impl Default for NotificationFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            items: VecDeque::new(),
            hidden: BTreeMap::new(),
        }
    }
}

impl NotificationFile {
    /// Drop each hidden id whose notification left the ring, and each
    /// session that then hides nothing. Call after every change to `items`.
    fn prune_hidden(&mut self) {
        let Self { items, hidden, .. } = self;
        let in_ring: HashSet<&str> = items.iter().map(|n| n.id.as_str()).collect();
        hidden.retain(|_, ids| {
            ids.retain(|id| in_ring.contains(id.as_str()));
            !ids.is_empty()
        });
    }

    /// The ids that the session `session_id` hides. Empty when it hides none.
    fn hidden_by(&self, session_id: &str) -> &BTreeSet<String> {
        static NONE: BTreeSet<String> = BTreeSet::new();
        self.hidden.get(session_id).unwrap_or(&NONE)
    }
}

/// What `dismiss_for_session` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionDismissal {
    /// The notification belonged to the session. The ring dropped it.
    Removed,
    /// The notification is shared. The session hides it from now on.
    Hidden,
    /// The session hid the notification before. Nothing changed.
    AlreadyHidden,
    /// The notification does not reach the session, or it is not there.
    Refused,
}

/// The daemon's notification store and its fan-out.
pub struct NotificationHub {
    store: RegistryStore<NotificationFile>,
    sessions: Arc<SessionManager>,
    projects: Arc<ProjectManager>,
    event_tx: crate::EventBus,
    tx: mpsc::Sender<NotifyRequest>,
    /// Taken once by `spawn_drain`.
    rx: Mutex<Option<mpsc::Receiver<NotifyRequest>>>,
}

/// What a VM holds: the channel into the hub, and the session the VM
/// a call belongs to, when the caller named one.
struct HubSink {
    tx: mpsc::Sender<NotifyRequest>,
    session_id: Option<String>,
}

impl NotificationSink for HubSink {
    fn notify(&self, mut request: NotifyRequest) -> std::result::Result<(), String> {
        if self.session_id.is_some() {
            request.session_id = self.session_id.clone();
        }
        self.tx.try_send(request).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => "notification queue full".to_string(),
            mpsc::error::TrySendError::Closed(_) => "the notification hub is gone".to_string(),
        })
    }
}

impl NotificationHub {
    pub fn new(
        data_home: &Path,
        sessions: Arc<SessionManager>,
        projects: Arc<ProjectManager>,
        event_tx: crate::EventBus,
    ) -> Self {
        let (tx, rx) = mpsc::channel(NOTIFY_QUEUE);
        Self {
            store: RegistryStore::new(data_home.join(NOTIFICATIONS_FILE)),
            sessions,
            projects,
            event_tx,
            tx,
            rx: Mutex::new(Some(rx)),
        }
    }

    /// The sink a VM gets. `session_id` is stamped onto every request from a
    /// caller, so `None` is the only way to stay unstamped.
    pub fn sink(self: &Arc<Self>, session_id: Option<&str>) -> Arc<dyn NotificationSink> {
        Arc::new(HubSink {
            tx: self.tx.clone(),
            session_id: session_id.map(str::to_string),
        })
    }

    /// Take the receiver and drain it for the life of the daemon. Call once
    /// at bind; a second call finds no receiver and does nothing.
    pub fn spawn_drain(self: &Arc<Self>) {
        let Some(mut rx) = self.rx.lock().expect("notification rx: poisoned").take() else {
            debug!("the notification drain already runs");
            return;
        };
        let hub = Arc::clone(self);
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                if let Err(e) = hub.add(request) {
                    warn!(error = %e, "failed to store a notification");
                }
            }
        });
    }

    /// Resolve the scope, store the notification and tell every matching
    /// live session. Returns the notification as stored.
    pub fn add(&self, request: NotifyRequest) -> Result<Notification> {
        let scope = self.resolve_scope(&request);
        self.insert(request.notification.with_scope(scope))
    }

    /// Store `notification` for the session `session_id` alone, and tell
    /// only that session.
    pub fn add_for_session(
        &self,
        session_id: &str,
        notification: Notification,
    ) -> Result<Notification> {
        self.insert(notification.with_scope(NotificationScope {
            session: Some(session_id.to_string()),
            ..NotificationScope::default()
        }))
    }

    fn insert(&self, notification: Notification) -> Result<Notification> {
        let notification = notification.with_created_now();
        let stored = notification.clone();
        let hidden = self
            .store
            .update(move |file| {
                file.items.push_front(notification);
                file.items.truncate(RING);
                file.prune_hidden();
                Ok(file.hidden.clone())
            })
            .context("failed to store the notification")?;
        self.fan_out(&stored, &hidden);
        Ok(stored)
    }

    /// The file, or an empty one when the read fails. A read failure is
    /// logged, because a list that cannot read the ring shows nothing.
    fn read_file(&self) -> NotificationFile {
        self.store.read().unwrap_or_else(|e| {
            warn!(error = %e, "failed to read the notification ring");
            NotificationFile::default()
        })
    }

    /// The ring, newest first. Without `all`, only what a client with
    /// `workspace` and `kilns` may see; the workspace also brings the kilns
    /// of the project it sits in.
    pub fn list(
        &self,
        workspace: Option<&Path>,
        kilns: &[KilnName],
        all: bool,
    ) -> Vec<Notification> {
        let items = self.read_file().items;
        if all {
            return items.into();
        }
        let workspace = workspace.map(canonical);
        let mut kilns = kilns.to_vec();
        if let Some(workspace) = &workspace {
            kilns.extend(self.project_kilns(workspace));
        }
        items
            .into_iter()
            .filter(|n| n.scope.matches(workspace.as_deref(), &kilns))
            .collect()
    }

    /// Every notification the hub delivers to `session`, newest first,
    /// without the shared notifications that the session hid.
    pub fn list_for_session(&self, session: &Session) -> Vec<Notification> {
        let file = self.read_file();
        let hidden = file.hidden_by(session.id.as_str());
        file.items
            .iter()
            .filter(|n| {
                reaches(
                    n,
                    session.id.as_str(),
                    session.workspace.as_deref(),
                    &session.kilns,
                    hidden,
                )
            })
            .cloned()
            .collect()
    }

    /// Close one notification for `session`. The ring drops a notification
    /// of the session itself. A shared notification that reaches the session
    /// stays in the ring for the other sessions, and `session` hides it.
    /// False when the notification is not there or does not reach `session`.
    pub fn dismiss_for_session(&self, session: &Session, id: &str) -> bool {
        let session_id = session.id.as_str();
        let outcome = self.store.update(|file| {
            let Some(notification) = file.items.iter().find(|n| n.id == id) else {
                return Ok(SessionDismissal::Refused);
            };
            if notification.scope.session.as_deref() == Some(session_id) {
                file.items.retain(|n| n.id != id);
                file.prune_hidden();
                return Ok(SessionDismissal::Removed);
            }
            // Ask without the hidden set: an id that the session hid before
            // still reaches it, and a second close is not a refusal. A
            // notification of another session does not reach this one.
            let no_hidden = BTreeSet::new();
            if !reaches(
                notification,
                session_id,
                session.workspace.as_deref(),
                &session.kilns,
                &no_hidden,
            ) {
                return Ok(SessionDismissal::Refused);
            }
            let newly_hidden = file
                .hidden
                .entry(session_id.to_string())
                .or_default()
                .insert(id.to_string());
            Ok(if newly_hidden {
                SessionDismissal::Hidden
            } else {
                SessionDismissal::AlreadyHidden
            })
        });
        let outcome = outcome.unwrap_or_else(|e| {
            warn!(error = %e, "failed to dismiss a notification");
            SessionDismissal::Refused
        });
        match outcome {
            SessionDismissal::Removed => self.announce_dismissed(WILDCARD_SESSION, id),
            // Only the clients of this session take the notification down.
            SessionDismissal::Hidden => self.announce_dismissed(session_id, id),
            SessionDismissal::AlreadyHidden | SessionDismissal::Refused => {}
        }
        outcome != SessionDismissal::Refused
    }

    /// Drop one notification. True when it was there. Every client hears
    /// about it, because every client that showed it must take it down.
    pub fn dismiss(&self, id: &str) -> bool {
        let removed = self.store.update(|file| {
            let before = file.items.len();
            file.items.retain(|n| n.id != id);
            file.prune_hidden();
            Ok(file.items.len() != before)
        });
        let removed = match removed {
            Ok(removed) => removed,
            Err(e) => {
                warn!(error = %e, "failed to dismiss a notification");
                false
            }
        };
        if removed {
            self.announce_dismissed(WILDCARD_SESSION, id);
        }
        removed
    }

    /// Tell the clients of `session_id` that the notification `id` closed.
    fn announce_dismissed(&self, session_id: &str, id: &str) {
        self.event_tx.emit(SessionEventMessage::new(
            session_id,
            "notification_dismissed",
            serde_json::json!({ "notification_id": id }),
        ));
    }

    /// Explicit hints win. Without them, a session-stamped request takes the
    /// session's own workspace and kilns. Anything else is global.
    ///
    /// A bad kiln name is dropped with a warning. When nothing else was
    /// given, the request then falls back to the session's scope rather
    /// than widen to everyone.
    fn resolve_scope(&self, request: &NotifyRequest) -> NotificationScope {
        let mut scope = NotificationScope {
            workspace: request.workspace.as_deref().map(canonical),
            ..NotificationScope::default()
        };
        if let Some(kiln) = &request.kiln {
            match KilnName::parse(kiln) {
                Ok(name) => scope.kilns.push(name),
                Err(e) => {
                    warn!(kiln = %kiln, error = %e, "cru.log.notify: dropped an invalid kiln name")
                }
            }
        }
        if !scope.is_global() {
            return scope;
        }
        let Some(session) = request
            .session_id
            .as_deref()
            .and_then(|id| self.sessions.get_session(id))
        else {
            return scope;
        };
        NotificationScope {
            workspace: session.workspace.as_deref().map(canonical),
            kilns: session.kilns,
            session: None,
        }
    }

    /// One event per matching live session, or one wildcard event when the
    /// notification is global. `hidden` is the file's hidden sets after the
    /// store, so a session does not get a notification that it hid.
    fn fan_out(&self, notification: &Notification, hidden: &BTreeMap<String, BTreeSet<String>>) {
        let none = BTreeSet::new();
        let data = serde_json::json!({
            "notification_id": notification.id,
            "notification": notification,
        });
        if let Some(session_id) = &notification.scope.session {
            self.event_tx.emit(SessionEventMessage::new(
                session_id.as_str(),
                "notification_added",
                data,
            ));
            return;
        }
        if notification.scope.is_global() {
            self.event_tx.emit(SessionEventMessage::new(
                WILDCARD_SESSION,
                "notification_added",
                data,
            ));
            return;
        }
        for session in self.sessions.list_sessions() {
            if session.archived || session.state == SessionState::Ended {
                continue;
            }
            if !reaches(
                notification,
                session.id.as_str(),
                session.workspace.as_deref(),
                &session.kilns,
                hidden.get(session.id.as_str()).unwrap_or(&none),
            ) {
                continue;
            }
            self.event_tx.emit(SessionEventMessage::new(
                session.id.as_str(),
                "notification_added",
                data.clone(),
            ));
        }
    }

    /// The named kilns of the project `workspace` sits in. The project is
    /// the nearest registered ancestor, `workspace` itself included.
    fn project_kilns(&self, workspace: &Path) -> Vec<KilnName> {
        let project = workspace.ancestors().find_map(|dir| self.projects.get(dir));
        let Some(project) = project else {
            return Vec::new();
        };
        project
            .kilns
            .iter()
            .filter_map(|k| k.name.as_deref())
            .filter_map(|name| KilnName::parse(name).ok())
            .collect()
    }
}

/// True when the hub delivers `notification` to `session`. Delivery and
/// `list_for_session` both ask this, so they cannot disagree. `hidden` is
/// the set of ids that the session closed.
fn reaches(
    notification: &Notification,
    session_id: &str,
    workspace: Option<&Path>,
    kilns: &[KilnName],
    hidden: &BTreeSet<String>,
) -> bool {
    if hidden.contains(&notification.id) {
        return false;
    }
    let workspace = workspace.map(canonical);
    notification.scope.session.as_deref() == Some(session_id)
        || notification.scope.matches(workspace.as_deref(), kilns)
}

/// The canonical path, or the path as given when it does not resolve.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_manager::ProjectManager;
    use crate::subscription::WILDCARD_SESSION;
    use crate::test_support::{kiln_name, temp_session_manager};
    use crucible_core::session::{Session, SessionType};
    use crucible_core::types::Notification;
    use crucible_lua::NotifyRequest;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio::sync::broadcast;

    struct Fixture {
        _tmp: tempfile::TempDir,
        data_home: PathBuf,
        workspace_a: PathBuf,
        workspace_b: PathBuf,
        session_a: String,
        session_b: String,
        sessions: Arc<crate::session_manager::SessionManager>,
        projects: Arc<ProjectManager>,
        event_tx: crate::EventBus,
        events: broadcast::Receiver<SessionEventMessage>,
        hub: Arc<NotificationHub>,
    }

    /// Two live sessions: `a` in `/w/a` with the kiln `notes`, `b` in `/w/b`
    /// with no kiln. The paths are real directories, so canonicalization
    /// works on both sides.
    fn fixture() -> Fixture {
        let tmp = tempfile::TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let workspace_a = tmp.path().join("w/a");
        let workspace_b = tmp.path().join("w/b");
        std::fs::create_dir_all(&workspace_a).unwrap();
        std::fs::create_dir_all(&workspace_b).unwrap();
        let workspace_a = workspace_a.canonicalize().unwrap();
        let workspace_b = workspace_b.canonicalize().unwrap();

        let sessions = temp_session_manager();
        let a = Session::new(SessionType::Chat, vec![kiln_name("notes")])
            .with_workspace(Some(workspace_a.clone()));
        let b = Session::new(SessionType::Chat, vec![]).with_workspace(Some(workspace_b.clone()));
        let session_a = a.id.to_string();
        let session_b = b.id.to_string();
        sessions.register_transient(a);
        sessions.register_transient(b);

        let projects = Arc::new(ProjectManager::new(data_home.join("projects.json")));
        let (event_tx, events) = crate::EventBus::channel(512);
        let hub = Arc::new(NotificationHub::new(
            &data_home,
            sessions.clone(),
            projects.clone(),
            event_tx.clone(),
        ));
        Fixture {
            _tmp: tmp,
            data_home,
            workspace_a,
            workspace_b,
            session_a,
            session_b,
            sessions,
            projects,
            event_tx,
            events,
            hub,
        }
    }

    fn request(
        message: &str,
        session_id: Option<&str>,
        workspace: Option<&Path>,
        kiln: Option<&str>,
    ) -> NotifyRequest {
        NotifyRequest {
            notification: Notification::toast(message),
            session_id: session_id.map(str::to_string),
            workspace: workspace.map(Path::to_path_buf),
            kiln: kiln.map(str::to_string),
        }
    }

    fn drain(events: &mut broadcast::Receiver<SessionEventMessage>) -> Vec<SessionEventMessage> {
        let mut out = Vec::new();
        while let Ok(event) = events.try_recv() {
            out.push(event);
        }
        out
    }

    #[tokio::test]
    async fn a_kiln_scoped_notification_reaches_only_sessions_with_that_kiln() {
        let mut f = fixture();

        f.hub.add(request("p", None, None, Some("notes"))).unwrap();

        let events = drain(&mut f.events);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].session_id, f.session_a);
        assert_eq!(events[0].event, "notification_added");
        assert_eq!(events[0].data["notification"]["message"], "p");
        assert_eq!(
            events[0].data["notification_id"],
            events[0].data["notification"]["id"]
        );

        let listed = f.hub.list(None, &[], true);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].scope.kilns, vec![kiln_name("notes")]);
        assert!(listed[0].created_at.is_some());
        let _ = &f.session_b;
    }

    #[tokio::test]
    async fn fan_out_skips_ended_and_archived_sessions() {
        let mut f = fixture();
        let mut ended = Session::new(SessionType::Chat, vec![kiln_name("notes")]);
        ended.end();
        let mut archived = Session::new(SessionType::Chat, vec![kiln_name("notes")]);
        archived.archived = true;
        f.sessions.register_transient(ended);
        f.sessions.register_transient(archived);

        f.hub.add(request("p", None, None, Some("notes"))).unwrap();

        let events = drain(&mut f.events);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].session_id, f.session_a);
    }

    #[tokio::test]
    async fn a_scoped_notification_takes_the_session_workspace_and_kilns() {
        let f = fixture();

        let stored = f
            .hub
            .add(request("p", Some(&f.session_a), None, None))
            .unwrap();

        assert_eq!(
            stored.scope.workspace.as_deref(),
            Some(f.workspace_a.as_path())
        );
        assert_eq!(stored.scope.kilns, vec![kiln_name("notes")]);
    }

    #[tokio::test]
    async fn explicit_hints_win_over_the_session() {
        let f = fixture();

        let stored = f
            .hub
            .add(request("p", Some(&f.session_a), Some(&f.workspace_b), None))
            .unwrap();

        assert_eq!(
            stored.scope.workspace.as_deref(),
            Some(f.workspace_b.as_path())
        );
        assert!(stored.scope.kilns.is_empty());
    }

    #[tokio::test]
    async fn a_bad_kiln_name_falls_back_to_the_session_scope() {
        let f = fixture();

        let stored = f
            .hub
            .add(request("p", Some(&f.session_a), None, Some("not a kiln!")))
            .unwrap();

        assert_eq!(stored.scope.kilns, vec![kiln_name("notes")]);
        assert_eq!(
            stored.scope.workspace.as_deref(),
            Some(f.workspace_a.as_path())
        );
    }

    /// A session notification stays in its session, even when the session
    /// has no workspace and no kiln that could scope it.
    #[tokio::test]
    async fn a_session_notification_reaches_only_its_session() {
        let mut f = fixture();
        let c = Session::new(SessionType::Chat, vec![]);
        let d = Session::new(SessionType::Chat, vec![]);
        let session_c = c.id.to_string();
        f.sessions.register_transient(c);
        f.sessions.register_transient(d);

        f.hub
            .add_for_session(&session_c, Notification::warning("p"))
            .unwrap();

        let events = drain(&mut f.events);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].session_id, session_c);
        assert_eq!(events[0].event, "notification_added");
    }

    #[tokio::test]
    async fn an_unscoped_notification_goes_to_the_wildcard() {
        let mut f = fixture();

        f.hub.add(request("p", None, None, None)).unwrap();

        let events = drain(&mut f.events);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].session_id, WILDCARD_SESSION);
        assert_eq!(events[0].event, "notification_added");
    }

    #[tokio::test]
    async fn a_workspace_scoped_notification_reaches_that_workspace_only() {
        let mut f = fixture();

        f.hub
            .add(request("p", None, Some(&f.workspace_b), None))
            .unwrap();

        let events = drain(&mut f.events);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].session_id, f.session_b);
    }

    #[tokio::test]
    async fn list_filters_by_workspace_and_expands_the_project_kilns() {
        let f = fixture();
        let config_dir = f.workspace_b.join(".crucible");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(f.workspace_b.join("notes")).unwrap();
        std::fs::write(
            config_dir.join("project.toml"),
            "[[kilns]]\npath = \"./notes\"\nname = \"notes\"\n",
        )
        .unwrap();
        f.projects.register(&f.workspace_b).unwrap();
        let workspace_c = f.workspace_b.parent().unwrap().join("c");
        std::fs::create_dir_all(&workspace_c).unwrap();

        f.hub.add(request("p", None, None, Some("notes"))).unwrap();

        assert_eq!(f.hub.list(Some(&f.workspace_b), &[], false).len(), 1);
        assert_eq!(
            f.hub
                .list(Some(&f.workspace_b.join("deeper")), &[], false)
                .len(),
            1,
            "a directory under the project takes the project's kilns"
        );
        assert_eq!(f.hub.list(Some(&workspace_c), &[], false).len(), 0);
        assert_eq!(f.hub.list(None, &[kiln_name("notes")], false).len(), 1);
        assert_eq!(f.hub.list(None, &[], false).len(), 0);
        assert_eq!(f.hub.list(None, &[], true).len(), 1);
    }

    #[tokio::test]
    async fn the_ring_keeps_the_newest_two_hundred_and_survives_a_reload() {
        let f = fixture();

        for i in 0..205 {
            f.hub
                .add(request(&format!("m{i}"), None, None, None))
                .unwrap();
        }

        let listed = f.hub.list(None, &[], true);
        assert_eq!(listed.len(), RING);
        assert_eq!(listed[0].message, "m204", "newest first");
        assert_eq!(listed[RING - 1].message, "m5");

        let reloaded = NotificationHub::new(
            &f.data_home,
            f.sessions.clone(),
            f.projects.clone(),
            f.event_tx.clone(),
        );
        assert_eq!(reloaded.list(None, &[], true).len(), RING);
        assert!(f.data_home.join(NOTIFICATIONS_FILE).is_file());
    }

    #[tokio::test]
    async fn dismiss_removes_and_broadcasts() {
        let mut f = fixture();
        let stored = f.hub.add(request("p", None, None, None)).unwrap();
        drain(&mut f.events);

        assert!(f.hub.dismiss(&stored.id));

        let events = drain(&mut f.events);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].session_id, WILDCARD_SESSION);
        assert_eq!(events[0].event, "notification_dismissed");
        assert_eq!(events[0].data["notification_id"], stored.id);
        assert!(f.hub.list(None, &[], true).is_empty());
        assert!(!f.hub.dismiss(&stored.id));
        assert!(drain(&mut f.events).is_empty());
    }

    fn session(f: &Fixture, id: &str) -> Session {
        f.sessions.get_session(id).expect("a live session")
    }

    /// A shared notice that session `a` closes leaves `a` only. The ring
    /// keeps it, `b` still lists it, and only the clients of `a` hear the
    /// close.
    #[tokio::test]
    async fn a_closed_shared_notice_leaves_that_session_only() {
        let mut f = fixture();
        let stored = f.hub.add(request("p", None, None, None)).unwrap();
        drain(&mut f.events);
        let (a, b) = (session(&f, &f.session_a), session(&f, &f.session_b));

        assert!(f.hub.dismiss_for_session(&a, &stored.id));

        assert!(f.hub.list_for_session(&a).is_empty());
        assert_eq!(f.hub.list_for_session(&b).len(), 1);
        assert_eq!(f.hub.list(None, &[], true).len(), 1);
        let events = drain(&mut f.events);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].session_id, f.session_a);
        assert_eq!(events[0].event, "notification_dismissed");
        assert_eq!(events[0].data["notification_id"], stored.id);

        assert!(
            f.hub.dismiss_for_session(&a, &stored.id),
            "a second close still answers success"
        );
        assert!(drain(&mut f.events).is_empty(), "and announces nothing");
    }

    /// A session cannot close a notice that never reached it.
    #[tokio::test]
    async fn a_session_cannot_close_a_notice_that_does_not_reach_it() {
        let mut f = fixture();
        let theirs = f
            .hub
            .add(request("p", None, Some(&f.workspace_b), None))
            .unwrap();
        let owned = f
            .hub
            .add_for_session(&f.session_b, Notification::toast("q"))
            .unwrap();
        drain(&mut f.events);
        let a = session(&f, &f.session_a);

        assert!(!f.hub.dismiss_for_session(&a, &theirs.id));
        assert!(!f.hub.dismiss_for_session(&a, &owned.id));
        assert!(!f.hub.dismiss_for_session(&a, "notif-absent"));

        assert!(f.hub.read_file().hidden.is_empty());
        assert_eq!(f.hub.list(None, &[], true).len(), 2);
        assert!(drain(&mut f.events).is_empty());
    }

    /// Live delivery asks the same match as the list. A client may add a
    /// notice with an id of its choice, so an id that a session hid can
    /// arrive again; the session that hid it does not get it.
    #[tokio::test]
    async fn delivery_skips_a_session_that_hid_the_id() {
        let mut f = fixture();
        let notice = Notification::toast("p");
        let kiln_request = |notification: Notification| NotifyRequest {
            notification,
            ..request("p", None, None, Some("notes"))
        };
        f.hub.add(kiln_request(notice.clone())).unwrap();
        assert!(f
            .hub
            .dismiss_for_session(&session(&f, &f.session_a), &notice.id));
        drain(&mut f.events);

        f.hub.add(kiln_request(notice)).unwrap();

        let added: Vec<_> = drain(&mut f.events)
            .into_iter()
            .filter(|e| e.event == "notification_added")
            .collect();
        assert!(added.is_empty(), "{added:?}");
    }

    /// The hidden ids live in the ring file, so they last as long as the
    /// ring: across a reload, and no longer than their notice.
    #[tokio::test]
    async fn hidden_ids_last_as_long_as_their_notice() {
        let f = fixture();
        let a = session(&f, &f.session_a);
        let first = f.hub.add(request("first", None, None, None)).unwrap();
        let second = f.hub.add(request("second", None, None, None)).unwrap();
        assert!(f.hub.dismiss_for_session(&a, &first.id));
        assert!(f.hub.dismiss_for_session(&a, &second.id));

        let reloaded = NotificationHub::new(
            &f.data_home,
            f.sessions.clone(),
            f.projects.clone(),
            f.event_tx.clone(),
        );
        assert!(
            reloaded.list_for_session(&a).is_empty(),
            "a restart keeps them"
        );

        assert!(f.hub.dismiss(&first.id));
        let hidden = f.hub.read_file().hidden;
        assert_eq!(
            hidden.get(f.session_a.as_str()).map(|ids| ids.len()),
            Some(1),
            "{hidden:?}"
        );

        for i in 0..RING {
            f.hub
                .add(request(&format!("m{i}"), None, None, None))
                .unwrap();
        }
        let hidden = f.hub.read_file().hidden;
        assert!(
            hidden.is_empty(),
            "the ring dropped the notice, so the set drops its id: {hidden:?}"
        );
    }

    #[tokio::test]
    async fn the_sink_stamps_its_session_and_the_drain_delivers() {
        let mut f = fixture();
        f.hub.spawn_drain();

        let sink = f.hub.sink(Some(&f.session_a));
        crucible_lua::NotificationSink::notify(&*sink, request("p", None, None, None)).unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(5), f.events.recv())
            .await
            .expect("the drain delivered nothing")
            .unwrap();
        assert_eq!(event.session_id, f.session_a);
        assert_eq!(event.data["notification"]["scope"]["kilns"][0], "notes");
    }

    #[tokio::test]
    async fn notify_refuses_a_full_queue_and_the_drain_empties_it() {
        let mut f = fixture();
        let sink = f.hub.sink(None);
        for i in 0..NOTIFY_QUEUE {
            NotificationSink::notify(&*sink, request(&format!("m{i}"), None, None, None)).unwrap();
        }

        let err =
            NotificationSink::notify(&*sink, request("overflow", None, None, None)).unwrap_err();
        assert_eq!(err, "notification queue full");

        f.hub.spawn_drain();
        // Every stored notification syncs the ring file to disk, so the
        // drain runs at the speed of the disk. A clock deadline measures the
        // disk, not the drain: a slow CI disk failed it. Wait for the event
        // of the last message instead. `add` stores before it fans out, so
        // the ring is complete when that event arrives. The queue holds more
        // than the event channel, so the receiver lags. That is not a fault.
        let last = format!("m{}", NOTIFY_QUEUE - 1);
        loop {
            match f.events.recv().await {
                Ok(event) if event.data["notification"]["message"] == last => break,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => {
                    panic!("the event channel closed before the drain finished")
                }
            }
        }
        let listed = f.hub.list(None, &[], true);
        assert_eq!(listed.len(), RING);
        assert_eq!(listed[0].message, last, "newest first");
        assert!(listed.iter().all(|n| n.message != "overflow"));
    }
}
