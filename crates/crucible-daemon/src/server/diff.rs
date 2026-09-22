//! The `diff.get` and `diff.file` RPCs.
//!
//! `diff.get` answers the list of files of one diffset, with counts and no
//! text. `diff.file` answers the two texts of one file of that diffset. The
//! client asks for the texts only when the user expands the file.
//!
//! # Admission
//!
//! The root of a branch diffset must be a registered project, the workspace
//! folder of a session, or a path inside a registered kiln. The root must also
//! be the top level of its git repository. A root below the top level is
//! refused, because git would then list files outside the admitted root.
//! Each check runs before git reads the repository.
//!
//! A proposal diffset needs no admission. The daemon reads the files of a
//! proposal from its own store, not from a root that the client names.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crucible_core::diff::{DiffFileText, Diffset, DiffsetSource};
use crucible_core::session::PhysicalRoot;

use crate::diff::branch;
use crate::kiln_manager::KilnManager;
use crate::project_manager::ProjectManager;
use crate::proposals::{ProposalError, ProposalStore};
use crate::protocol::{Request, RequestId, Response, INTERNAL_ERROR, INVALID_PARAMS};
use crate::review::{ReviewError, ReviewLedgers};
use crate::rpc_client::{DiffFileRequest, DiffGetRequest};
use crate::scm::{run_git, GitOpts};
use crate::server::fs::project_root;
use crate::session_manager::SessionManager;
use crate::tools::containment::reject_non_normal;

/// The refusal for a root that no admission names.
pub(crate) const DIFF_ROOT_NOT_ADMITTED: &str =
    "root is not a registered project, a session's own workspace folder or a registered kiln";

/// The refusal for a root that is not the top level of a git repository.
pub(crate) const NOT_A_REPOSITORY: &str = "root is not the top level of a git repository";

/// An error code and its message.
pub(crate) type Refusal = (i32, String);

pub(crate) fn params_error(message: impl Into<String>) -> Refusal {
    (INVALID_PARAMS, message.into())
}

pub(crate) fn internal_error(error: impl std::fmt::Display) -> Refusal {
    (INTERNAL_ERROR, format!("{error:#}"))
}

/// The daemon state that admits a root.
pub(crate) struct Admission<'a> {
    pub(crate) projects: &'a Arc<ProjectManager>,
    pub(crate) kilns: &'a Arc<KilnManager>,
    pub(crate) sessions: &'a Arc<SessionManager>,
    /// The review ledgers, which hold the base of each session record.
    pub(crate) review: &'a Arc<ReviewLedgers>,
    /// The proposals, which hold the base and the new text of each write.
    pub(crate) proposals: &'a ProposalStore,
}

impl Admission<'_> {
    /// The canonical base that admits `root`, or `None`.
    async fn base(&self, root: &Path) -> Option<PathBuf> {
        if let Some(base) = project_root(self.projects, self.sessions, root).await {
            return Some(base);
        }
        let canonical = root.canonicalize().ok()?;
        self.kilns.admit_kiln_root(&canonical).await
    }

    /// The git top level of `root`, when an admission names it.
    async fn branch_root(&self, root: &Path) -> Result<PhysicalRoot, Refusal> {
        let Some(base) = self.base(root).await else {
            return Err(params_error(DIFF_ROOT_NOT_ADMITTED));
        };
        let canonical = root
            .canonicalize()
            .map_err(|_| params_error(DIFF_ROOT_NOT_ADMITTED))?;
        let top = run_git(
            &canonical,
            &["rev-parse", "--show-toplevel"],
            GitOpts::default(),
        )
        .await
        .map_err(|_| params_error(NOT_A_REPOSITORY))?;
        let top = Path::new(top.trim())
            .canonicalize()
            .map_err(|_| params_error(NOT_A_REPOSITORY))?;
        // The kiln base can be a parent of the root. The root itself must be
        // the top level, so that each path of the diff is inside the base.
        if top != canonical || !top.starts_with(&base) {
            return Err(params_error(NOT_A_REPOSITORY));
        }
        Ok(PhysicalRoot::from_top_level(top))
    }
}

