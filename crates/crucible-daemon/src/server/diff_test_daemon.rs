//! The daemon state that the `diff.*` RPC tests share.
//!
//! The `server::diff` tests and the `server::diff_comments` tests call the
//! same handlers against the same registries. One fixture serves both, so a
//! change to `Admission` changes one test type.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crucible_core::diff::DiffsetSource;
use crucible_core::session::PhysicalRoot;
use tempfile::TempDir;
use tokio::sync::broadcast;

use super::diff::{handle_diff_file, handle_diff_get, Admission};
use super::diff_comments::{
    handle_diff_comment, handle_diff_comments, handle_diff_delete_comment,
    handle_diff_resolve_comment,
};
use crate::kiln_manager::KilnManager;
use crate::project_manager::ProjectManager;
use crate::proposals::ProposalStore;
use crate::protocol::{Request, RequestId, RpcError, SessionEventMessage};
use crate::review::ReviewLedgers;
use crate::session_manager::SessionManager;
use crate::test_support::{git, init_repo};

/// The daemon state of one test: a project registry, a kiln manager, a
/// session manager, the review ledgers and the proposal store, each under a
/// temp directory, and the event channel that the comment RPCs announce on.
pub(crate) struct Daemon {
    pub(crate) projects: Arc<ProjectManager>,
    pub(crate) kilns: Arc<KilnManager>,
    pub(crate) sessions: Arc<SessionManager>,
    pub(crate) review: Arc<ReviewLedgers>,
    pub(crate) proposals: ProposalStore,
    pub(crate) event_tx: crate::EventBus,
    pub(crate) events: broadcast::Receiver<SessionEventMessage>,
    _store: TempDir,
}

impl Daemon {
    pub(crate) fn new() -> Self {
        let store = TempDir::new().unwrap();
        let (event_tx, events) = crate::EventBus::channel(16);
        Self {
            projects: Arc::new(ProjectManager::new(store.path().join("projects.json"))),
            kilns: Arc::new(KilnManager::new()),
            sessions: Arc::new(SessionManager::with_storage(
                crate::test_support::temp_session_storage(),
            )),
            review: Arc::new(ReviewLedgers::for_tests(store.path().join("snapshots"))),
            proposals: ProposalStore::new(store.path().join("proposals")),
            event_tx,
            events,
            _store: store,
        }
    }

    pub(crate) fn admission(&self) -> Admission<'_> {
        Admission {
            projects: &self.projects,
            kilns: &self.kilns,
            sessions: &self.sessions,
            review: &self.review,
            proposals: &self.proposals,
        }
    }

    /// Send one `diff.*` request to its handler. Decode the result as `T`.
    pub(crate) async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: impl serde::Serialize,
    ) -> Result<T, RpcError> {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap(),
        };
        let resp = match method {
            "diff.get" => handle_diff_get(req, self.admission()).await,
            "diff.file" => handle_diff_file(req, self.admission()).await,
            "diff.comment" => handle_diff_comment(req, self.admission(), &self.event_tx).await,
            "diff.resolve_comment" => {
                handle_diff_resolve_comment(req, self.admission(), &self.event_tx).await
            }
            "diff.delete_comment" => {
                handle_diff_delete_comment(req, self.admission(), &self.event_tx).await
            }
            "diff.comments" => handle_diff_comments(req, self.admission()).await,
            other => panic!("no handler for {other}"),
        };
        match resp.error {
            Some(error) => Err(error),
            None => Ok(serde_json::from_value(resp.result.expect("a result")).unwrap()),
        }
    }
}

/// A registered repository on `main` with `files` in one commit, and a
/// checked-out branch `feature`.
pub(crate) async fn repo(daemon: &Daemon, files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path(), files).await;
    git(tmp.path(), &["branch", "-M", "main"]).await;
    git(tmp.path(), &["checkout", "-q", "-b", "feature"]).await;
    let root = daemon.projects.register(tmp.path()).unwrap().path;
    (tmp, root)
}

/// The branch source of `root` with the default base and the working tree.
pub(crate) fn working_tree(root: &Path) -> DiffsetSource {
    DiffsetSource::Branch {
        root: PhysicalRoot::from_top_level(root),
        base: String::new(),
        head: None,
    }
}
