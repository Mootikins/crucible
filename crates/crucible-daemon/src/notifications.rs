//! Daemon-owned notifications with a scope.
//!
//! `cru.log.notify` in any VM sends a `NotifyRequest` here through a sync
//! channel. The hub resolves the scope, keeps the newest `RING` entries in
//! `<data_home>/notifications.json`, and tells every matching live session
//! with a `notification_added` event that carries the body. A notification
//! with no scope goes out on the wildcard.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use crucible_core::config::KilnName;
use crucible_core::session::SessionState;
use crucible_core::types::{Notification, NotificationScope};
use crucible_lua::{NotificationSink, NotifyRequest};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

use crate::event_emitter::emit_event;
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
}

impl Default for NotificationFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            items: VecDeque::new(),
        }
    }
}

/// The daemon's notification store and its fan-out.
pub struct NotificationHub {
    store: RegistryStore<NotificationFile>,
    sessions: Arc<SessionManager>,
    projects: Arc<ProjectManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    tx: mpsc::Sender<NotifyRequest>,
    /// Taken once by `spawn_drain`.
    rx: Mutex<Option<mpsc::Receiver<NotifyRequest>>>,
}

/// What a VM holds: the channel into the hub, and the session the VM
/// belongs to when it is a session VM.
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
        event_tx: broadcast::Sender<SessionEventMessage>,
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
    /// session VM, so the plugin VM's `None` is the only way to stay unstamped.
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
        let notification = request.notification.with_scope(scope).with_created_now();
        let stored = notification.clone();
        self.store
            .update(move |file| {
                file.items.push_front(notification);
                file.items.truncate(RING);
                Ok(())
            })
            .context("failed to store the notification")?;
        self.fan_out(&stored);
        Ok(stored)
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
        let items = match self.store.read() {
            Ok(file) => file.items,
            Err(e) => {
                warn!(error = %e, "failed to read the notification ring");
                VecDeque::new()
            }
        };
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

    /// Drop one notification. True when it was there. Every client hears
    /// about it, because every client that showed it must take it down.
    pub fn dismiss(&self, id: &str) -> bool {
        let removed = self.store.update(|file| {
            let before = file.items.len();
            file.items.retain(|n| n.id != id);
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
            emit_event(
                &self.event_tx,
                SessionEventMessage::new(
                    WILDCARD_SESSION,
                    "notification_dismissed",
                    serde_json::json!({ "notification_id": id }),
                ),
            );
        }
        removed
    }

    /// Explicit hints win. Without them, a session VM's request takes the
    /// session's own workspace and kilns. Anything else is global.
    ///
    /// A bad kiln name is dropped with a warning. When nothing else was
    /// given, the request then falls back to the session's scope rather
    /// than widen to everyone.
    fn resolve_scope(&self, request: &NotifyRequest) -> NotificationScope {
        let mut scope = NotificationScope {
            workspace: request.workspace.as_deref().map(canonical),
            kilns: Vec::new(),
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
        }
    }

    /// One event per matching live session, or one wildcard event when the
    /// notification is global.
    fn fan_out(&self, notification: &Notification) {
        let data = serde_json::json!({
            "notification_id": notification.id,
            "notification": notification,
        });
        if notification.scope.is_global() {
            emit_event(
                &self.event_tx,
                SessionEventMessage::new(WILDCARD_SESSION, "notification_added", data),
            );
            return;
        }
        for session in self.sessions.list_sessions() {
            if session.archived || session.state == SessionState::Ended {
                continue;
            }
            let workspace = session.workspace.as_deref().map(canonical);
            if !notification
                .scope
                .matches(workspace.as_deref(), &session.kilns)
            {
                continue;
            }
            emit_event(
                &self.event_tx,
                SessionEventMessage::new(session.id.as_str(), "notification_added", data.clone()),
            );
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
    use std::time::{Duration, Instant};
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
        event_tx: broadcast::Sender<SessionEventMessage>,
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
        let (event_tx, events) = broadcast::channel(512);
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
    async fn a_session_vm_notification_takes_the_session_workspace_and_kilns() {
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
        let f = fixture();
        let sink = f.hub.sink(None);
        for i in 0..NOTIFY_QUEUE {
            NotificationSink::notify(&*sink, request(&format!("m{i}"), None, None, None)).unwrap();
        }

        let err =
            NotificationSink::notify(&*sink, request("overflow", None, None, None)).unwrap_err();
        assert_eq!(err, "notification queue full");

        f.hub.spawn_drain();
        let last = format!("m{}", NOTIFY_QUEUE - 1);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let listed = f.hub.list(None, &[], true);
            if listed.first().is_some_and(|n| n.message == last) {
                assert_eq!(listed.len(), RING);
                assert!(listed.iter().all(|n| n.message != "overflow"));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the drain did not empty the queue"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