/// The resolved sides of a branch diffset.
pub(crate) struct BranchSides {
    pub(crate) root: PhysicalRoot,
    pub(crate) base: String,
    pub(crate) head: Option<String>,
    pub(crate) merge_base: String,
}

impl BranchSides {
    pub(crate) fn source(&self) -> DiffsetSource {
        DiffsetSource::Branch {
            root: self.root.clone(),
            base: self.base.clone(),
            head: self.head.clone(),
        }
    }
}

/// Admit the root of a branch source and find its merge base.
///
/// An empty `base` names the default branch of the repository.
pub(crate) async fn branch_sides(
    admission: &Admission<'_>,
    root: &Path,
    base: &str,
    head: Option<&str>,
) -> Result<BranchSides, Refusal> {
    let root = admission.branch_root(root).await?;
    let base = if base.is_empty() {
        branch::default_branch(&root)
            .await
            .map_err(|e| params_error(format!("{e:#}")))?
    } else {
        base.to_string()
    };
    let merge_base = branch::merge_base(&root, &base, head)
        .await
        .map_err(|e| params_error(format!("no merge base of {base}: {e:#}")))?;
    Ok(BranchSides {
        root,
        base,
        head: head.map(str::to_string),
        merge_base,
    })
}

/// A proposal error as an RPC refusal. The caller can correct each variant
/// except a store failure and a refused write.
pub(crate) fn proposal_refusal(error: ProposalError) -> Refusal {
    match error {
        ProposalError::NotFound(_)
        | ProposalError::Settled(..)
        | ProposalError::NoWrite(..)
        | ProposalError::NoConflict(..) => params_error(error.to_string()),
        ProposalError::WriteFailed(_) | ProposalError::Store(_) => internal_error(error),
    }
}

/// The root of a file of a proposal diffset. A proposal can write more than
/// one root, and no write renames a file.
pub(crate) fn proposal_file_root<'a>(
    root: Option<&'a PhysicalRoot>,
    from: Option<&str>,
) -> Result<&'a PhysicalRoot, Refusal> {
    let Some(root) = root else {
        return Err(params_error("a proposal file needs its root"));
    };
    if from.is_some() {
        return Err(params_error("a proposal has no renamed file"));
    }
    Ok(root)
}

async fn diff_get(admission: &Admission<'_>, source: &DiffsetSource) -> Result<Diffset, Refusal> {
    match source {
        DiffsetSource::Branch { root, base, head } => {
            let sides = branch_sides(admission, root, base, head.as_deref()).await?;
            let files = branch::changes(&sides.root, &sides.merge_base, sides.head.as_deref())
                .await
                .map_err(internal_error)?;
            let source = sides.source();
            Ok(Diffset {
                id: source.id(),
                source,
                files,
            })
        }
        DiffsetSource::SessionRecord { session } => {
            let files = admission
                .review
                .record_files(session.as_str())
                .await
                .map_err(internal_error)?;
            Ok(Diffset {
                id: source.id(),
                source: source.clone(),
                files,
            })
        }
        DiffsetSource::Proposal { id } => Ok(Diffset {
            id: source.id(),
            source: source.clone(),
            files: admission
                .proposals
                .diff_files(id)
                .map_err(proposal_refusal)?,
        }),
    }
}

/// The two texts of `path` in a branch diffset. The base text comes from
/// `base_path`, the old path of a renamed file.
pub(crate) async fn branch_file_text(
    sides: &BranchSides,
    path: &str,
    base_path: &str,
) -> Result<DiffFileText, Refusal> {
    if sides.head.is_none() {
        check_contained(&sides.root, path)?;
    }
    let base_text = branch::file_text(&sides.root, Some(&sides.merge_base), base_path)
        .await
        .map_err(internal_error)?;
    let current_text = branch::file_text(&sides.root, sides.head.as_deref(), path)
        .await
        .map_err(internal_error)?;
    Ok(DiffFileText {
        base_text: base_text.into_shown(),
        current_text: current_text.into_shown(),
    })
}

