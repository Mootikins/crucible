//! Content-search (ripgrep-style) RPC: `search_grep`.
//!
//! Backs the web `POST /api/search/grep` endpoint. Walks an absolute `root`,
//! matching file contents — literal substring by default, regex when the
//! `regex` param is set (honoring `.gitignore`, skipping binaries) — via the
//! shared [`grep_search`](crate::tools::grep_engine::grep_search) engine (ripgrep's
//! `grep-regex`/`grep-searcher` crates) — the same one the `grep_notes` MCP
//! tool uses.
//!
//! # Containment (load-bearing, daemon-side)
//!
//! Like every `fs.*`/`search_*` handler, trust nothing from the thin web layer.
//! `root` is accepted only if it canonicalizes **inside** a registered project
//! or an open kiln (`validate_grep_root`, fail-closed). A `root` outside every
//! known root is rejected with `INVALID_PARAMS` before any disk walk. Symlinks
//! in the root are resolved (canonicalize) before the containment check; the
//! shared walker uses `follow_links(false)` so intra-walk symlinks can't be
//! followed out of the tree either.

use super::*;
use crate::tools::grep_engine::{grep_search, GrepSearchError};

/// Default hit cap when the caller omits `limit`.
pub(crate) const GREP_DEFAULT_LIMIT: usize = 100;
/// Hard cap on hits regardless of the caller's `limit`.
const GREP_MAX_LIMIT: usize = 500;

/// Handle the `search_grep` RPC. Read-only content search over a contained root.
pub(crate) async fn handle_search_grep(
    req: Request,
    pm: &Arc<ProjectManager>,
    km: &Arc<KilnManager>,
    sessions: &Arc<crate::session_manager::SessionManager>,
) -> Response {
    let params =
        match crate::rpc_helpers::typed_params::<crate::rpc_client::GrepSearchRequest>(&req) {
            Ok(p) => p,
            Err(response) => return *response,
        };
    let root = params.root.as_str();
    let query = params.query.as_str();
    let regex = params.regex;
    let glob = params.glob;
    let case_insensitive = params.case_insensitive;
    // The caller's `limit` is a request, not an authority: the hard cap is
    // the daemon's.
    let limit = params.limit.clamp(1, GREP_MAX_LIMIT);

    let canonical_root = match validate_grep_root(pm, km, sessions, root).await {
        Ok(p) => p,
        Err(msg) => return Response::error(req.id, INVALID_PARAMS, msg),
    };

    // User-driven: `root` came from the person at the keyboard (TUI / web
    // search), was validated against the registered projects and kilns above,
    // and is not a model-supplied path.
    match grep_search(
        crate::tools::grep_engine::WalkScope::user_driven(&canonical_root),
        &canonical_root,
        query,
        regex,
        glob.as_deref(),
        limit,
        case_insensitive,
    ) {
        Ok((hits, truncated)) => Response::success(
            req.id,
            serde_json::json!({ "hits": hits, "truncated": truncated }),
        ),
        // A regex that doesn't compile is the caller's mistake, not a daemon
        // failure — reject it as INVALID_PARAMS with the parser's message.
        Err(e @ GrepSearchError::InvalidRegex(_)) => {
            Response::error(req.id, INVALID_PARAMS, e.to_string())
        }
        Err(GrepSearchError::Other(e)) => internal_error(req.id, e),
    }
}

