use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, Json};
use crucible_core::config::expand_tilde;
// `Project` is crucible-core's own wire type. Every one of these routes
// answers it, so none of them keeps a copy of its shape.
use crucible_core::Project;
use crucible_daemon::project_manager::untrusted_root_refusal;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn project_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(register_project))
        .routes(routes!(unregister_project))
        .routes(routes!(list_projects))
        .routes(routes!(get_project))
}

#[derive(Debug, Deserialize, ToSchema)]
struct ProjectPathRequest {
    /// Absolute path of the project root.
    #[schema(value_type = String)]
    path: PathBuf,
}

/// What `POST /api/project/unregister` answers.
///
/// The route's own shape: the daemon reports the unregistration as `()`, so
/// there is no daemon body to forward.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ProjectUnregisterResponse {
    /// Always true. A refusal is an error status, not a `false`.
    ok: bool,
}

/// The optional tightening filter over registration.
///
/// The gate is [`untrusted_root_refusal`] — the filesystem root, the home
/// directory, credential stores and the user's config tree are refused whatever
/// this returns. On top of that, `[web] registration_roots` lets an operator
/// confine registration to an explicit set: `None` (the default, empty list)
/// leaves the floor as the only gate, so any ordinary directory `cru web` is
/// pointed at registers, exactly as running `cru` inside it does. `Some(roots)`
/// additionally requires containment in one of them.
///
/// Entries are canonicalized and floor-checked, so `registration_roots = ["/"]`
/// cannot re-open the hole. A non-empty list whose every entry is invalid
/// yields `Some(empty)`, which refuses everything — a misconfigured allowlist
/// fails closed rather than falling back to the floor.
pub(crate) fn registration_roots(state: &AppState) -> Option<Vec<PathBuf>> {
    let home = dirs::home_dir();
    let configured = state
        .config
        .web
        .as_ref()
        .map(|w| w.registration_roots.as_slice())
        .unwrap_or_default();
    if configured.is_empty() {
        return None;
    }

    let mut roots = Vec::with_capacity(configured.len());
    for raw in configured {
        let raw = expand_tilde(raw, home.as_deref());
        let Ok(root) = raw.canonicalize() else {
            tracing::debug!(root = %raw.display(), "Ignoring unresolvable registration_roots entry");
            continue;
        };
        match untrusted_root_refusal(&root, home.as_deref()) {
            Some(reason) => {
                tracing::warn!(root = %root.display(), %reason, "Ignoring forbidden registration_roots entry")
            }
            None => roots.push(root),
        }
    }
    Some(roots)
}

pub(crate) fn contained(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

fn refuse_outside_restriction(path: &Path) -> WebError {
    WebError::Forbidden(format!(
        "Refusing to register {}: it is not inside a [web] registration_roots entry.",
        path.display()
    ))
}

/// `POST /api/project/register` — make a directory a registered project.
///
/// The credential-store, config-tree and filesystem-floor refusals live in
/// the daemon now (`project_manager::register_untrusted`, reached through
/// `project.register`'s `untrusted` flag): this route asks for the untrusted
/// path, so every caller of that RPC method gets the same refusal, not only
/// requests that arrive through this route. `[web] registration_roots`
/// stays here: it is an operator setting of the web process, not a daemon
/// concept, so it is checked before AND after the daemon acts — the daemon
/// resolves a registration inside a git repo up to the repo root, which can
/// land above what was checked.
#[utoipa::path(
    post,
    path = "/api/project/register",
    request_body = ProjectPathRequest,
    responses(
        (status = 200, body = Project),
        (status = 403, description = "The path is outside a configured `[web] registration_roots` entry"),
        (status = 422, description = "The daemon refuses this path as a root for an untrusted caller, and the body says why"),
        (status = 502, description = "The daemon could not register the project"),
    )
)]
async fn register_project(
    State(state): State<AppState>,
    Json(req): Json<ProjectPathRequest>,
) -> Result<Json<Project>, WebError> {
    let restriction = registration_roots(&state);
    // Canonicalize before deciding, so a symlink cannot present a name inside
    // a restriction root for a target outside it.
    let canonical = req.path.canonicalize().map_err(|_| {
        WebError::Forbidden(format!(
            "Refusing to register {}: it does not resolve to an existing directory.",
            req.path.display()
        ))
    })?;
    if let Some(roots) = restriction.as_deref() {
        if !contained(&canonical, roots) {
            return Err(refuse_outside_restriction(&canonical));
        }
    }

    let project = state
        .daemon
        .project_register_untrusted(&canonical)
        .await
        .daemon_err()?;

    // The daemon resolves a registration inside a git repo up to the repo
    // root, which can land ABOVE what was checked. Re-check the restriction
    // where it actually landed and undo it if it escaped — nothing outside
    // `registration_roots` stays registered. The daemon reports a canonical
    // path, so this re-checks the restriction only; the daemon already
    // refused a credential store or the floor.
    if let Some(roots) = restriction.as_deref() {
        if !contained(&project.path, roots) {
            if let Err(e) = state.daemon.project_unregister(&project.path).await {
                tracing::error!(
                    path = %project.path.display(),
                    error = %e,
                    "Failed to roll back an out-of-base project registration"
                );
            }
            return Err(refuse_outside_restriction(&project.path));
        }
    }

    Ok(Json(project))
}