/// Refuse a path that is not a plain relative path.
pub(crate) fn check_path(path: &str) -> Result<(), Refusal> {
    let relative = Path::new(path);
    if path.is_empty() || relative.is_absolute() || path.contains('\0') {
        return Err(params_error(format!(
            "not a path relative to the root: {path:?}"
        )));
    }
    reject_non_normal(relative).map_err(|e| params_error(e.to_string()))
}

/// Refuse a working-tree path whose parent directory resolves outside `root`.
///
/// A symbolic link to a directory can put a plain path outside the root. The
/// leaf itself is not resolved: a link as the leaf gives its target text.
pub(crate) fn check_contained(root: &Path, path: &str) -> Result<(), Refusal> {
    let Some(parent) = root.join(path).parent().map(Path::to_path_buf) else {
        return Ok(());
    };
    match parent.canonicalize() {
        Ok(resolved) if !resolved.starts_with(root) => {
            Err(params_error(format!("path escapes the root: {path:?}")))
        }
        // A missing parent gives an absent file.
        _ => Ok(()),
    }
}

async fn diff_file(
    admission: &Admission<'_>,
    request: &DiffFileRequest,
) -> Result<DiffFileText, Refusal> {
    check_path(&request.path)?;
    let base_path = request.from.as_deref().unwrap_or(&request.path);
    check_path(base_path)?;
    match &request.source {
        DiffsetSource::Branch { root, base, head } => {
            let sides = branch_sides(admission, root, base, head.as_deref()).await?;
            if request.root.as_ref().is_some_and(|r| *r != sides.root) {
                return Err(params_error(
                    "the root of the request is not the root of the branch source",
                ));
            }
            branch_file_text(&sides, &request.path, base_path).await
        }
        DiffsetSource::SessionRecord { session } => {
            let Some(root) = &request.root else {
                return Err(params_error("a session record file needs its root"));
            };
            // The ledger lists no renames, so no file has an old path.
            if request.from.is_some() {
                return Err(params_error("a session record has no renamed file"));
            }
            check_contained(root, &request.path)?;
            admission
                .review
                .record_text(session.as_str(), root, &request.path)
                .await
                .map_err(|e| match e {
                    ReviewError::NoLedger(_) | ReviewError::PathEscapesRoot { .. } => {
                        params_error(e.to_string())
                    }
                    other => internal_error(other),
                })
        }
        DiffsetSource::Proposal { id } => {
            let root = proposal_file_root(request.root.as_ref(), request.from.as_deref())?;
            admission
                .proposals
                .diff_text(id, root, &request.path)
                .map_err(proposal_refusal)
        }
    }
}

pub(crate) fn answer<T: serde::Serialize>(
    id: Option<RequestId>,
    result: Result<T, Refusal>,
) -> Response {
    match result.and_then(|value| serde_json::to_value(value).map_err(internal_error)) {
        Ok(value) => Response::success(id, value),
        Err((code, message)) => Response::error(id, code, message),
    }
}

/// Handle the `diff.get` RPC.
pub(crate) async fn handle_diff_get(req: Request, admission: Admission<'_>) -> Response {
    let params = match crate::rpc_helpers::typed_params::<DiffGetRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    answer(req.id, diff_get(&admission, &params.source).await)
}

