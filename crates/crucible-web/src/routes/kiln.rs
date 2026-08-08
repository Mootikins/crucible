use super::helpers::{
    note_to_file_json, reject_path_traversal, validate_file_within_kiln,
    validate_write_target_within_kiln, MAX_CONTENT_SIZE,
};
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::response::{IntoResponse, Response};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    routing::get,
    Json, Router,
};
use crucible_core::config::{read_project_config, ProjectFileAccess};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;

pub fn kiln_routes() -> Router<AppState> {
    Router::new()
        .route("/api/kiln/files", get(list_kiln_files))
        .route("/api/kiln/notes", get(list_kiln_notes))
        .route("/api/kiln/graph", get(kiln_graph))
        .route("/api/kiln/file", get(get_kiln_file).put(put_kiln_file))
        .route("/api/file/raw", get(get_raw_file))
}

// =========================================================================
// Query / Request types
// =========================================================================

#[derive(Debug, Deserialize)]
struct KilnPathQuery {
    kiln: PathBuf,
}

#[derive(Debug, Deserialize)]
struct FilePathQuery {
    path: String,
}

#[derive(Debug, Deserialize)]
struct PutFileRequest {
    path: String,
    content: String,
}

// =========================================================================
// Handlers
// =========================================================================

/// `GET /api/kiln/files?kiln=<path>` — list notes in a kiln as file entries.
async fn list_kiln_files(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let notes = state
        .daemon
        .list_notes(&query.kiln, None)
        .await
        .daemon_err()?;

    let files: Vec<serde_json::Value> = notes.into_iter().map(note_to_file_json).collect();

    Ok(Json(serde_json::json!({ "files": files })))
}

/// `GET /api/kiln/notes?kiln=<path>` — list notes in a kiln with metadata.
async fn list_kiln_notes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let notes = state
        .daemon
        .list_notes(&query.kiln, None)
        .await
        .daemon_err()?;

    let notes_json: Vec<serde_json::Value> = notes.into_iter().map(note_to_file_json).collect();

    Ok(Json(serde_json::json!({ "files": notes_json })))
}

/// `GET /api/kiln/graph?kiln=<path>` — the full note-link graph of a kiln.
///
/// Returns the daemon's `kiln.graph` result verbatim:
/// `{ notes: [{ path, title, tags }], links: [{ source, target, resolved }] }`.
async fn kiln_graph(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let graph = state.daemon.kiln_graph(&query.kiln).await.daemon_err()?;
    Ok(Json(graph))
}

/// `GET /api/kiln/file?path=<path>` — read a file's content.
///
/// The path must reside within an open kiln; otherwise the request is rejected.
async fn get_kiln_file(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FilePathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    // The editor addresses files by ABSOLUTE path (a note's `path`); containment
    // is enforced below by find_enclosing_root + validate_file_within_kiln.
    reject_path_traversal(&query.path)?;

    let file_path = PathBuf::from(&query.path);
    let root = find_enclosing_root(&state, &file_path).await?;
    // Project files are readable unless the project's policy is `off` (then
    // they behave as not served — a 404, same as a path in no root at all).
    if let EnclosingRoot::Project(_, policy) = &root {
        if !policy.can_read() {
            return Err(WebError::NotFound(
                "File not within any open kiln".to_string(),
            ));
        }
    }
    let canonical_file = validate_file_within_kiln(&file_path, root.path(), &query.path)?;

    // Read the file directly. GET /api/notes/{name} (get_note_by_name) returns
    // only path/title/tags/links_to/content_hash — never a "content" field — so
    // a daemon-first content branch here was statically unreachable and a
    // footgun (it would have served stale DB text over the file bytes).
    let content = fs::read_to_string(&canonical_file)
        .await
        .map_err(|e| WebError::NotFound(format!("File not found: {e}")))?;

    Ok(Json(serde_json::json!({ "content": content })))
}

