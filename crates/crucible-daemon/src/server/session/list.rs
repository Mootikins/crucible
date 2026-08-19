use super::super::*;
use super::scope::caller_kiln_scope;
use crate::rpc_client::SessionIdRequest;
use crate::rpc_helpers::typed_params;
use crate::{optional_param, require_param};

use crucible_core::session::{SessionState, SessionSummary, SessionType};

/// List sessions.
///
/// Params:
///   - `kilns` (array of strings, optional): The **caller's whole kiln set**;
///     a session is listed if its own set overlaps it
///   - `kiln` (string, optional): the single-member spelling of `kilns`
///   - `workspace`, `type`, `state`, `include_archived`, `include_children`
///
/// Unlike the other three backlog-spanning handlers, an absent scope here is
/// not "nothing": this is the listing the TUI resumes from, so it falls back to
/// what the daemon can see without being told — the open kilns, the data home,
/// and the kiln-less sessions that belong to no kiln at all.
pub(crate) async fn handle_session_list(
    req: Request,
    sm: &Arc<SessionManager>,
    km: &Arc<KilnManager>,
    data_home: &std::path::Path,
) -> Response {
    // Parse optional filters
    let scope = match caller_kiln_scope(&req, sm.kiln_registry()) {
        Ok(scope) => scope,
        Err(message) => return Response::error(req.id, INVALID_PARAMS, message),
    };
    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);
    let session_type =
        optional_param!(req, "type", as_str).and_then(|s| s.parse::<SessionType>().ok());
    let state = optional_param!(req, "state", as_str).and_then(|s| match s {
        "active" => Some(SessionState::Active),
        "paused" => Some(SessionState::Paused),
        "compacting" => Some(SessionState::Compacting),
        "ended" => Some(SessionState::Ended),
        _ => None,
    });
    let include_archived = optional_param!(req, "include_archived", as_bool).unwrap_or(false);
    // Delegated child sessions are hidden by default: full sessions in
    // behavior, but not first-class in visibility.
    let include_children = optional_param!(req, "include_children", as_bool).unwrap_or(false);

    let mut sessions = if !scope.is_empty() {
        // Same overlap rule as `session.search`: the caller's whole set, not
        // one member of it.
        let mut listed = sm
            .list_sessions_filtered_async(
                KilnFilter::Any,
                workspace.as_ref(),
                session_type,
                state,
                include_archived,
            )
            .await;
        listed.retain(|s| scope.overlaps(&s.kilns));
        listed
    } else {
        // When no kiln is specified, load sessions from all open kilns + crucible home
        let mut all_sessions = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // Helper: fetch sessions for a kiln and dedup into the accumulator
        let mut collect_from = |sessions: Vec<SessionSummary>| {
            for session in sessions {
                if seen_ids.insert(session.id.clone()) {
                    all_sessions.push(session);
                }
            }
        };

        // Sessions with an EMPTY kiln set, first. Every other branch here
        // filters on kiln membership, so a kiln-less session — a legitimate
        // tools-only agent, and the shape a `session.disconnect_kiln` of the
        // last kiln produces — matched none of them and disappeared from every
        // listing, including the one the TUI resumes from.
        collect_from(
            sm.list_sessions_filtered_async(
                KilnFilter::Kilnless,
                workspace.as_ref(),
                session_type,
                state,
                include_archived,
            )
            .await,
        );

        // Then, sessions from every kiln this daemon knows a name for.
        //
        // The registry is the complete set, and it is the one that matters: a
        // session's kiln set holds NAMES, and a name exists only because an
        // entry does. Fanning out over the *open* kilns alone would hide every
        // session whose kiln happens not to be open — which is most of them on
        // a fresh daemon, because startup opens registered PROJECT kilns and
        // nothing says a `[kilns]` entry belongs to a project.
        //
        // The open set is still folded in, resolved through `name_for` rather
        // than compared as paths: a kiln can be open under a spelling that is
        // not the one the registry stores — a symlink, a `~`, a relative path
        // from wherever the daemon was spawned — and `name_for` compares both
        // forms of both sides so they land on one entry. An open directory with
        // no entry contributes nothing, which is correct: it is not a kiln, so
        // no session can name it.
        let mut names: Vec<crucible_core::config::KilnName> = sm
            .kiln_registry()
            .iter()
            .map(|kiln| kiln.name().clone())
            .collect();
        for (kiln_path, _, _) in km.list().await {
            if let Some(name) = sm.kiln_registry().name_for(&kiln_path) {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        for name in &names {
            let filtered = sm
                .list_sessions_filtered_async(
                    KilnFilter::Attached(name),
                    workspace.as_ref(),
                    session_type,
                    state,
                    include_archived,
                )
                .await;
            collect_from(filtered);
        }

        // The daemon data home used to get a branch of its own here. It cannot
        // any more, and does not need one: the registry refuses the data root
        // and every ancestor of it, so no `[kilns]` entry can name it, so no
        // session can hold a kiln that resolves to it. `server/mod.rs` has
        // documented since `61f28c144` that home is scanned but never OPENED as
        // a kiln — this makes that unrepresentable rather than remembered.
        let _ = data_home;

        all_sessions
    };

    if !include_children {
        sessions.retain(|s| s.parent_session_id.is_none());
    }

    let sessions_json: Vec<_> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "session_id": s.id,
                "type": s.session_type.as_prefix(),
                "kilns": s.kilns,
                "workspace": s.workspace,
                "state": format!("{}", s.state),
                "started_at": s.started_at.to_rfc3339(),
                "last_activity": s.last_activity.map(|t| t.to_rfc3339()),
                "title": s.title,
                "agent_model": s.agent_model,
                "event_count": s.event_count,
                "archived": s.archived,
                "parent_session_id": s.parent_session_id,
            })
        })
        .collect();

    Response::success(
        req.id,
        serde_json::json!({
            "sessions": sessions_json,
            "total": sessions_json.len(),
        }),
    )
}

