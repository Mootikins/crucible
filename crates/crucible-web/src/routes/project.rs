use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, Json};
use crucible_core::config::expand_tilde;
// `Project` is crucible-core's own wire type. Every one of these routes
// answers it, so none of them keeps a copy of its shape.
use crucible_core::Project;
use crucible_daemon::project_manager::forbidden_root_reason;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
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

/// Personal secret stores. A web-registered root may neither BE one, sit
/// INSIDE one, nor directly CONTAIN one — each of those turns the root into a
/// read scope over credentials for whoever is holding the API key.
const SECRET_ENTRIES: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".kube",
    ".password-store",
    ".git-credentials",
    ".netrc",
];

/// User config/state trees. `~/.config/crucible` holds the web API key
/// (reading it is a full auth bypass) and `~/.local/share` holds keyrings.
const USER_STATE_ENTRIES: &[&str] = &[".config", ".local"];

/// Why an UNTRUSTED (HTTP) caller may not use `path` as a root, or `None`.
///
/// This is the daemon floor (`forbidden_root_reason` — catastrophic for every
/// caller) plus the rules that only make sense against a caller who is not the
/// local user sitting at the machine. A local `cru` invocation inside a
/// dotfiles repo or `~/.config/nvim` is a person registering their own work; an
/// HTTP request naming those paths is someone turning the file API into a
/// credential reader.
///
/// `path` must already be canonical, so a symlink cannot present an innocent
/// name for a forbidden target.
pub(crate) fn untrusted_root_refusal(path: &Path, home: Option<&Path>) -> Option<String> {
    if let Some(why) = forbidden_root_reason(path, home) {
        return Some(why.to_string());
    }

    let named = |entries: &[&str]| {
        path.components().any(|c| match c {
            Component::Normal(name) => entries.iter().any(|e| name == std::ffi::OsStr::new(e)),
            _ => false,
        })
    };
    if named(SECRET_ENTRIES) {
        return Some("it is a credential store or sits inside one".to_string());
    }
    if named(USER_STATE_ENTRIES) {
        return Some("it is inside the user's config/state tree".to_string());
    }
    // `symlink_metadata` rather than `exists`: a dangling or redirected
    // `.ssh` still means clients would serve that name from this root.
    SECRET_ENTRIES
        .iter()
        .find(|e| path.join(e).symlink_metadata().is_ok())
        .map(|entry| format!("it holds the credential store {entry}"))
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

fn refuse(path: &Path, why: &str) -> WebError {
    WebError::Forbidden(format!(
        "Refusing to register {}: {why}. The filesystem root, your home \
         directory, credential stores and config directories are never \
         registerable over the web API; `[web] registration_roots`, when set, \
         confines registration further.",
        path.display()
    ))
}

/// Decide whether the web API may make the ALREADY-CANONICAL `path` a root.
/// `restriction` is [`registration_roots`]: `None` gates on the floor alone,
/// `Some` additionally requires containment.
pub(crate) fn check_canonical_root(
    path: &Path,
    restriction: Option<&[PathBuf]>,
) -> Result<(), WebError> {
    if let Some(why) = untrusted_root_refusal(path, dirs::home_dir().as_deref()) {
        return Err(refuse(path, &why));
    }
    if let Some(roots) = restriction {
        if !contained(path, roots) {
            return Err(refuse(
                path,
                "it is not inside a [web] registration_roots entry",
            ));
        }
    }
    Ok(())
}

/// Canonicalize a caller-supplied `path`, then [`check_canonical_root`] it.
/// Returns the canonical path so callers act on exactly what was checked.
pub(crate) fn check_root(
    path: &Path,
    restriction: Option<&[PathBuf]>,
) -> Result<PathBuf, WebError> {
    // Canonicalize before deciding, so a symlink cannot present a name inside
    // a root for a target outside it. A path that does not resolve is refused
    // rather than guessed at.
    let canonical = path
        .canonicalize()
        .map_err(|_| refuse(path, "it does not resolve to an existing directory"))?;
    check_canonical_root(&canonical, restriction)?;
    Ok(canonical)
}

/// `POST /api/project/register` — make a directory a registered project.
///
/// Registering a root also grants the file API read access to everything
/// beneath it, so the path is canonicalized and checked before AND after the
/// daemon acts: the daemon resolves a registration inside a git repo up to the
/// repo root, which can land above what was checked.
#[utoipa::path(
    post,
    path = "/api/project/register",
    request_body = ProjectPathRequest,
    responses(
        (status = 200, body = Project),
        (status = 403, description = "The web API may not make this path a root, and the body says why"),
        (status = 502, description = "The daemon could not register the project"),
    )
)]
async fn register_project(
    State(state): State<AppState>,
    Json(req): Json<ProjectPathRequest>,
) -> Result<Json<Project>, WebError> {
    let restriction = registration_roots(&state);
    let canonical = check_root(&req.path, restriction.as_deref())?;

    let project = state
        .daemon
        .project_register(&canonical)
        .await
        .daemon_err()?;

    // The daemon resolves a registration inside a git repo up to the repo
    // root, which can land ABOVE what was checked. Re-check where it actually
    // landed and undo it if it escaped the floor (or an active restriction) —
    // nothing outside stays registered. The daemon reports a canonical path,
    // so this re-checks the policy only.
    if let Err(refusal) = check_canonical_root(&project.path, restriction.as_deref()) {
        if let Err(e) = state.daemon.project_unregister(&project.path).await {
            tracing::error!(
                path = %project.path.display(),
                error = %e,
                "Failed to roll back an out-of-base project registration"
            );
        }
        return Err(refusal);
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
        build_mock_state_with_config, build_test_app, mock_project, shape, start_mock_daemon,
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
        let app = build_test_app(build_mock_state_with_config(client, config));
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

    #[tokio::test]
    async fn register_on_a_default_install_still_refuses_a_credential_store() {
        // "No allowlist" is not "no gate": the floor stands on a default
        // install, so a dotfiles repo holding `.ssh` is refused with no config.
        let tmp = tempfile::tempdir().unwrap();
        let dotfiles = tmp.path().join("dotfiles");
        std::fs::create_dir_all(dotfiles.join(".ssh")).unwrap();
        assert_eq!(
            register(CliAppConfig::default(), &dotfiles).await,
            StatusCode::FORBIDDEN
        );
    }

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
    // dotfiles repo or `~/.config/nvim`. An HTTP caller is not that user, so
    // the credential/config rules live here, on top of the floor.

    #[tokio::test]
    async fn register_refuses_a_directory_that_holds_a_credential_store() {
        let tmp = tempfile::tempdir().unwrap();
        let dotfiles = tmp.path().join("dotfiles");
        std::fs::create_dir_all(dotfiles.join(".ssh")).unwrap();

        assert_eq!(
            register(base_covering_the_mock_reply(tmp.path()), &dotfiles).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn register_refuses_a_credential_store_or_the_user_config_tree() {
        let home = Path::new("/home/u");

        for path in [
            "/home/u/dotfiles/.ssh",
            "/home/u/.gnupg",
            "/home/u/.aws/cli",
            "/home/u/.config/nvim",
            "/home/u/.config/crucible",
            "/home/u/.local/share",
            // The daemon floor is still part of this policy.
            "/",
            "/etc/ssl/private",
        ] {
            assert!(
                untrusted_root_refusal(Path::new(path), Some(home)).is_some(),
                "{path} must be refused for an untrusted caller"
            );
        }

        assert_eq!(
            untrusted_root_refusal(Path::new("/home/u/Projects/app"), Some(home)),
            None
        );
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