/// `GET /api/file/raw?path=<path>` — serve a file's raw bytes. Same
/// containment as reading via `/api/kiln/file` (kiln, or a project whose
/// `project_files` policy permits reads); used to load the media that markdown
/// and canvas cards reference by path (e.g. a README's `assets/demo.gif`).
///
/// The content type is NOT simply the guess: see [`raw_file_response`], which
/// serves media as itself (sandboxing the one scriptable media type) and forces
/// everything else to download.
async fn get_raw_file(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FilePathQuery>,
) -> Result<Response, WebError> {
    reject_path_traversal(&query.path)?;

    let file_path = PathBuf::from(&query.path);
    let root = find_enclosing_root(&state, &file_path).await?;
    if let EnclosingRoot::Project(_, policy) = &root {
        if !policy.can_read() {
            return Err(WebError::NotFound(
                "File not within any open kiln".to_string(),
            ));
        }
    }
    let canonical_file = validate_file_within_kiln(&file_path, root.path(), &query.path)?;

    let bytes = fs::read(&canonical_file)
        .await
        .map_err(|e| WebError::NotFound(format!("File not found: {e}")))?;

    Ok(raw_file_response(&canonical_file, bytes))
}

/// Top-level types whose every subtype the browser hands to an image, audio or
/// video decoder — a media document, with no scripting surface — rather than
/// parsing as a document. The one subtype that is also a *document* is called
/// out by [`SANDBOXED_MEDIA_TYPE`]; it is still served inline.
const INLINE_SAFE_PREFIXES: &[&str] = &["image/", "audio/", "video/"];

/// The one media type that is also a scriptable document: an SVG *navigated to*
/// (or framed) parses as a document and runs its own `<script>`. As an `<img>`
/// or `<object>` subresource — which is how the app loads it — no script runs
/// at all.
///
/// It is served inline with its real type because both a markdown
/// `<img src="diagram.svg">` and a canvas image card (`IMAGE_EXT` in
/// `components/canvas/CanvasNodeView.tsx` matches `.svg`) fetch it from here,
/// and `octet-stream` + `attachment` makes both fail silently. The document
/// case is closed instead by [`sandbox_csp`], which strips the origin rather
/// than the rendering.
const SANDBOXED_MEDIA_TYPE: &str = "image/svg+xml";

/// Denies a document built from these bytes everything it would need to matter:
/// `sandbox` with no `allow-*` token puts it in a unique opaque origin, so its
/// script cannot reach the API, the session cookie, or the app's DOM, and
/// `frame-ancestors 'none'` stops the app itself from framing it.
///
/// Applied to the SVG path (where the bytes really are a document) and to the
/// download path (where they are only a document if something ignores the
/// `Content-Disposition`).
fn sandbox_csp() -> HeaderValue {
    HeaderValue::from_static("sandbox; frame-ancestors 'none'")
}

/// Non-media types served inline, exhaustively.
///
/// `application/pdf` because a canvas card embeds one, and a PDF's own
/// scripting runs inside the viewer's sandbox with no DOM, cookie, or
/// same-origin fetch access to the embedding page. `text/plain` because it is
/// the browser's inert rendering path by definition.
const INLINE_SAFE_TYPES: &[&str] = &["application/pdf", "text/plain"];

/// The content type to serve `essence` with inline, or `None` to force a
/// download.
///
/// Kiln and project files are agent-writable and `/api/file/raw` is
/// same-origin with the API, so any file the browser parses as a document here
/// can `fetch('/api/shell/exec')` with the user's credentials already applied.
/// This is therefore an allowlist: a type gets served as itself only if the
/// browser renders it without running script on *this* origin — either because
/// it has no scripting surface at all, or because [`sandbox_csp`] takes the
/// origin away. Anything unrecognised — including a file with no extension at
/// all — falls through to the download path.
fn inline_content_type(essence: &str) -> Option<&str> {
    // Pinning the charset keeps the browser from picking one out of the bytes,
    // which is its own (historic) script-injection route.
    if essence == "text/plain" {
        return Some("text/plain; charset=utf-8");
    }
    (INLINE_SAFE_PREFIXES.iter().any(|p| essence.starts_with(p))
        || INLINE_SAFE_TYPES.contains(&essence))
    .then_some(essence)
}