/// Search persisted session transcripts.
///
/// Params:
///   - `query` (string, required): Substring to match, case-insensitive
///   - `kilns` (array of strings, optional): The **caller's whole kiln set**,
///     not directories to scan
///   - `kiln` (string, optional): the single-member spelling of `kilns`
///   - `limit` (u64, optional): Max matches to return (default 20)
///
/// Every session now lives in one flat root, so the corpus is no longer
/// partitioned by directory and scope has to be stated rather than walked to.
/// A session is searchable only if its kiln set **overlaps** the caller's.
/// That is deliberately narrower than the backlog: with the per-kiln directory
/// boundary gone and no privacy predicate yet (Phase 2), anything wider would
/// read corpora the caller was never cleared for. With no kilns there is no
/// scope at all, so the result is empty rather than everything — which is also
/// what a kiln-less tools-only session gets.
///
/// Overlap is over the caller's *whole* set. Passing one member (`kilns[0]`
/// standing in for the rest) tests a fraction of the caller's reach: a caller
/// on `[A, B]` would miss every session that shares only `B`.
pub(crate) async fn handle_session_search(req: Request, sm: &Arc<SessionManager>) -> Response {
    let query = require_param!(req, "query", as_str);
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    let scope = match caller_kiln_scope(&req, sm.kiln_registry()) {
        Ok(scope) => scope,
        Err(message) => return Response::error(req.id, INVALID_PARAMS, message),
    };
    if scope.is_empty() {
        return Response::success(
            req.id,
            serde_json::json!({
                "matches": [],
                "total": 0,
                "note": "Specify 'kilns' to scope the search to sessions that share one"
            }),
        );
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    // Sessions whose kiln set overlaps the caller's, persisted and in-memory.
    // `KilnFilter` tests one kiln at a time, so the overlap is applied here
    // rather than pushed down: it is a property of the *pair* of kiln sets.
    let reachable = sm
        .list_sessions_filtered_async(KilnFilter::Any, None, None, None, true)
        .await;

    for summary in reachable.iter().filter(|s| scope.overlaps(&s.kilns)) {
        if matches.len() >= limit {
            break;
        }
        let jsonl_path = sm.session_dir(&summary.id).join("session.jsonl");
        let content = match tokio::fs::read_to_string(&jsonl_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (line_num, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                let truncated = if line.len() > 100 {
                    // Use floor_char_boundary to avoid panicking on multi-byte UTF-8
                    let end = line.floor_char_boundary(100);
                    format!("{}...", &line[..end])
                } else {
                    line.to_string()
                };
                matches.push(serde_json::json!({
                    "session_id": summary.id,
                    "line": line_num + 1,
                    "context": truncated
                }));
                break;
            }
        }
    }

    // Supplement for flush lag: an in-process session's transcript may not be
    // on disk yet, so match its title too. Same scope as the corpus pass.
    let active_sessions = sm.list_sessions_filtered(KilnFilter::Any, None, None, None, true);
    for session in active_sessions.iter().filter(|s| scope.overlaps(&s.kilns)) {
        if matches.len() >= limit {
            break;
        }
        if let Some(title) = &session.title {
            if title.to_lowercase().contains(&query_lower)
                && !matches
                    .iter()
                    .any(|m| m["session_id"] == session.id.as_str())
            {
                matches.push(serde_json::json!({
                    "session_id": session.id,
                    "line": 0,
                    "context": format!("[active] {}", title)
                }));
            }
        }
    }

    let total = matches.len();
    Response::success(
        req.id,
        serde_json::json!({
            "matches": matches,
            "total": total
        }),
    )
}

pub(crate) async fn handle_session_get(req: Request, sm: &Arc<SessionManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    match sm.get_session(session_id) {
        Some(session) => {
            let mut response = serde_json::json!({
                "session_id": session.id,
                "type": session.session_type.as_prefix(),
                "kilns": session.kilns,
                "workspace": session.workspace,
                "state": format!("{}", session.state),
                "started_at": session.started_at.to_rfc3339(),
                "title": session.title,
                "continued_from": session.continued_from,
                "parent_session_id": session.parent_session_id,
                "agent": session.agent,
            });

            if let Some(mode) = session.recording_mode {
                response["recording_mode"] = serde_json::json!(format!("{}", mode));
            }

            Response::success(req.id, response)
        }
        None => session_not_found(req.id, session_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiln_manager::KilnManager;
    use crate::session_manager::SessionManager;
    use crate::session_storage::FileSessionStorage;
    use crucible_core::session::Session;
    use tempfile::TempDir;

    fn search_request(params: serde_json::Value) -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.search",
            "params": params,
        }))
        .unwrap()
    }

    /// Persist a session attached to `kilns` with `body` as its transcript.
    async fn seed_session(
        sm: &SessionManager,
        kilns: Vec<crucible_core::config::KilnName>,
        body: &str,
    ) -> String {
        let session = Session::new(SessionType::Chat, kilns);
        sm.update_session(&session).await.unwrap();
        let dir = sm.session_dir(&session.id);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("session.jsonl"), body)
            .await
            .unwrap();
        session.id.to_string()
    }

    /// A `SessionManager` under `tmp` whose registry knows the names these
    /// tests scope by. Without it every seeded kiln would resolve to nothing
    /// and every scope assertion would pass for the wrong reason.
    fn scoped_manager(tmp: &TempDir) -> Arc<SessionManager> {
        let owned: Vec<(&str, PathBuf)> = ["mine", "theirs", "a", "b", "elsewhere", "somewhere"]
            .into_iter()
            .map(|name| (name, tmp.path().join("kilns").join(name)))
            .collect();
        let kilns: Vec<(&str, &std::path::Path)> = owned
            .iter()
            .map(|(name, path)| (*name, path.as_path()))
            .collect();
        let registry = crate::test_support::kiln_registry(tmp.path(), &kilns);
        Arc::new(
            SessionManager::with_storage(Arc::new(
                FileSessionStorage::new(FileSessionStorage::root_for(tmp.path()))
                    .with_registry(registry.clone()),
            ))
            .with_kiln_registry(registry),
        )
    }

    fn kiln(name: &str) -> crucible_core::config::KilnName {
        crate::test_support::kiln_name(name)
    }

    fn matched_ids(resp: &Response) -> Vec<String> {
        resp.result.as_ref().unwrap()["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["session_id"].as_str().unwrap().to_string())
            .collect()
    }

    /// The per-kiln directory scan was the isolation boundary; with one flat
    /// root, kiln-set overlap has to be it instead. A session that shares no
    /// kiln with the caller stays out of the result set.
    #[tokio::test]
    async fn search_returns_only_sessions_sharing_a_kiln_with_the_caller() {
        let tmp = TempDir::new().unwrap();
        let sm = scoped_manager(&tmp);
        let mine = kiln("mine");
        let theirs = kiln("theirs");

        let ours = seed_session(&sm, vec![mine.clone()], "{\"content\":\"needle\"}\n").await;
        let foreign = seed_session(&sm, vec![theirs], "{\"content\":\"needle\"}\n").await;

        let resp = handle_session_search(
            search_request(serde_json::json!({
                "query": "needle",
                "kiln": mine.as_str(),
            })),
            &sm,
        )
        .await;

        let ids = matched_ids(&resp);
        assert!(ids.contains(&ours), "own-kiln session missing: {ids:?}");
        assert!(
            !ids.contains(&foreign),
            "session from an unshared kiln leaked: {ids:?}"
        );
    }

    /// Overlap is over the caller's WHOLE set. Scoping to one member tests a
    /// fraction of what the caller reaches, so a session sharing any other
    /// member is wrongly invisible.
    #[tokio::test]
    async fn search_spans_every_kiln_in_the_callers_set() {
        let tmp = TempDir::new().unwrap();
        let sm = scoped_manager(&tmp);
        let a = kiln("a");
        let b = kiln("b");
        let elsewhere = kiln("elsewhere");

        let on_a = seed_session(&sm, vec![a.clone()], "{\"content\":\"needle\"}\n").await;
        let on_b = seed_session(&sm, vec![b.clone()], "{\"content\":\"needle\"}\n").await;
        let foreign = seed_session(&sm, vec![elsewhere], "{\"content\":\"needle\"}\n").await;

        let resp = handle_session_search(
            search_request(serde_json::json!({
                "query": "needle",
                "kilns": [a.as_str(), b.as_str()],
            })),
            &sm,
        )
        .await;

        let ids = matched_ids(&resp);
        assert!(ids.contains(&on_a), "first-kiln session missing: {ids:?}");
        assert!(
            ids.contains(&on_b),
            "session sharing only the caller's second kiln missing: {ids:?}"
        );
        assert!(
            !ids.contains(&foreign),
            "session from an unshared kiln leaked: {ids:?}"
        );
    }

    /// A caller with no kilns has no overlap with anything — the tools-only
    /// session searches its way into an empty result set, not the backlog.
    #[tokio::test]
    async fn search_from_a_kiln_less_caller_returns_nothing() {
        let tmp = TempDir::new().unwrap();
        let sm = scoped_manager(&tmp);
        seed_session(&sm, vec![kiln("somewhere")], "{\"content\":\"needle\"}\n").await;

        let resp = handle_session_search(
            search_request(serde_json::json!({ "query": "needle", "kilns": [] })),
            &sm,
        )
        .await;

        assert!(matched_ids(&resp).is_empty());
    }

    /// No scope is not "every scope": without `kiln` there is nothing the
    /// caller is provably cleared to read, so the backlog stays closed.
    #[tokio::test]
    async fn search_without_a_kiln_scope_returns_nothing() {
        let tmp = TempDir::new().unwrap();
        let sm = scoped_manager(&tmp);
        seed_session(&sm, vec![kiln("somewhere")], "{\"content\":\"needle\"}\n").await;

        let resp = handle_session_search(
            search_request(serde_json::json!({ "query": "needle" })),
            &sm,
        )
        .await;

        assert!(matched_ids(&resp).is_empty());
    }

    /// Zero kilns is a legitimate session shape, and every branch of the
    /// no-kiln-argument listing filters on kiln MEMBERSHIP — so a tools-only
    /// session matched none of them and disappeared from `session.list`
    /// entirely: unresumable in the TUI, invisible in the web listing, with
    /// nothing to explain where it went.
    #[tokio::test]
    async fn a_kiln_less_session_still_appears_in_the_listing() {
        let km = Arc::new(KilnManager::new());
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().to_path_buf();
        let sm = scoped_manager(&tmp);

        let kiln_less = seed_session(&sm, vec![], "").await;
        // Attached to a registered kiln — the control for "the new branch did
        // not displace the old ones". No `KilnManager` open is needed: the
        // fan-out is over the REGISTRY, which is what a session's kiln names
        // are drawn from.
        let attached = seed_session(&sm, vec![kiln("mine")], "").await;

        let req: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.list",
            "params": {},
        }))
        .unwrap();

        let resp = handle_session_list(req, &sm, &km, &data_home).await;
        let ids: Vec<String> = resp.result.as_ref().unwrap()["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["session_id"].as_str().unwrap().to_string())
            .collect();

        assert!(
            ids.contains(&kiln_less),
            "a session with no kilns vanished from session.list: {ids:?}"
        );
        assert_eq!(
            ids.iter().filter(|id| **id == kiln_less).count(),
            1,
            "the kiln-less branch must dedup against the others: {ids:?}"
        );
        assert!(
            ids.contains(&attached),
            "the new branch must not cost the ordinary listing: {ids:?}"
        );
    }

    /// The fourth scoped handler answers to the same predicate as the other
    /// three: a caller attached to `[a, b]` that can only spell one kiln sees
    /// half its own sessions, and the ones it cannot see are its own.
    #[tokio::test]
    async fn listing_spans_every_kiln_in_the_callers_set() {
        let km = Arc::new(KilnManager::new());
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().to_path_buf();
        let sm = scoped_manager(&tmp);
        let a = kiln("a");
        let b = kiln("b");

        let on_a = seed_session(&sm, vec![a.clone()], "").await;
        let on_b = seed_session(&sm, vec![b.clone()], "").await;
        let foreign = seed_session(&sm, vec![kiln("elsewhere")], "").await;

        let req: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.list",
            "params": { "kilns": [a.as_str(), b.as_str()] },
        }))
        .unwrap();

        let resp = handle_session_list(req, &sm, &km, &data_home).await;
        let ids: Vec<String> = resp.result.as_ref().unwrap()["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["session_id"].as_str().unwrap().to_string())
            .collect();

        assert!(ids.contains(&on_a), "first-kiln session missing: {ids:?}");
        assert!(
            ids.contains(&on_b),
            "session sharing only the caller's second kiln missing: {ids:?}"
        );
        assert!(
            !ids.contains(&foreign),
            "session from an unshared kiln leaked: {ids:?}"
        );
    }

    /// A request that NAMES kilns and resolves none of them is refused.
    ///
    /// This is the empty-set-permits-everything shape one module over: an
    /// empty scope is *permissive* here — it means "every open kiln, the data
    /// home, and the kiln-less sessions" — so a set that resolved to nothing
    /// would turn a request to NARROW into a request that was widened to the
    /// whole backlog. The assertion is therefore on the refusal AND on the
    /// backlog staying closed, not on the absence of a panic.
    #[tokio::test]
    async fn a_listing_scoped_to_names_that_resolve_to_nothing_is_refused() {
        let km = Arc::new(KilnManager::new());
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().to_path_buf();
        let sm = scoped_manager(&tmp);

        let secret = seed_session(&sm, vec![kiln("mine")], "").await;
        let kiln_less = seed_session(&sm, vec![], "").await;
        // Precondition: an UNSCOPED listing really does reach both, so the
        // refusal below is the only thing keeping them out.
        let unscoped: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "session.list", "params": {},
        }))
        .unwrap();
        let wide = handle_session_list(unscoped, &sm, &km, &data_home).await;
        let wide_ids: Vec<String> = wide.result.as_ref().unwrap()["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["session_id"].as_str().unwrap().to_string())
            .collect();
        assert!(
            wide_ids.contains(&secret) && wide_ids.contains(&kiln_less),
            "precondition: an unscoped listing is the wide one: {wide_ids:?}"
        );

        for scope in [
            serde_json::json!({ "kilns": ["no-such-kiln"] }),
            // A path is not a name, and must not degrade to "said nothing".
            serde_json::json!({ "kilns": ["/home/user/notes"] }),
            // Nor is a non-string. `filter_map(as_str)` would drop it and
            // leave an empty array behind — the widening again, by another
            // route.
            serde_json::json!({ "kilns": [7] }),
            serde_json::json!({ "kiln": "no-such-kiln" }),
        ] {
            let req: Request = serde_json::from_value(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "session.list",
                "params": scope,
            }))
            .unwrap();

            let resp = handle_session_list(req, &sm, &km, &data_home).await;

            let error = resp
                .error
                .as_ref()
                .unwrap_or_else(|| panic!("{scope} must be refused, got {:?}", resp.result));
            assert_eq!(error.code, INVALID_PARAMS, "{scope}: {error:?}");
            assert!(
                resp.result.is_none(),
                "{scope} must return no sessions at all, got {:?}",
                resp.result
            );
        }
    }

    /// The other half of the same rule: a set that resolves *partly* keeps the
    /// members that resolve. Dropping is safe only because the non-empty
    /// remainder still narrows — which is exactly why the all-dropped case
    /// above has to be a refusal instead.
    #[tokio::test]
    async fn a_partly_resolvable_scope_keeps_the_kilns_that_exist() {
        let km = Arc::new(KilnManager::new());
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().to_path_buf();
        let sm = scoped_manager(&tmp);

        let mine = seed_session(&sm, vec![kiln("mine")], "").await;
        let theirs = seed_session(&sm, vec![kiln("theirs")], "").await;

        let req: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.list",
            "params": { "kilns": ["mine", "no-such-kiln"] },
        }))
        .unwrap();

        let resp = handle_session_list(req, &sm, &km, &data_home).await;
        let ids: Vec<String> = resp.result.as_ref().unwrap()["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["session_id"].as_str().unwrap().to_string())
            .collect();

        assert!(
            ids.contains(&mine),
            "the resolvable member still scopes: {ids:?}"
        );
        assert!(
            !ids.contains(&theirs),
            "and the unresolvable one must not widen it: {ids:?}"
        );
    }

    /// `session.search` answers to the same parse site, and its empty-scope
    /// behaviour is the *closed* one — so the risk there is the opposite:
    /// silently returning nothing for a request the caller believes narrowed.
    /// It is refused, distinguishing "said nothing" from "said something that
    /// is not a kiln".
    #[tokio::test]
    async fn a_search_scoped_to_names_that_resolve_to_nothing_is_refused() {
        let tmp = TempDir::new().unwrap();
        let sm = scoped_manager(&tmp);
        seed_session(&sm, vec![kiln("mine")], "{\"content\":\"needle\"}\n").await;

        let resp = handle_session_search(
            search_request(serde_json::json!({ "query": "needle", "kilns": ["no-such-kiln"] })),
            &sm,
        )
        .await;

        let error = resp
            .error
            .as_ref()
            .expect("an unresolvable scope is refused");
        assert_eq!(error.code, INVALID_PARAMS, "{error:?}");
        assert!(
            resp.result.is_none(),
            "and it must not answer with an empty match list, which reads as \
             'nothing matched': {:?}",
            resp.result
        );
    }

    /// `server/mod.rs` documents the invariant this handler broke: the title
    /// sweep scans `~/.crucible` "but never OPENS home as a kiln — that is the
    /// leak this split fixes" (commit `61f28c144`). Listing sessions opened it
    /// anyway, and an open kiln is a *watched* kiln, so every session body
    /// under the data root — including a chat integration's transcript of a
    /// stranger's messages — was parsed, embedded and indexed permanently.
    ///
    /// The open was never load-bearing: `list_sessions_filtered_async` reads
    /// the in-memory map and `storage.list(kiln_path)`, and never consults
    /// `KilnManager` at all.
    #[tokio::test]
    async fn listing_sessions_never_opens_the_data_root_as_a_kiln() {
        let km = Arc::new(KilnManager::new());
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().to_path_buf();
        let sm = scoped_manager(&tmp);

        let req: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.list",
            "params": {},
        }))
        .unwrap();

        let _ = handle_session_list(req, &sm, &km, &data_home).await;

        let opened: Vec<_> = km.list().await.into_iter().map(|(p, _, _)| p).collect();
        assert!(
            opened.is_empty(),
            "session.list opened kilns as a side effect: {opened:?}"
        );
    }
}