/// `POST /api/project/unregister` — forget a registered project.
///
/// The directory stays on disk; only the registration goes.
#[utoipa::path(
    post,
    path = "/api/project/unregister",
    request_body = ProjectPathRequest,
    responses(
        (status = 200, body = ProjectUnregisterResponse),
        (status = 502, description = "The daemon could not unregister the project"),
    )
)]
async fn unregister_project(
    State(state): State<AppState>,
    Json(req): Json<ProjectPathRequest>,
) -> Result<Json<ProjectUnregisterResponse>, WebError> {
    state
        .daemon
        .project_unregister(&req.path)
        .await
        .daemon_err()?;

    Ok(Json(ProjectUnregisterResponse { ok: true }))
}

/// `GET /api/project/list` — every project the daemon holds registered.
#[utoipa::path(
    get,
    path = "/api/project/list",
    responses(
        (status = 200, body = Vec<Project>),
        (status = 502, description = "The daemon could not list the projects"),
    )
)]
async fn list_projects(State(state): State<AppState>) -> Result<Json<Vec<Project>>, WebError> {
    let projects = state.daemon.project_list().await.daemon_err()?;

    Ok(Json(projects))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct GetProjectQuery {
    /// Absolute path of the project root.
    #[param(value_type = String)]
    path: PathBuf,
}

/// `GET /api/project/get` — one project by its root path.
///
/// A path no project is registered for answers 404 rather than a null body,
/// so a client cannot mistake "not registered" for a project with no fields.
#[utoipa::path(
    get,
    path = "/api/project/get",
    params(GetProjectQuery),
    responses(
        (status = 200, body = Project),
        (status = 404, description = "No project is registered for this path"),
        (status = 502, description = "The daemon could not read the project"),
    )
)]
async fn get_project(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<GetProjectQuery>,
) -> Result<Json<Project>, WebError> {
    match state.daemon.project_get(&query.path).await {
        Ok(Some(project)) => Ok(Json(project)),
        Ok(None) => Err(WebError::NotFound(format!(
            "Project not found: {}",
            query.path.display()
        ))),
        Err(e) => Err(e).daemon_err(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        build_state_with_config, build_test_app, mock_project, shape, start_mock_daemon,
    };
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use crucible_core::config::{CliAppConfig, WebConfig};
    use serde_json::json;
    use tower::ServiceExt;

    /// Config whose registration base is `roots`.
    fn config_with_roots(roots: &[&Path]) -> CliAppConfig {
        CliAppConfig {
            web: Some(WebConfig {
                registration_roots: roots
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
                ..WebConfig::default()
            }),
            ..CliAppConfig::default()
        }
    }

    async fn register(config: CliAppConfig, path: &Path) -> StatusCode {
        let (_mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_state_with_config(client, config));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/project/register")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "path": path.to_string_lossy() }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        response.status()
    }

    /// The mock daemon answers `project.register` with a fixed
    /// `/tmp/test-project`, so any test that expects success has to admit that
    /// path too — the response is containment-checked as well as the request.
    fn base_covering_the_mock_reply(extra: &Path) -> CliAppConfig {
        config_with_roots(&[extra, &std::env::temp_dir()])
    }

    #[tokio::test]
    async fn register_refuses_the_filesystem_root() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            register(config_with_roots(&[tmp.path()]), Path::new("/")).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn register_refuses_a_parent_of_the_registration_base() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        std::fs::create_dir(&base).unwrap();

        assert_eq!(
            register(config_with_roots(&[&base]), tmp.path()).await,
            StatusCode::FORBIDDEN
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn register_refuses_a_symlink_that_escapes_the_registration_base() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let outside = tmp.path().join("outside");
        std::fs::create_dir(&base).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("link")).unwrap();

        // Lexically inside the base; it resolves outside it.
        assert_eq!(
            register(config_with_roots(&[&base]), &base.join("link")).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn register_refuses_a_path_that_does_not_exist() {
        // Fail closed: containment cannot be decided for a path that isn't
        // there, and the daemon would canonicalize it differently.
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            register(config_with_roots(&[tmp.path()]), &tmp.path().join("ghost")).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn register_on_a_default_install_accepts_an_ordinary_directory() {
        // No `registration_roots`: the daemon floor is the only gate, so an
        // ordinary directory the operator points `cru web` at registers — the
        // repo you are working in included. It does NOT require a `~/Projects`
        // or any other configured base.
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            register(CliAppConfig::default(), tmp.path()).await,
            StatusCode::OK
        );
    }

    // "No allowlist" is not "no gate": the daemon's untrusted-caller floor
    // stands on a default install with no `[web] registration_roots` at all.
    // See `register_refuses_a_directory_that_holds_a_credential_store` below,
    // which proves this same case through a real daemon.

    #[tokio::test]
    async fn register_refuses_a_traversal_out_of_the_registration_base() {
        // `<base>/../outside` is textually prefixed by the base. Containment is
        // decided after canonicalization, so the `..` is resolved first.
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let outside = tmp.path().join("outside");
        std::fs::create_dir(&base).unwrap();
        std::fs::create_dir(&outside).unwrap();

        assert_eq!(
            register(config_with_roots(&[&base]), &base.join("../outside")).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn register_refuses_a_sibling_whose_name_extends_the_base() {
        // `<base>-evil` passes a naive string prefix test but is not inside the
        // base. Containment must compare path COMPONENTS.
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let sibling = tmp.path().join("base-evil");
        std::fs::create_dir(&base).unwrap();
        std::fs::create_dir(&sibling).unwrap();

        assert_eq!(
            register(config_with_roots(&[&base]), &sibling).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn register_accepts_a_directory_inside_the_registration_base() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let project = base.join("app");
        std::fs::create_dir_all(&project).unwrap();

        assert_eq!(
            register(base_covering_the_mock_reply(&base), &project).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn a_configured_restriction_confines_registration_to_its_roots() {
        // With `registration_roots` set, the floor is no longer the only gate:
        // a directory outside every configured root is refused even though the
        // floor would permit it.
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        assert_eq!(
            register(config_with_roots(&[&allowed]), &outside).await,
            StatusCode::FORBIDDEN
        );
    }

    // ── Untrusted-caller policy ─────────────────────────────────────────
    //
    // The daemon floor deliberately lets a LOCAL user register their own
    // dotfiles repo or `~/.config/nvim`. The web route asks the daemon to
    // register `untrusted`, so `project_manager::register_untrusted` refuses
    // a credential store or the user's config tree on top of the floor — see
    // `crates/crucible-daemon/tests/project_register.rs` for that rule
    // proved through the raw RPC path. This is a REAL daemon, not the mock:
    // the mock answers a canned reply and never applies the rule, so it
    // cannot show the route still refuses now that the check moved out of
    // this file.
    #[tokio::test]
    async fn register_refuses_a_directory_that_holds_a_credential_store() {
        let (_daemon, client) = crate::test_support::start_real_daemon_with_kilns(&[]).await;
        let app = crate::test_support::build_test_app(
            crate::test_support::build_state_with_config(client, CliAppConfig::default()),
        );

        let tmp = tempfile::tempdir().unwrap();
        let dotfiles = tmp.path().join("dotfiles");
        std::fs::create_dir_all(dotfiles.join(".ssh")).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/project/register")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "path": dotfiles.to_string_lossy() }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        // The daemon reports every `project.register` failure as
        // `INVALID_PARAMS`, so `daemon_err` maps it to 422 — the same status
        // every other daemon-enforced containment refusal reaches the browser
        // as (see `rpc_error_parts` in `error.rs`). `registration_roots`
        // refusals stay 403, because the web checks those itself before
        // asking the daemon at all.
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ── The declared shapes ─────────────────────────────────────────────
    //
    // Every route here answers `crucible_core::Project`, so the shape is the
    // core type's own and no copy of it lives in this file. The mock daemon
    // builds its replies from that same type, which makes each of these a
    // round trip: core type → JSON → route → core type → body.

    /// The fixture, as the daemon put it on the wire.
    fn sent<T: serde::Serialize>(value: T) -> serde_json::Value {
        serde_json::to_value(value).expect("the daemon's type writes JSON")
    }

    /// The registered project reaches the browser as the daemon wrote it.
    #[tokio::test]
    async fn register_project_answers_the_declared_shape() {
        let project = mock_project();
        let answered: Project = shape(
            "POST",
            "/api/project/register",
            Some(json!({ "path": std::env::temp_dir().display().to_string() })),
        )
        .await;

        assert_eq!(sent(&answered), sent(&project));
        assert_eq!(answered.name, "test-project");
    }

    /// An unnamed kiln arrives with NO `name` key.
    ///
    /// `ProjectKiln` skips the field rather than writing null, so a reader
    /// that treats absent and null alike sees neither branch it tests. The
    /// document says `name` is optional because of this.
    #[tokio::test]
    async fn an_unnamed_project_kiln_sends_no_name_key() {
        let answered: serde_json::Value = shape(
            "POST",
            "/api/project/register",
            Some(json!({ "path": std::env::temp_dir().display().to_string() })),
        )
        .await;

        let kilns = answered["kilns"].as_array().expect("a kiln array");
        assert_eq!(kilns[0]["name"], json!("test-kiln"));
        assert!(
            kilns[1].get("name").is_none(),
            "an unnamed kiln wrote a `name` key: {}",
            kilns[1]
        );
    }

    /// The project list reaches the browser as the daemon wrote it.
    #[tokio::test]
    async fn list_projects_answers_the_declared_shape() {
        let answered: Vec<Project> = shape("GET", "/api/project/list", None).await;

        assert_eq!(sent(&answered), sent(vec![mock_project()]));
        assert_eq!(answered.len(), 1);
        assert!(answered[0].repository.is_some(), "the repository was lost");
    }

    /// One project reaches the browser as the daemon wrote it.
    #[tokio::test]
    async fn get_project_answers_the_declared_shape() {
        let project = mock_project();
        let uri = format!("/api/project/get?path={}", project.path.display());
        let answered: Project = shape("GET", &uri, None).await;

        assert_eq!(sent(&answered), sent(&project));
    }

    #[tokio::test]
    async fn unregister_project_answers_the_declared_shape() {
        let answered: ProjectUnregisterResponse = shape(
            "POST",
            "/api/project/unregister",
            Some(json!({ "path": "/tmp/test-project" })),
        )
        .await;

        assert!(answered.ok);
    }
}