/// Build the `/api/file/raw` response.
///
/// The single enforcement point for "the browser must never execute a kiln
/// file on the app origin". `nosniff` is set here as well as by the global
/// `if_not_present` layer in `server.rs` — behaviourally identical today, kept
/// deliberately so this route's guarantee does not depend on router
/// composition. It serves attacker-writable bytes, and a declared content type
/// is only binding with `nosniff`.
fn raw_file_response(path: &Path, bytes: Vec<u8>) -> Response {
    let essence = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_ascii_lowercase();

    // A type from the table above can only be a valid header value, but fall
    // through to the download path rather than assume it.
    let inline = inline_content_type(&essence).and_then(|ct| HeaderValue::from_str(ct).ok());

    let mut headers = HeaderMap::new();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    match &inline {
        Some(content_type) => {
            headers.insert(header::CONTENT_TYPE, content_type.clone());
        }
        None => {
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            // Navigating to this URL downloads the file instead of rendering it.
            headers.insert(header::CONTENT_DISPOSITION, attachment_disposition(path));
        }
    }
    // Sandbox exactly the responses that can still become a document: SVG,
    // which is one by design, and the download path, for the case where
    // something renders it anyway (a plugin, a viewer, a browser that
    // mishandles the disposition). Decoded media and the PDF viewer are left
    // with the app's own policy — `sandbox` is known to break Chrome's PDF
    // viewer, and a canvas file card embeds one.
    if inline.is_none() || essence == SANDBOXED_MEDIA_TYPE {
        headers.insert(header::CONTENT_SECURITY_POLICY, sandbox_csp());
    }

    (headers, bytes).into_response()
}