/// Handle the `diff.file` RPC.
pub(crate) async fn handle_diff_file(req: Request, admission: Admission<'_>) -> Response {
    let params = match crate::rpc_helpers::typed_params::<DiffFileRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    answer(req.id, diff_file(&admission, &params).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RpcError;
    use crate::test_support::{git, init_repo};
    use crucible_core::diff::{DiffFileEntry, FileStatus};
    use crucible_core::types::acp::MAX_DIFF_BYTES;
    use serde_json::{json, Value};
    use std::fs;
    use tempfile::TempDir;

    /// The daemon state of one test: a project registry, a kiln manager and
    /// a session manager, each under a temp directory.
    struct Daemon {
        projects: Arc<ProjectManager>,
        kilns: Arc<KilnManager>,
        sessions: Arc<SessionManager>,
        review: Arc<ReviewLedgers>,
        proposals: ProposalStore,
        _store: TempDir,
    }

    impl Daemon {
        fn new() -> Self {
            let store = TempDir::new().unwrap();
            Self {
                projects: Arc::new(ProjectManager::new(store.path().join("projects.json"))),
                kilns: Arc::new(KilnManager::new()),
                sessions: Arc::new(SessionManager::with_storage(
                    crate::test_support::temp_session_storage(),
                )),
                review: Arc::new(ReviewLedgers::for_tests(store.path().join("snapshots"))),
                proposals: ProposalStore::new(store.path().join("proposals")),
                _store: store,
            }
        }

        fn admission(&self) -> Admission<'_> {
            Admission {
                projects: &self.projects,
                kilns: &self.kilns,
                sessions: &self.sessions,
                review: &self.review,
                proposals: &self.proposals,
            }
        }

        async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
            let req = Request {
                jsonrpc: "2.0".to_string(),
                id: Some(RequestId::Number(1)),
                method: method.to_string(),
                params,
            };
            let resp = match method {
                "diff.get" => handle_diff_get(req, self.admission()).await,
                "diff.file" => handle_diff_file(req, self.admission()).await,
                other => panic!("no handler for {other}"),
            };
            match resp.error {
                Some(error) => Err(error),
                None => Ok(resp.result.expect("a success has a result")),
            }
        }

        async fn get(&self, source: &DiffsetSource) -> Result<Diffset, RpcError> {
            let value = self.call("diff.get", json!({ "source": source })).await?;
            Ok(serde_json::from_value(value).unwrap())
        }

        async fn file(
            &self,
            source: &DiffsetSource,
            path: &str,
            from: Option<&str>,
        ) -> Result<DiffFileText, RpcError> {
            self.file_in(source, None, path, from).await
        }

        async fn file_in(
            &self,
            source: &DiffsetSource,
            root: Option<&PhysicalRoot>,
            path: &str,
            from: Option<&str>,
        ) -> Result<DiffFileText, RpcError> {
            let request = DiffFileRequest {
                source: source.clone(),
                path: path.to_string(),
                from: from.map(str::to_string),
                root: root.cloned(),
            };
            let value = self
                .call("diff.file", serde_json::to_value(request).unwrap())
                .await?;
            Ok(serde_json::from_value(value).unwrap())
        }
    }

    /// A registered repository on `main` with `files` in one commit, and a
    /// checked-out branch `feature`.
    async fn repo(daemon: &Daemon, files: &[(&str, &str)]) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path(), files).await;
        git(tmp.path(), &["branch", "-M", "main"]).await;
        git(tmp.path(), &["checkout", "-q", "-b", "feature"]).await;
        let root = daemon.projects.register(tmp.path()).unwrap().path;
        (tmp, root)
    }

    async fn commit_all(dir: &Path) {
        git(dir, &["add", "-A"]).await;
        git(dir, &["commit", "-q", "-m", "change"]).await;
    }

    /// The branch source of `root` with the default base and the working tree.
    fn working_tree(root: &Path) -> DiffsetSource {
        DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level(root),
            base: String::new(),
            head: None,
        }
    }

    fn entry<'a>(diffset: &'a Diffset, path: &str) -> &'a DiffFileEntry {
        diffset
            .files
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("no entry for {path}: {:?}", diffset.files))
    }

    #[tokio::test]
    async fn branch_diff_lists_added_modified_deleted_and_renamed() {
        let daemon = Daemon::new();
        let body = "one\ntwo\nthree\nfour\nfive\n";
        let (tmp, root) = repo(
            &daemon,
            &[("old.md", body), ("keep.md", "a\nb\n"), ("gone.md", "x\n")],
        )
        .await;
        let dir = tmp.path();
        git(dir, &["mv", "old.md", "new.md"]).await;
        fs::write(dir.join("keep.md"), "a\nB\nc\n").unwrap();
        fs::remove_file(dir.join("gone.md")).unwrap();
        commit_all(dir).await;
        // One change stays in the working tree.
        fs::write(dir.join("fresh.md"), "f\n").unwrap();

        let diffset = daemon.get(&working_tree(&root)).await.unwrap();

        // The daemon names the default branch that it used.
        let expected_source = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level(&root),
            base: "main".into(),
            head: None,
        };
        assert_eq!(diffset.source, expected_source);
        assert_eq!(diffset.id, expected_source.id());

        let listed: Vec<_> = diffset
            .files
            .iter()
            .map(|e| (e.path.as_str(), e.status.clone(), e.added, e.removed))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("fresh.md", FileStatus::Added, 1, 0),
                ("gone.md", FileStatus::Deleted, 0, 1),
                ("keep.md", FileStatus::Modified, 2, 1),
                (
                    "new.md",
                    FileStatus::Renamed {
                        from: "old.md".into()
                    },
                    0,
                    0
                ),
            ]
        );

        // A named head leaves out the working tree.
        let committed = daemon
            .get(&DiffsetSource::Branch {
                root: PhysicalRoot::from_top_level(&root),
                base: "main".into(),
                head: Some("feature".into()),
            })
            .await
            .unwrap();
        assert_eq!(committed.files.len(), 3);
        assert!(committed.files.iter().all(|e| e.path != "fresh.md"));
        assert_ne!(committed.id, diffset.id);

        let error = daemon
            .get(&DiffsetSource::Branch {
                root: PhysicalRoot::from_top_level(&root),
                base: "no-such-branch".into(),
                head: None,
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn a_root_that_is_not_a_repository_is_refused() {
        let daemon = Daemon::new();
        let plain = TempDir::new().unwrap();
        let plain_root = daemon.projects.register(plain.path()).unwrap().path;
        let error = daemon.get(&working_tree(&plain_root)).await.unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        assert_eq!(error.message, NOT_A_REPOSITORY);

        // A folder below the top level would list files outside it.
        // The kiln admits the folder, but the folder is not the top level.
        let (tmp, _root) = repo(&daemon, &[("sub/a.md", "a\n")]).await;
        let sub = tmp.path().join("sub");
        daemon.kilns.open(&sub).await.unwrap();
        let error = daemon.get(&working_tree(&sub)).await.unwrap_err();
        assert_eq!(error.message, NOT_A_REPOSITORY);
        let error = daemon
            .file(&working_tree(&sub), "a.md", None)
            .await
            .unwrap_err();
        assert_eq!(error.message, NOT_A_REPOSITORY);
    }

    #[tokio::test]
    async fn a_root_that_is_not_admitted_is_refused() {
        let daemon = Daemon::new();
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path(), &[("a.md", "a\n")]).await;
        git(tmp.path(), &["branch", "-M", "main"]).await;

        let source = working_tree(tmp.path());
        let error = daemon.get(&source).await.unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        assert_eq!(error.message, DIFF_ROOT_NOT_ADMITTED);
        let error = daemon.file(&source, "a.md", None).await.unwrap_err();
        assert_eq!(error.message, DIFF_ROOT_NOT_ADMITTED);

        // An open kiln admits its root.
        daemon.kilns.open(tmp.path()).await.unwrap();
        let diffset = daemon.get(&source).await.unwrap();
        assert_eq!(
            diffset.source,
            DiffsetSource::Branch {
                root: PhysicalRoot::from_top_level(tmp.path().canonicalize().unwrap()),
                base: "main".into(),
                head: None,
            }
        );
    }

    #[tokio::test]
    async fn diff_get_lists_the_files_of_a_proposal() {
        use crucible_core::file_write::ExpectedBase;
        use crucible_core::note_edit::disk_hash;
        use crucible_core::proposal::ProposalAuthor;
        use crucible_core::session::SessionId;

        let daemon = Daemon::new();
        let kiln = TempDir::new().unwrap();
        fs::write(kiln.path().join("keep.md"), "one\n").unwrap();
        let root = PhysicalRoot::from_top_level(kiln.path());
        let author = ProposalAuthor::Plugin {
            name: "reflection".into(),
        };
        let session = SessionId::parse("aux-1").unwrap();
        let record = |path: &str, base: ExpectedBase, text: &str| {
            daemon
                .proposals
                .record_write(
                    author.clone(),
                    &session,
                    root.clone(),
                    path,
                    base,
                    text.into(),
                )
                .unwrap()
        };
        record("new.md", ExpectedBase::Absent, "a\nb\n");
        let proposal = record(
            "keep.md",
            ExpectedBase::Text {
                text: "one\n".into(),
                hash: disk_hash("one\n"),
            },
            "one\ntwo\n",
        );

        let source = DiffsetSource::Proposal { id: proposal.id };
        let diffset = daemon.get(&source).await.unwrap();
        assert_eq!(diffset.id, source.id());
        assert_eq!(diffset.source, source);
        let listed: Vec<_> = diffset
            .files
            .iter()
            .map(|e| (e.path.as_str(), e.status.clone(), e.added, e.removed))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("new.md", FileStatus::Added, 2, 0),
                ("keep.md", FileStatus::Modified, 1, 0),
            ]
        );
        assert!(diffset.files.iter().all(|e| e.root == root));

        // The base side is the text that the writer read. The current side is
        // the new text. The disk does not change.
        let text = daemon
            .file_in(&source, Some(&root), "keep.md", None)
            .await
            .unwrap();
        assert_eq!(
            text,
            DiffFileText {
                base_text: Some("one\n".into()),
                current_text: Some("one\ntwo\n".into()),
            }
        );
        let added = daemon
            .file_in(&source, Some(&root), "new.md", None)
            .await
            .unwrap();
        assert_eq!(added.base_text, None);
        assert_eq!(added.current_text.as_deref(), Some("a\nb\n"));
        assert_eq!(
            fs::read_to_string(kiln.path().join("keep.md")).unwrap(),
            "one\n"
        );

        // The file request must name the root and a path that the proposal
        // writes, and no old path.
        let error = daemon.file(&source, "keep.md", None).await.unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        let error = daemon
            .file_in(&source, Some(&root), "other.md", None)
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        let error = daemon
            .file_in(&source, Some(&root), "keep.md", Some("old.md"))
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);

        // An unknown proposal is the caller's error.
        let unknown = DiffsetSource::Proposal {
            id: "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f".parse().unwrap(),
        };
        let error = daemon.get(&unknown).await.unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
    }

    fn record(session: &str) -> DiffsetSource {
        DiffsetSource::SessionRecord {
            session: crucible_core::session::SessionId::parse(session).unwrap(),
        }
    }

    #[tokio::test]
    async fn a_session_record_serves_the_base_and_the_disk() {
        let daemon = Daemon::new();
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path(), &[("a.md", "one\n")]).await;
        daemon
            .review
            .open("chat-1", &[tmp.path().to_path_buf()])
            .await
            .unwrap();
        fs::write(tmp.path().join("a.md"), "one\ntwo\n").unwrap();

        let source = record("chat-1");
        let diffset = daemon.get(&source).await.unwrap();
        assert_eq!(diffset.id, source.id());
        assert_eq!(diffset.source, source);
        let root = PhysicalRoot::from_top_level(tmp.path().canonicalize().unwrap());
        assert_eq!(
            diffset.files,
            vec![DiffFileEntry {
                root: root.clone(),
                path: "a.md".into(),
                status: FileStatus::Modified,
                added: 1,
                removed: 0,
                binary: false,
                too_large: false,
            }]
        );

        let text = daemon
            .file_in(&source, Some(&root), "a.md", None)
            .await
            .unwrap();
        assert_eq!(
            text,
            DiffFileText {
                base_text: Some("one\n".into()),
                current_text: Some("one\ntwo\n".into()),
            }
        );

        // The file request must name the root, and no old path.
        let error = daemon.file(&source, "a.md", None).await.unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        let error = daemon
            .file_in(&source, Some(&root), "a.md", Some("b.md"))
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        // A root that the session does not track is refused.
        let other = TempDir::new().unwrap();
        let error = daemon
            .file_in(
                &source,
                Some(&PhysicalRoot::from_top_level(other.path())),
                "a.md",
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
        for path in ["../outside", "/etc/passwd", ""] {
            let error = daemon
                .file_in(&source, Some(&root), path, None)
                .await
                .unwrap_err();
            assert_eq!(error.code, INVALID_PARAMS, "{path}");
        }
    }

    #[tokio::test]
    async fn a_session_with_no_ledger_serves_an_empty_record() {
        let daemon = Daemon::new();
        let source = record("chat-1");
        let diffset = daemon.get(&source).await.unwrap();
        assert_eq!(diffset.source, source);
        assert!(diffset.files.is_empty());
    }

    #[tokio::test]
    async fn a_binary_file_has_no_text() {
        let daemon = Daemon::new();
        let (tmp, root) = repo(&daemon, &[("a.md", "a\n")]).await;
        fs::write(tmp.path().join("image.bin"), b"\x89PNG\0\0\x01\x02").unwrap();
        commit_all(tmp.path()).await;

        let source = working_tree(&root);
        let diffset = daemon.get(&source).await.unwrap();
        assert!(entry(&diffset, "image.bin").binary);

        let text = daemon.file(&source, "image.bin", None).await.unwrap();
        assert_eq!(
            text,
            DiffFileText {
                base_text: None,
                current_text: None
            }
        );
    }

    #[tokio::test]
    async fn a_file_over_the_limit_has_no_text() {
        let daemon = Daemon::new();
        let (tmp, root) = repo(&daemon, &[("big.txt", "small\n")]).await;
        let big = "x\n".repeat(MAX_DIFF_BYTES / 2 + 1);
        fs::write(tmp.path().join("big.txt"), &big).unwrap();

        let source = working_tree(&root);
        let diffset = daemon.get(&source).await.unwrap();
        assert!(entry(&diffset, "big.txt").too_large);

        let text = daemon.file(&source, "big.txt", None).await.unwrap();
        assert_eq!(text.base_text.as_deref(), Some("small\n"));
        assert_eq!(text.current_text, None);
    }

    #[tokio::test]
    async fn diff_file_returns_both_texts() {
        let daemon = Daemon::new();
        let body = "one\ntwo\nthree\nfour\nfive\n";
        let (tmp, root) = repo(&daemon, &[("keep.md", "a\nb\n"), ("old.md", body)]).await;
        let dir = tmp.path();
        git(dir, &["mv", "old.md", "new.md"]).await;
        commit_all(dir).await;
        fs::write(dir.join("keep.md"), "a\nB\n").unwrap();
        fs::write(dir.join("fresh.md"), "f\n").unwrap();

        let source = working_tree(&root);
        let text = |base: Option<&str>, current: Option<&str>| DiffFileText {
            base_text: base.map(str::to_string),
            current_text: current.map(str::to_string),
        };
        assert_eq!(
            daemon.file(&source, "keep.md", None).await.unwrap(),
            text(Some("a\nb\n"), Some("a\nB\n"))
        );
        assert_eq!(
            daemon.file(&source, "fresh.md", None).await.unwrap(),
            text(None, Some("f\n"))
        );
        assert_eq!(
            daemon
                .file(&source, "new.md", Some("old.md"))
                .await
                .unwrap(),
            text(Some(body), Some(body))
        );

        // A named head reads the commit, not the working tree.
        let committed = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level(&root),
            base: "main".into(),
            head: Some("feature".into()),
        };
        assert_eq!(
            daemon.file(&committed, "keep.md", None).await.unwrap(),
            text(Some("a\nb\n"), Some("a\nb\n"))
        );

        for path in ["../outside", "/etc/passwd", ""] {
            let error = daemon.file(&source, path, None).await.unwrap_err();
            assert_eq!(error.code, INVALID_PARAMS, "{path}");
        }
        let error = daemon
            .file(&source, "keep.md", Some("../outside"))
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);

        // A link to a directory outside the root does not reach the outside.
        let outside = TempDir::new().unwrap();
        fs::write(outside.path().join("secret.txt"), "secret\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.join("link")).unwrap();
        let error = daemon
            .file(&source, "link/secret.txt", None)
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
    }
}
