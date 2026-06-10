//! Static asset serving with conditional compilation
//!
//! - Release builds: assets embedded via rust-embed
//! - Debug builds: served from filesystem
//! - --web-dir flag overrides both

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get_service,
    Router,
};
use rust_embed::Embed;
use tower_http::services::ServeDir;

/// Embedded assets for release builds
#[derive(Embed)]
#[folder = "web/dist"]
struct Assets;

/// Create router for serving static assets
pub fn static_routes(web_dir: Option<&str>) -> Router {
    if let Some(dir) = web_dir {
        // Explicit override: serve from specified directory
        tracing::info!("Serving static assets from: {}", dir);
        serve_from_dir(dir)
    } else if cfg!(debug_assertions) {
        // Debug mode: serve from filesystem
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist");
        tracing::info!("Debug mode: serving static assets from: {}", dir);
        serve_from_dir(dir)
    } else {
        // Release mode: serve embedded assets
        tracing::info!("Release mode: serving embedded static assets");
        serve_embedded()
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
    /// PWA installability depends on the manifest and service worker being
    /// served with the right content types. Both the embedded path (above)
    /// and the debug-mode `ServeDir` path resolve via mime_guess, so pin the
    /// resolutions here to catch a mime_guess regression on upgrade.
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