/// `Content-Disposition` for a forced download. The file name is
/// attacker-chosen, so it is reduced to `[A-Za-z0-9._-]` — dropping the
/// quotes, semicolons and CR/LF that could otherwise close the quoted string
/// or inject a second header — and omitted entirely when nothing usable
/// survives, rather than emitted empty.
fn attachment_disposition(path: &Path) -> HeaderValue {
    let name: String = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(64)
        .collect();

    if !name.contains(|c: char| c.is_ascii_alphanumeric()) {
        return HeaderValue::from_static("attachment");
    }

    HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

/// `PUT /api/kiln/file` — write content to a file within an open kiln.
async fn put_kiln_file(
    State(state): State<AppState>,
    Json(req): Json<PutFileRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Accept absolute paths (the editor saves by a note's absolute path);
    // containment is enforced below by find_enclosing_kiln + parent-within-kiln.
    reject_path_traversal(&req.path)?;

    // Security: limit content size (10 MB)
    if req.content.len() > MAX_CONTENT_SIZE {
        return Err(WebError::Validation(format!(
            "Content too large: {} bytes (max {MAX_CONTENT_SIZE})",
            req.content.len()
        )));
    }

    let file_path = PathBuf::from(&req.path);
    let root = find_enclosing_root(&state, &file_path).await?;
    // Writes obey the project policy: `read-only` → 403, `off` → 404 (as if the
    // file were not served). Kiln notes are always writable.
    if let EnclosingRoot::Project(_, policy) = &root {
        if !policy.can_write() {
            return Err(if policy.can_read() {
                WebError::Forbidden("Project files are read-only".to_string())
            } else {
                WebError::NotFound("File not within any open kiln".to_string())
            });
        }
    }

    validate_write_target_within_kiln(&file_path, root.path())?;

    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await.map_err(WebError::Io)?;
    }

    fs::write(&file_path, &req.content)
        .await
        .map_err(WebError::Io)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// A root the file endpoints may serve `file_path` from. Kilns are the
/// knowledge content and are always read-write; projects (the code/repo dir a
/// kiln lives in) obey a per-project [`ProjectFileAccess`] policy.
enum EnclosingRoot {
    Kiln(PathBuf),
    Project(PathBuf, ProjectFileAccess),
}

impl EnclosingRoot {
    /// The canonical containing directory, for containment validation.
    fn path(&self) -> &Path {
        match self {
            EnclosingRoot::Kiln(p) | EnclosingRoot::Project(p, _) => p,
        }
    }
}

/// Return the canonical root if `file_path` is inside `root` (matched against
/// both the canonical and raw forms, as daemon-reported paths may be either).
fn canonical_if_contains(file_path: &Path, root: &Path) -> Option<PathBuf> {
    let canonical = root.canonicalize().ok()?;
    (file_path.starts_with(&canonical) || file_path.starts_with(root)).then_some(canonical)
}

/// Resolve which open root encloses `file_path`. Kilns take precedence over
/// projects, so a kiln nested inside a project keeps its always-read-write
/// treatment. Daemon-free (canonicalizes on the filesystem only) so the
/// precedence and containment rules are unit-testable without a running daemon.
fn resolve_enclosing_root(
    file_path: &Path,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Option<EnclosingRoot> {
    for kiln in kilns {
        if let Some(root) = canonical_if_contains(file_path, kiln) {
            return Some(EnclosingRoot::Kiln(root));
        }
    }
    for (project, policy) in projects {
        if let Some(root) = canonical_if_contains(file_path, project) {
            return Some(EnclosingRoot::Project(root, *policy));
        }
    }
    None
}

/// Find the open kiln or registered project that contains `file_path`. The
/// project's `project_files` policy (default read-write) is loaded from its
/// `.crucible/project.toml` here so the handlers can gate read/write.
async fn find_enclosing_root(
    state: &AppState,
    file_path: &Path,
) -> Result<EnclosingRoot, WebError> {
    let kilns: Vec<PathBuf> = state
        .daemon
        .kiln_list()
        .await
        .daemon_err()?
        .iter()
        .filter_map(|v| v.get("path").and_then(|p| p.as_str()).map(PathBuf::from))
        .collect();

    let projects: Vec<(PathBuf, ProjectFileAccess)> = state
        .daemon
        .project_list()
        .await
        .daemon_err()?
        .into_iter()
        .map(|p| {
            let policy = read_project_config(&p.path)
                .map(|c| c.security.project_files)
                .unwrap_or_default();
            (p.path, policy)
        })
        .collect();

    resolve_enclosing_root(file_path, &kilns, &projects)
        .ok_or_else(|| WebError::NotFound("File not within any open kiln".to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::helpers::{reject_path_traversal, validate_parent_within_kiln};
    use super::*;
    use crate::test_support::{arb_safe_path, arb_traversal_path};
    use proptest::prelude::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::symlink as symlink_dir;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir;

    #[test]
    fn test_reject_path_traversal_rejects_dotdot() {
        assert!(reject_path_traversal("../etc/passwd").is_err());
        assert!(reject_path_traversal("foo/../../bar").is_err());
    }

    #[test]
    fn test_reject_path_traversal_rejects_null_bytes() {
        assert!(reject_path_traversal("file\0.md").is_err());
    }

    #[test]
    fn test_reject_path_traversal_allows_valid_paths() {
        assert!(reject_path_traversal("notes/daily/2024-01-15.md").is_ok());
        assert!(reject_path_traversal("subdir/note.md").is_ok());
    }

    #[test]
    fn test_reject_path_traversal_allows_absolute_paths() {
        // The kiln file routes accept absolute paths; kiln containment is
        // enforced separately by find_enclosing_kiln + within-kiln checks.
        assert!(reject_path_traversal("/home/user/kiln/note.md").is_ok());
        // ...but an absolute path with a `..` segment is still rejected.
        assert!(reject_path_traversal("/home/user/kiln/../../etc/passwd").is_err());
    }

    #[test]
    fn test_content_size_allows_exactly_ten_megabytes() {
        const MAX_SIZE: usize = 10 * 1024 * 1024;
        let content = "a".repeat(MAX_SIZE);

        assert_eq!(content.len(), MAX_SIZE);
        assert!(content.len() <= MAX_SIZE);
    }

    #[test]
    fn test_content_size_rejects_ten_megabytes_plus_one_byte() {
        const MAX_SIZE: usize = 10 * 1024 * 1024;
        let content = "a".repeat(MAX_SIZE + 1);

        assert_eq!(
            format!(
                "Content too large: {} bytes (max {MAX_SIZE})",
                content.len()
            ),
            "Content too large: 10485761 bytes (max 10485760)"
        );
        assert!(content.len() > MAX_SIZE);
    }

    #[test]
    fn symlink_escape_rejected() {
        let kiln = tempdir().expect("temp kiln");
        let outside = tempdir().expect("temp outside");

        let outside_file = outside.path().join("outside-note.md");
        std::fs::write(&outside_file, "outside").expect("write outside file");

        let link = kiln.path().join("escape-link");
        symlink_dir(outside.path(), &link).expect("create symlink to outside");

        let escaped_path = link.join("outside-note.md");
        let err =
            validate_file_within_kiln(&escaped_path, kiln.path(), &escaped_path.to_string_lossy())
                .expect_err("symlink target outside kiln must be rejected");

        match err {
            WebError::Validation(message) => {
                assert_eq!(message, "File path escapes kiln directory");
            }
            other => panic!("expected validation error, got: {other:?}"),
        }
    }

    #[test]
    fn put_kiln_file_rejects_new_file_outside_kiln() {
        let kiln = tempdir().expect("temp kiln");
        let outside = tempdir().expect("temp outside");

        let link = kiln.path().join("escape-link");
        symlink_dir(outside.path(), &link).expect("create symlink to outside");

        let new_file_path = link.join("new-note.md");
        assert!(!new_file_path.exists());

        let err = validate_parent_within_kiln(&new_file_path, kiln.path())
            .expect_err("symlinked parent outside kiln must be rejected");
        match err {
            WebError::Validation(message) => {
                assert_eq!(message, "Path escapes kiln directory");
            }
            other => panic!("expected validation error, got: {other:?}"),
        }
    }

    #[test]
    fn write_target_symlinked_final_component_rejected() {
        // KILN/evil.md is a pre-planted symlink to a file OUTSIDE the kiln. The
        // parent (the kiln root) is legitimate, so only the final-component
        // symlink check catches the escape — without it, fs::write would follow
        // the link and overwrite the outside file.
        let kiln = tempdir().expect("temp kiln");
        let outside = tempdir().expect("temp outside");

        let secret = outside.path().join("secret.md");
        std::fs::write(&secret, "original secret").expect("write secret");

        let link = kiln.path().join("evil.md");
        symlink_dir(&secret, &link).expect("plant symlink to outside file");

        let canonical_kiln = kiln.path().canonicalize().expect("canonical kiln");
        let err = validate_write_target_within_kiln(&link, &canonical_kiln)
            .expect_err("symlinked final component pointing outside the kiln must be rejected");
        match err {
            WebError::Validation(message) => assert_eq!(message, "Path escapes kiln directory"),
            other => panic!("expected validation error, got: {other:?}"),
        }

        // The guard runs before any write, so the outside file is untouched.
        assert_eq!(
            std::fs::read_to_string(&secret).expect("read secret"),
            "original secret"
        );
    }

    #[test]
    fn write_target_regular_file_within_kiln_allowed() {
        // A normal (non-symlink) file inside the kiln passes.
        let kiln = tempdir().expect("temp kiln");
        let canonical_kiln = kiln.path().canonicalize().expect("canonical kiln");
        let note = canonical_kiln.join("note.md");
        std::fs::write(&note, "hi").expect("write note");
        assert!(validate_write_target_within_kiln(&note, &canonical_kiln).is_ok());
    }

    // -- /api/file/raw: never hand back an executable document ---------------

    /// Read a header off a built response, or `""` when absent.
    fn header_of(response: &Response, name: axum::http::HeaderName) -> String {
        response
            .headers()
            .get(&name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    }

    /// Path suffixes a browser will parse as a document — or as script or CSS —
    /// on the app's own origin. Kiln and project files are agent-writable, so
    /// every one of these can be attacker-authored; none may come back with a
    /// content type that lets the browser run it here.
    ///
    /// Bypass coverage: `xhtml`/`xht`/`xml` (XML documents, XSLT-scriptable),
    /// a bare name with NO extension (mime_guess falls through), a trailing-dot
    /// name, an uppercase extension, and a double extension whose LAST
    /// component is the dangerous one.
    ///
    /// SVG is deliberately NOT here — it renders inline, sandboxed; see
    /// [`raw_file_serves_svg_inline_under_a_sandbox_csp`].
    const EXECUTABLE_SUFFIXES: &[&str] = &[
        "note.html",
        "note.htm",
        "note.xhtml",
        "note.xht",
        "note.shtml",
        "note.xml",
        "note.js",
        "note.mjs",
        "note.css",
        "note.HTML",
        "note",
        "note.",
        "note.png.html",
        "note.jpg.xhtml",
        "archive.tar.gz",
    ];

    #[test]
    fn raw_file_never_serves_an_executable_document() {
        for suffix in EXECUTABLE_SUFFIXES {
            let path = PathBuf::from(format!("/kiln/{suffix}"));
            let response =
                raw_file_response(&path, b"<script>fetch('/api/shell/exec')</script>".to_vec());

            assert_eq!(
                header_of(&response, header::CONTENT_TYPE),
                "application/octet-stream",
                "{suffix} must not be served with a type the browser renders"
            );
            assert!(
                header_of(&response, header::CONTENT_DISPOSITION).starts_with("attachment"),
                "{suffix} must be forced to download, got {:?}",
                header_of(&response, header::CONTENT_DISPOSITION)
            );
            assert_eq!(
                header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
                "nosniff",
                "{suffix} must not be sniffed back into a document type"
            );
            assert!(
                header_of(&response, header::CONTENT_SECURITY_POLICY).contains("sandbox"),
                "{suffix} must be sandboxed to an opaque origin if it is rendered anyway"
            );
        }
    }

    #[test]
    fn raw_file_serves_svg_inline_under_a_sandbox_csp() {
        // Both halves have to hold at once.
        //
        // Product: markdown `<img src="diagram.svg">` and a canvas image card
        // (`IMAGE_EXT` in components/canvas/CanvasNodeView.tsx matches `.svg`)
        // both load through this endpoint. `octet-stream` + `attachment` makes
        // an `<img>` fail its decode and the card render an onerror placeholder,
        // so the real type has to come back with no disposition.
        //
        // Security: an SVG *navigated to* is a document that runs its own
        // `<script>`. The sandbox CSP gives that document an opaque origin — no
        // API, no session cookie, no app DOM — which is the property that
        // matters. As an `<img>` subresource no script runs at all.
        for suffix in ["diagram.svg", "diagram.SVG", "diagram.png.svg"] {
            let path = PathBuf::from(format!("/kiln/{suffix}"));
            let response = raw_file_response(
                &path,
                br#"<svg xmlns="http://www.w3.org/2000/svg"><script>fetch('/api/shell/exec')</script></svg>"#.to_vec(),
            );

            assert_eq!(
                header_of(&response, header::CONTENT_TYPE),
                "image/svg+xml",
                "{suffix} must render as an image, not download"
            );
            assert_eq!(
                header_of(&response, header::CONTENT_DISPOSITION),
                "",
                "{suffix} must not be forced to download — it is a canvas image card"
            );
            assert_eq!(
                header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
                "nosniff",
                "{suffix} must not be sniffed into some other document type"
            );
            assert_eq!(
                header_of(&response, header::CONTENT_SECURITY_POLICY),
                "sandbox; frame-ancestors 'none'",
                "{suffix} must get an opaque origin if it is navigated to or framed"
            );
        }
    }

    #[test]
    fn raw_file_serves_inert_media_inline() {
        // The endpoint exists so markdown can show a README's `assets/demo.gif`.
        // These types render without ever running script on our origin, so they
        // keep their real content type — but still never get sniffed.
        for (suffix, expected) in [
            ("demo.png", "image/png"),
            ("demo.jpg", "image/jpeg"),
            ("demo.jpeg", "image/jpeg"),
            ("demo.gif", "image/gif"),
            ("demo.webp", "image/webp"),
            ("demo.avif", "image/avif"),
            ("notes.txt", "text/plain; charset=utf-8"),
            ("paper.pdf", "application/pdf"),
        ] {
            let path = PathBuf::from(format!("/kiln/{suffix}"));
            let response = raw_file_response(&path, b"\x89PNG".to_vec());

            assert_eq!(
                header_of(&response, header::CONTENT_TYPE),
                expected,
                "{suffix} should be served inline as {expected}"
            );
            assert_eq!(
                header_of(&response, header::CONTENT_DISPOSITION),
                "",
                "{suffix} is inert; it should not be forced to download"
            );
            assert_eq!(
                header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
                "nosniff",
                "{suffix} must carry nosniff so the declared type is binding"
            );
        }
    }

    #[test]
    fn raw_file_serves_canvas_media_inline() {
        // A canvas media card renders <audio>/<video> straight from this
        // endpoint (components/canvas/CanvasNodeView.tsx). `octet-stream` plus
        // nosniff makes a media element refuse to play, so these keep their
        // real type — a media document has no scripting surface to abuse.
        for (suffix, expected) in [
            ("clip.mp3", "audio/mpeg"),
            ("clip.wav", "audio/wav"),
            ("clip.ogg", "audio/ogg"),
            ("clip.flac", "audio/flac"),
            ("clip.mp4", "video/mp4"),
            ("clip.webm", "video/webm"),
            ("clip.mov", "video/quicktime"),
            ("clip.mkv", "video/x-matroska"),
        ] {
            let path = PathBuf::from(format!("/kiln/{suffix}"));
            let response = raw_file_response(&path, b"\x00\x00".to_vec());

            assert_eq!(header_of(&response, header::CONTENT_TYPE), expected);
            assert_eq!(
                header_of(&response, header::CONTENT_DISPOSITION),
                "",
                "{suffix} must stay playable, not download"
            );
            assert_eq!(
                header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
                "nosniff"
            );
        }
    }

    #[test]
    fn raw_file_double_extension_resolves_to_the_final_extension() {
        // `evil.html.png` is HTML on disk. It is served as image/png with
        // nosniff, so the browser decodes it as an image and never as a
        // document — the declared type is binding.
        let path = PathBuf::from("/kiln/evil.html.png");
        let response = raw_file_response(&path, b"<script>alert(1)</script>".to_vec());
        assert_eq!(header_of(&response, header::CONTENT_TYPE), "image/png");
        assert_eq!(
            header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
            "nosniff"
        );
    }

    #[test]
    fn raw_file_attachment_filename_cannot_inject_headers() {
        // A kiln file name is attacker-chosen. Quotes, semicolons, CR/LF and
        // non-ASCII must not reach the header value.
        let path = PathBuf::from("/kiln/ev\"il;\r\nname\u{4e2d}.html");
        let response = raw_file_response(&path, b"x".to_vec());
        let disposition = header_of(&response, header::CONTENT_DISPOSITION);

        assert_eq!(disposition, "attachment; filename=\"evilname.html\"");
        assert!(!disposition.contains('\r') && !disposition.contains('\n'));
    }

    #[test]
    fn raw_file_with_an_unnameable_filename_still_downloads() {
        // Nothing survives sanitisation, so the disposition drops the filename
        // rather than emitting an empty or malformed one — it still downloads.
        let path = PathBuf::from("/kiln/\u{4e2d}\u{6587}");
        let response = raw_file_response(&path, b"x".to_vec());
        assert_eq!(
            header_of(&response, header::CONTENT_DISPOSITION),
            "attachment"
        );
        assert_eq!(
            header_of(&response, header::CONTENT_TYPE),
            "application/octet-stream"
        );
    }

    // -- enclosing-root resolution (kiln vs project + policy) ----------------

    #[test]
    fn resolve_prefers_kiln_over_enclosing_project() {
        // A kiln nested inside a project keeps its always-read-write treatment
        // rather than inheriting the project's file policy.
        let project = tempdir().expect("temp project");
        let kiln = project.path().join("docs");
        std::fs::create_dir(&kiln).expect("mkdir kiln");
        let file = kiln.join("note.md");
        std::fs::write(&file, "n").expect("write note");

        let root = resolve_enclosing_root(
            &file,
            std::slice::from_ref(&kiln),
            &[(project.path().to_path_buf(), ProjectFileAccess::Off)],
        )
        .expect("kiln should match first");
        assert!(matches!(root, EnclosingRoot::Kiln(_)));
    }

    #[test]
    fn resolve_matches_project_and_carries_policy() {
        let project = tempdir().expect("temp project");
        let file = project.path().join("README.md");
        std::fs::write(&file, "r").expect("write readme");

        for policy in [
            ProjectFileAccess::ReadWrite,
            ProjectFileAccess::ReadOnly,
            ProjectFileAccess::Off,
        ] {
            let root =
                resolve_enclosing_root(&file, &[], &[(project.path().to_path_buf(), policy)])
                    .expect("project should match");
            match root {
                EnclosingRoot::Project(_, p) => assert_eq!(p, policy),
                other => panic!("expected project root, got a kiln: {:?}", other.path()),
            }
        }
    }

    #[test]
    fn resolve_returns_none_when_outside_every_root() {
        let project = tempdir().expect("temp project");
        let outside = tempdir().expect("temp outside");
        let file = outside.path().join("secret.md");
        std::fs::write(&file, "s").expect("write secret");

        assert!(resolve_enclosing_root(
            &file,
            &[],
            &[(project.path().to_path_buf(), ProjectFileAccess::ReadWrite)],
        )
        .is_none());
    }

    #[test]
    fn project_file_access_read_write_matrix() {
        assert!(ProjectFileAccess::ReadWrite.can_read());
        assert!(ProjectFileAccess::ReadWrite.can_write());
        assert!(ProjectFileAccess::ReadOnly.can_read());
        assert!(!ProjectFileAccess::ReadOnly.can_write());
        assert!(!ProjectFileAccess::Off.can_read());
        assert!(!ProjectFileAccess::Off.can_write());
    }

    proptest! {
        #[test]
        fn prop_traversal_paths_are_rejected(path in arb_traversal_path()) {
            prop_assert!(reject_path_traversal(&path).is_err());
        }

        #[test]
        fn prop_safe_paths_are_accepted(path in arb_safe_path()) {
            prop_assert!(reject_path_traversal(&path).is_ok());
        }

        #[test]
        fn prop_null_bytes_are_always_rejected(prefix in ".{0,32}", suffix in ".{0,32}") {
            let path = format!("{prefix}\0{suffix}");
            prop_assert!(reject_path_traversal(&path).is_err());
        }

        #[test]
        fn prop_new_file_path_traversal_rejected(file_name in "[a-zA-Z0-9_-]{1,32}\\.md") {
            let kiln = tempdir().expect("temp kiln");
            let outside = tempdir().expect("temp outside");

            let link = kiln.path().join("escape-link");
            symlink_dir(outside.path(), &link).expect("create symlink to outside");

            let new_file_path = link.join(file_name);
            prop_assume!(!new_file_path.exists());

            prop_assert!(validate_parent_within_kiln(&new_file_path, kiln.path()).is_err());
        }
    }
}
