use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use crucible_daemon::project_manager::{forbidden_root_reason, resolve_registration_root};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

pub fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/api/project/register", post(register_project))
        .route("/api/project/unregister", post(unregister_project))
        .route("/api/project/list", get(list_projects))
        .route("/api/project/get", get(get_project))
}

#[derive(Debug, Deserialize)]
struct ProjectPathRequest {
    path: PathBuf,
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

/// Directories the web API may create a NEW root under.
///
/// Three sources, all canonical:
/// - `[web] registration_roots` — explicit operator opt-in.
/// - `[scm] projects_dir` (default `~/Projects`) — the base `scm.clone`
///   already clones into, so this grants no scope cloning does not. It is what
///   makes a default install work: "add project" and picking a worktree
///   created by `git worktree add ../feature-x` both land here.
/// - The already-registered projects — a worktree or subproject of something
///   the user already trusts needs no new configuration.
///
/// Anything outside all three is refused: a root is also a read scope for the
/// file API, so widening it stays a deliberate act (`cru` inside the directory,
/// or config).
pub(crate) async fn allowed_roots(state: &AppState) -> Result<Vec<PathBuf>, WebError> {
    let home = dirs::home_dir();
    let configured = state
        .config
        .web
        .as_ref()
        .map(|w| w.registration_roots.as_slice())
        .unwrap_or_default();

    let mut roots = Vec::with_capacity(configured.len() + 1);
    let mut admit = |label: &str, root: PathBuf| {
        // A root that does not resolve cannot contain anything, and one that is
        // itself forbidden must not be honoured — otherwise
        // `registration_roots = ["/"]` would re-open the hole from config.
        let Ok(root) = root.canonicalize() else {
            tracing::debug!(root = %root.display(), "Ignoring unresolvable {label} entry");
            return;
        };
        match untrusted_root_refusal(&root, home.as_deref()) {
            Some(reason) => {
                tracing::warn!(root = %root.display(), %reason, "Ignoring forbidden {label} entry")
            }
            None => roots.push(root),
        }
    };

    for raw in configured {
        admit(
            "[web] registration_roots",
            resolve_registration_root(raw, home.as_deref()),
        );
    }
    admit(
        "[scm] projects_dir",
        crucible_daemon::scm::resolve_projects_dir(
            state
                .config
                .scm
                .as_ref()
                .and_then(|s| s.projects_dir.as_deref()),
            home.as_deref(),
        ),
    );

    // Daemon-side project paths are already canonical.
    roots.extend(
        state
            .daemon
            .project_list()
            .await
            .daemon_err()?
            .into_iter()
            .map(|p| p.path),
    );
    Ok(roots)
}

pub(crate) fn contained(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

fn refuse(path: &Path, why: &str) -> WebError {
    WebError::Forbidden(format!(
        "Refusing to register {}: {why}. A project root is also a read scope \
         for the file API, so roots are limited to [scm] projects_dir \
         (default ~/Projects), any already-registered project, and \
         [web] registration_roots.",
        path.display()
    ))
}

/// Decide whether the web API may make the ALREADY-CANONICAL `path` a root.
pub(crate) fn check_canonical_root(path: &Path, roots: &[PathBuf]) -> Result<(), WebError> {
    if let Some(why) = untrusted_root_refusal(path, dirs::home_dir().as_deref()) {
        return Err(refuse(path, &why));
    }
    if !contained(path, roots) {
        return Err(refuse(path, "it is not inside an allowed root"));
    }
    Ok(())
}

/// Canonicalize a caller-supplied `path`, then [`check_canonical_root`] it.
/// Returns the canonical path so callers act on exactly what was checked.
pub(crate) fn check_root(path: &Path, roots: &[PathBuf]) -> Result<PathBuf, WebError> {
    // Canonicalize before deciding, so a symlink cannot present a name inside
    // a root for a target outside it. A path that does not resolve is refused
    // rather than guessed at.
    let canonical = path
        .canonicalize()
        .map_err(|_| refuse(path, "it does not resolve to an existing directory"))?;
    check_canonical_root(&canonical, roots)?;
    Ok(canonical)
}

async fn register_project(
    State(state): State<AppState>,
    Json(req): Json<ProjectPathRequest>,
) -> Result<Json<crucible_core::Project>, WebError> {
    let roots = allowed_roots(&state).await?;
    let canonical = check_root(&req.path, &roots)?;

    let project = state
        .daemon
        .project_register(&canonical)
        .await
        .daemon_err()?;

    // The daemon resolves a registration inside a git repo up to the repo
    // root, which can land ABOVE the base. Check where it actually landed and
    // undo it if it escaped — nothing outside the base stays registered. The
    // roots snapshot was taken before the call, so a path that escapes was by
    // definition not already registered and rolling it back destroys nothing.
    // The daemon reports a canonical path, so this re-checks the policy only.
    if let Err(refusal) = check_canonical_root(&project.path, &roots) {
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

async fn unregister_project(
    State(state): State<AppState>,
    Json(req): Json<ProjectPathRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    state
        .daemon
        .project_unregister(&req.path)
        .await
        .daemon_err()?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<crucible_core::Project>>, WebError> {
    let projects = state.daemon.project_list().await.daemon_err()?;

    Ok(Json(projects))
}

#[derive(Debug, Deserialize)]
struct GetProjectQuery {
    path: PathBuf,
}

async fn get_project(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<GetProjectQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    match state.daemon.project_get(&query.path).await {
        Ok(Some(project)) => Ok(Json(
            serde_json::to_value(project).expect("Project serializes to JSON"),
        )),
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
    use crate::test_support::{build_mock_state_with_config, build_test_app, start_mock_daemon};
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
    async fn register_on_a_default_install_refuses_a_root_outside_the_projects_dir() {
        // Default config: no `registration_roots`, the mock daemon's
        // project.list is empty, and the projects dir defaults to
        // `~/Projects` — which a temp dir is never inside. Seeding the
        // projects dir widens the allowed set to exactly that base, not to
        // anywhere the caller names.
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            register(CliAppConfig::default(), tmp.path()).await,
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

    /// Config whose `[scm] projects_dir` is `dir` and whose
    /// `registration_roots` is empty — i.e. a DEFAULT install, except that the
    /// projects dir is pointed at a temp dir instead of `~/Projects`.
    fn config_with_projects_dir(dir: &Path) -> CliAppConfig {
        CliAppConfig {
            scm: Some(crucible_core::config::ScmConfig {
                projects_dir: Some(dir.to_string_lossy().to_string()),
                ..crucible_core::config::ScmConfig::default()
            }),
            ..CliAppConfig::default()
        }
    }

    #[tokio::test]
    async fn register_accepts_a_worktree_beside_the_repo_in_the_projects_dir() {
        // `git worktree add ../feature-x` from `<projects_dir>/repo` lands at
        // `<projects_dir>/feature-x` — outside every registered project. On a
        // default install (`registration_roots` empty) this used to 403 and
        // name a config key documented nowhere, which broke the web UI's "add
        // project" button and `RootDropdown.selectWorktreeRoot`.
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("Projects");
        let worktree = projects.join("feature-x");
        std::fs::create_dir_all(&worktree).unwrap();

        let mut config = config_with_projects_dir(&projects);
        // The mock daemon always answers with `/tmp/test-project`, which is
        // containment-checked too.
        config.web = Some(WebConfig {
            registration_roots: vec![std::env::temp_dir().to_string_lossy().to_string()],
            ..WebConfig::default()
        });

        assert_eq!(register(config, &worktree).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn register_refuses_a_path_outside_the_projects_dir() {
        // Seeding from `[scm] projects_dir` grants exactly the base `scm.clone`
        // already writes into — nothing wider.
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("Projects");
        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        assert_eq!(
            register(config_with_projects_dir(&projects), &outside).await,
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
}
