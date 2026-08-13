//! Static asset serving
//!
//! Two sources, chosen by configuration and never by build profile:
//!
//! - default: the assets embedded in the binary via rust-embed
//! - `--static-dir` (or `[web] static_dir`): serve that directory from disk

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get_service,
    Router,
};
use rust_embed::Embed;
use tower_http::services::ServeDir;

/// Assets embedded into the binary — the only default source.
///
/// `allow_missing` because `web/dist` is a build artifact of the frontend
/// (bun), not a tracked file — so it is absent in a fresh clone. Without it
/// the derive is a hard compile error and `cargo build` fails for anyone who
/// has not run the frontend build first, which is every new user and every
/// contributor whose first command is `cargo test`. A release build still
/// embeds the real assets; a build without them serves the message in
/// [`serve_embedded`] instead of failing to compile.
///
/// `build.rs` does not build the frontend — it only reports whether one is
/// present, and fails the build when `CRUCIBLE_REQUIRE_WEB_UI` says the binary
/// must ship with it. See that file for why building it here was reverted.
#[derive(Embed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Assets;

/// Create router for serving static assets.
///
/// The asset source is configuration, not build profile: embedded by default in
/// every profile, and `--static-dir` (or `[web] static_dir`) to serve a
/// directory. A `cfg!(debug_assertions)` branch used to decide it instead, which
/// tied asset source to optimization level and baked the build machine's
/// absolute `web/dist` into the binary. See the CHANGELOG entry for the rest.
///
/// The two sources are not quite equivalent: the embedded handler falls back to
/// `index.html` for extension-less paths and `ServeDir` does not, so a deep link
/// 404s under `--static-dir` and works on the default. Harmless while the app is
/// one document at `/` — but if client-side routing ever lands, this is the one
/// place dev and production would diverge again, in the opposite direction.
pub fn static_routes(static_dir: Option<&str>) -> Router {
    match static_dir {
        Some(dir) => {
            tracing::info!("Serving static assets from: {}", dir);
            serve_from_dir(dir)
        }
        None => {
            tracing::info!("Serving embedded static assets");
            serve_embedded()
        }
    }
}

fn serve_from_dir(dir: &str) -> Router {
    Router::new().fallback_service(
        get_service(
            ServeDir::new(dir).fallback(ServeDir::new(dir).append_index_html_on_directories(true)),
        )
        .handle_error(|_| async { StatusCode::INTERNAL_SERVER_ERROR }),
    )
}

fn serve_embedded() -> Router {
    // Distinguish "built without the frontend" from "asset genuinely missing"
    // at startup rather than leaving the user a bare 404 per request. This is
    // reachable because the embed tolerates a missing `web/dist` (see
    // [`Assets`]) — a build that skipped `just web-build` gets here.
    if <Assets as Embed>::iter().next().is_none() {
        tracing::error!(
            "No embedded web assets: this binary was built without the frontend. \
             Run `just web-build` and rebuild, or pass --static-dir to serve from a directory."
        );
    }
    Router::new().fallback(embedded_handler)
}

async fn embedded_handler(req: Request<Body>) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    // Try exact path first
    if let Some(content) = <Assets as Embed>::get(path) {
        return respond_with_asset(path, content.data.to_vec());
    }

    // Try with index.html for directories
    let index_path = if path.is_empty() {
        "index.html".to_string()
    } else {
        format!("{}/index.html", path)
    };

    if let Some(content) = <Assets as Embed>::get(&index_path) {
        return respond_with_asset(&index_path, content.data.to_vec());
    }

    // SPA fallback: serve index.html for non-asset paths
    if !path.contains('.') {
        if let Some(content) = <Assets as Embed>::get("index.html") {
            return respond_with_asset("index.html", content.data.to_vec());
        }
    }

    // Not found
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not Found"))
        .expect("valid 404 response")
}

fn respond_with_asset(path: &str, data: Vec<u8>) -> Response<Body> {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(data))
        .expect("valid asset response")
}

#[cfg(test)]
mod tests {
    use super::static_routes;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    async fn get(router: axum::Router, path: &str) -> (StatusCode, String) {
        let response = router
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("valid test request"),
            )
            .await
            .expect("router is infallible");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body collects");
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// An explicit directory wins: whatever is on disk there is what ships,
    /// even for a name the embedded bundle also has.
    #[tokio::test]
    async fn explicit_static_dir_is_served_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("index.html"), "<!-- from disk -->")
            .expect("write fixture asset");

        let router = static_routes(Some(dir.path().to_str().expect("utf-8 tempdir")));

        let (status, body) = get(router, "/index.html").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "<!-- from disk -->");
    }

    /// With no override the embedded handler serves every request — including
    /// in a debug build, which this test binary is. Asserted through the 404
    /// body because that is what distinguishes the two sources: the embedded
    /// handler writes "Not Found", `ServeDir` returns an empty body. A
    /// profile-dependent asset source would fail here.
    #[tokio::test]
    async fn default_uses_embedded_assets_in_any_build_profile() {
        let (status, body) = get(static_routes(None), "/no-such-asset.xyz").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, "Not Found");
    }

    /// PWA installability depends on the manifest and service worker being
    /// served with the right content types. Both the embedded path (above)
    /// and the `--static-dir` `ServeDir` path resolve via mime_guess, so pin
    /// the resolutions here to catch a mime_guess regression on upgrade.
    #[test]
    fn pwa_assets_resolve_to_correct_mime_types() {
        let manifest = mime_guess::from_path("manifest.webmanifest")
            .first()
            .expect("webmanifest extension must be known");
        assert_eq!(manifest.essence_str(), "application/manifest+json");

        let sw = mime_guess::from_path("sw.js")
            .first()
            .expect("js extension must be known");
        assert_eq!(sw.type_(), "text");
        assert_eq!(sw.subtype(), "javascript");
    }
}