/// Resolve `root` and confirm it is contained within a registered project, a
/// session's own workspace folder, or an open kiln. Returns the canonical
/// root, or the `INVALID_PARAMS` message.
///
/// Open-kilns-only (not `get_or_open`) is deliberate: opening a kiln would
/// initialize `.crucible/` in an arbitrary directory, minting search capability
/// over any path — the same fail-closed stance as `fs::resolve_root`.
async fn validate_grep_root(
    pm: &Arc<ProjectManager>,
    km: &Arc<KilnManager>,
    sessions: &Arc<crate::session_manager::SessionManager>,
    root: &str,
) -> Result<PathBuf, String> {
    let canon = Path::new(root)
        .canonicalize()
        .map_err(|_| "root does not exist".to_string())?;

    // Registered projects (paths are stored canonicalized, but re-canonicalize
    // defensively in case the on-disk target changed).
    for project in pm.list() {
        if let Ok(base) = project.path.canonicalize() {
            if canon.starts_with(&base) {
                return Ok(canon);
            }
        }
    }

    // Kilns — registered, not merely open. Searching only the open set meant
    // a restart made every registered kiln unsearchable, and `lazy` meant
    // "never" rather than "not opened unasked".
    for kiln_path in km.admissible_kiln_roots().await {
        if let Ok(base) = kiln_path.canonicalize() {
            if canon.starts_with(&base) {
                return Ok(canon);
            }
        }
    }

    // The folder the daemon made for a project-less session — the same
    // admission the file tree gets, so search and browse agree on it.
    if sessions
        .session_workspace_containing(&canon)
        .await
        .is_some()
    {
        return Ok(canon);
    }

    Err("root is not within a registered project, a session's own workspace folder, or an open kiln".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_manager::ProjectManager;
    use std::fs;
    use std::sync::Arc;

    fn req(params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "search_grep".to_string(),
            params,
        }
    }

    fn hits_of(resp: &Response) -> &serde_json::Value {
        resp.result
            .as_ref()
            .expect("success response with a result")
    }

    /// A `ProjectManager` (hermetic temp `projects.json`) with `dir` registered,
    /// plus an empty `KilnManager` — the common fixture for these tests.
    fn registered(
        store: &std::path::Path,
        dir: &std::path::Path,
    ) -> (Arc<ProjectManager>, Arc<KilnManager>, std::path::PathBuf) {
        let pm = Arc::new(ProjectManager::new(store.join("projects.json")));
        let project = pm.register(dir).expect("register project");
        let km = Arc::new(KilnManager::new());
        (pm, km, project.path)
    }

    /// A session manager that owns no session folders.
    fn no_sessions() -> Arc<crate::session_manager::SessionManager> {
        crate::test_support::temp_session_manager()
    }

    /// A project-less session's own workspace folder is searchable, like the
    /// project it stands in for; a stranger under the same base is not.
    #[tokio::test]
    async fn a_sessions_own_workspace_folder_is_a_grep_root() {
        let scratch = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let pm = Arc::new(ProjectManager::new(store.path().join("projects.json")));
        let km = Arc::new(KilnManager::new());
        let sessions = Arc::new(
            crate::session_manager::SessionManager::with_storage(
                crate::test_support::temp_session_storage(),
            )
            .with_session_workspace_dir(Some(scratch.path().to_path_buf())),
        );
        let session = sessions
            .create_session(
                crucible_core::session::SessionType::Chat,
                vec![],
                None,
                None,
            )
            .await
            .unwrap();
        let folder = session.workspace.clone().unwrap();
        fs::write(folder.join("a.md"), "a needle here\n").unwrap();
        let stranger = scratch.path().join("not-a-session");
        fs::create_dir(&stranger).unwrap();

        let ok = validate_grep_root(&pm, &km, &sessions, folder.to_str().unwrap()).await;
        assert_eq!(ok, Ok(folder.canonicalize().unwrap()));
        let refused = validate_grep_root(&pm, &km, &sessions, stranger.to_str().unwrap()).await;
        assert!(refused.is_err(), "a stranger under the base: {refused:?}");

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": folder.to_string_lossy(),
                "query": "needle",
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;
        assert_eq!(hits_of(&resp)["hits"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn greps_notes_with_offsets_and_rel_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::create_dir(proj.join("sub")).unwrap();
        fs::write(proj.join("a.md"), "first line\n    a needle here\n").unwrap();
        fs::write(proj.join("sub/b.md"), "no match here\n").unwrap();
        fs::write(proj.join("c.txt"), "needle in a text file\n").unwrap();

        let (pm, km, root) = registered(store.path(), proj);
        let sessions = no_sessions();

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": root.to_string_lossy(),
                "query": "needle",
                "glob": "*.md",
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        let result = hits_of(&resp);
        let hits = result["hits"].as_array().unwrap();
        // Only the .md hit — the .txt file is excluded by the glob.
        assert_eq!(hits.len(), 1, "hits: {hits:?}");
        let hit = &hits[0];
        assert_eq!(hit["rel_path"], "a.md");
        assert_eq!(hit["line"], 2);
        // Leading whitespace is trimmed and offsets adjusted: "a needle here".
        assert_eq!(hit["text"], "a needle here");
        assert_eq!(hit["match_start"], 2);
        assert_eq!(hit["match_end"], 8);
        assert!(hit["path"].as_str().unwrap().ends_with("a.md"));
        assert_eq!(result["truncated"], false);
    }

    #[tokio::test]
    async fn searches_all_files_when_glob_omitted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::write(proj.join("a.md"), "needle\n").unwrap();
        fs::write(proj.join("c.txt"), "needle\n").unwrap();

        let (pm, km, root) = registered(store.path(), proj);
        let sessions = no_sessions();

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": root.to_string_lossy(),
                "query": "needle",
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        let hits = hits_of(&resp)["hits"].as_array().unwrap().len();
        assert_eq!(hits, 2, "both files should match with no glob filter");
    }

    #[tokio::test]
    async fn root_outside_every_registered_root_is_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        fs::write(outside.path().join("secret.md"), "needle\n").unwrap();

        // Register `tmp` but ask to grep `outside`.
        let (pm, km, _root) = registered(store.path(), tmp.path());
        let sessions = no_sessions();

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": outside.path().to_string_lossy(),
                "query": "needle",
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        assert!(resp.result.is_none(), "should not return a result");
        let err = resp.error.expect("error response");
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("not within a registered"));
    }

    #[tokio::test]
    async fn subdirectory_of_registered_project_is_allowed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::create_dir(proj.join("sub")).unwrap();
        fs::write(proj.join("sub/note.md"), "needle in sub\n").unwrap();

        let (pm, km, root) = registered(store.path(), proj);
        let sessions = no_sessions();
        let sub = root.join("sub");

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": sub.to_string_lossy(),
                "query": "needle",
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        let hits = hits_of(&resp)["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        // rel_path is relative to the searched root (the subdir).
        assert_eq!(hits[0]["rel_path"], "note.md");
    }

    #[tokio::test]
    async fn regex_param_enables_pattern_matching() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::write(proj.join("a.md"), "TODO7: tag\nTODOx: not a digit\n").unwrap();

        let (pm, km, root) = registered(store.path(), proj);
        let sessions = no_sessions();

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": root.to_string_lossy(),
                "query": r"TODO[0-9]:",
                "regex": true,
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        let hits = hits_of(&resp)["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "only the digit line should match");
        assert_eq!(hits[0]["line"], 1);
    }

    #[tokio::test]
    async fn invalid_regex_is_rejected_as_invalid_params() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        fs::write(tmp.path().join("a.md"), "content\n").unwrap();

        let (pm, km, root) = registered(store.path(), tmp.path());
        let sessions = no_sessions();

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": root.to_string_lossy(),
                "query": "foo(",
                "regex": true,
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        assert!(resp.result.is_none());
        let err = resp.error.expect("error response");
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(
            err.message.contains("Invalid regex"),
            "message should identify the bad regex: {}",
            err.message
        );
    }

    #[tokio::test]
    async fn metachars_stay_literal_without_regex_param() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        fs::write(tmp.path().join("a.md"), "fooXbar\nfoo.bar\n").unwrap();

        let (pm, km, root) = registered(store.path(), tmp.path());
        let sessions = no_sessions();

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": root.to_string_lossy(),
                "query": "foo.bar",
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        let hits = hits_of(&resp)["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "literal dot must not match fooXbar");
        assert_eq!(hits[0]["text"], "foo.bar");
    }

    #[tokio::test]
    async fn limit_is_clamped_and_truncation_flagged() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::write(proj.join("a.md"), "needle\nneedle\nneedle\nneedle\n").unwrap();

        let (pm, km, root) = registered(store.path(), proj);
        let sessions = no_sessions();

        let resp = handle_search_grep(
            req(serde_json::json!({
                "root": root.to_string_lossy(),
                "query": "needle",
                "limit": 2,
            })),
            &pm,
            &km,
            &sessions,
        )
        .await;

        let result = hits_of(&resp);
        assert_eq!(result["hits"].as_array().unwrap().len(), 2);
        assert_eq!(result["truncated"], true);
    }
}
