use super::helpers::{note_to_file_json, reject_path_traversal, validate_file_within_kiln};
use crate::routes::session::daemon_shape;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::response::{IntoResponse, Response};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    Json,
};
use crucible_core::config::{read_project_config, ProjectFileAccess};
use crucible_core::note_edit::{disk_hash, AnchoredEdit, EditRefusal};
use crucible_core::note_merge::Region;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn kiln_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_kiln_files))
        .routes(routes!(list_kiln_notes))
        .routes(routes!(kiln_graph))
        .routes(routes!(get_kiln_file, put_kiln_file, patch_kiln_file))
        .routes(routes!(get_raw_file))
}

// =========================================================================
// Query / Request types
// =========================================================================

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct KilnPathQuery {
    /// Absolute path of the kiln to read.
    #[param(value_type = String)]
    kiln: PathBuf,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct FilePathQuery {
    /// ABSOLUTE path of the file. Containment against an open kiln or a
    /// readable project is enforced by the handler, not by this shape.
    path: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PutFileRequest {
    path: String,
    content: String,
    /// The hash the caller read. Absent keeps the blind overwrite; present
    /// refuses with 409 and the current hash when the file moved on.
    #[serde(default)]
    base_hash: Option<String>,
    /// The text the caller read, whose hash is `base_hash`. Present, a stale
    /// base is merged against the disk instead of refused: the caller loses
    /// its edit otherwise, and it is the only party that holds the text its
    /// edit was made from. Absent, the refusal stands.
    #[serde(default)]
    base_text: Option<String>,
}

/// `PATCH /api/kiln/file` — change a few lines, not the whole file.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PatchFileRequest {
    path: String,
    edits: Vec<AnchoredEdit>,
    /// The disk hash the caller last read. Present, it gates: the batch is
    /// refused with 409 and `stale_base: true` when the file moved on, even
    /// when every anchor applies. Absent, the anchors alone decide; the outbox
    /// replay sends no base, because it edits the note's current text.
    #[serde(default)]
    base_hash: Option<String>,
}

// =========================================================================
// Handlers
// =========================================================================

/// One entry of a kiln's file listing.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct FileEntryRow {
    /// The file stem, or the whole path when the stem is not UTF-8.
    name: String,
    /// RELATIVE to the kiln root.
    path: String,
    /// Always `false`: this listing walks the note index, which holds files.
    /// The key stays because the file tree reads one entry type for every
    /// source, and `GET /api/fs/list` does report directories.
    is_dir: bool,
}

/// What `GET /api/kiln/files` and `GET /api/kiln/notes` both answer.
///
/// One type for two routes because they answer the same projection of the same
/// listing. The key is `files` on both, including on the one named for notes.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct KilnFilesResponse {
    files: Vec<FileEntryRow>,
}

/// `GET /api/kiln/files?kiln=<path>` — list notes in a kiln as file entries.
#[utoipa::path(
    get,
    path = "/api/kiln/files",
    params(KilnPathQuery),
    responses(
        (status = 200, body = KilnFilesResponse),
        (status = 502, description = "The daemon could not list the notes, or answered a shape this route cannot read"),
    )
)]
async fn list_kiln_files(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<KilnFilesResponse>, WebError> {
    Ok(Json(kiln_file_listing(&state, &query.kiln).await?))
}

/// `GET /api/kiln/notes?kiln=<path>` — list notes in a kiln with metadata.
#[utoipa::path(
    get,
    path = "/api/kiln/notes",
    params(KilnPathQuery),
    responses(
        (status = 200, body = KilnFilesResponse),
        (status = 502, description = "The daemon could not list the notes, or answered a shape this route cannot read"),
    )
)]
async fn list_kiln_notes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<KilnFilesResponse>, WebError> {
    Ok(Json(kiln_file_listing(&state, &query.kiln).await?))
}

/// The listing both routes answer. One body, so the two cannot drift apart
/// the way two copies of the same projection did.
async fn kiln_file_listing(state: &AppState, kiln: &Path) -> Result<KilnFilesResponse, WebError> {
    let notes = state.daemon.list_notes(kiln, None).await.daemon_err()?;

    let files: Vec<serde_json::Value> = notes.into_iter().map(note_to_file_json).collect();

    Ok(KilnFilesResponse {
        files: daemon_shape(serde_json::Value::Array(files), "note.list")?,
    })
}

/// `GET /api/kiln/graph?kiln=<path>` — the full note-link graph of a kiln.
///
/// Returns the daemon's `kiln.graph` result verbatim:
/// `{ notes: [{ path, title, tags }], links: [{ source, target, resolved }] }`.
#[utoipa::path(
    get,
    path = "/api/kiln/graph",
    params(KilnPathQuery),
    responses(
        (status = 200, body = KilnGraphResponse),
        (status = 502, description = "The daemon could not build the graph, or answered a shape this route cannot read"),
    )
)]
async fn kiln_graph(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<KilnGraphResponse>, WebError> {
    let graph = state.daemon.kiln_graph(&query.kiln).await.daemon_err()?;
    Ok(Json(daemon_shape(graph, "kiln.graph")?))
}

/// One node of the note-link graph.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct GraphNoteRow {
    /// Kiln-relative, and the value a resolved link's `target` joins against.
    path: String,
    /// Never empty: the daemon falls back to the file stem.
    title: String,
    tags: Vec<String>,
}

/// One edge of the note-link graph.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct GraphLinkRow {
    /// The linking note's path. Always a `path` in `notes`.
    source: String,
    /// The linked note's path when `resolved`; otherwise the target as it was
    /// written, which names no note.
    target: String,
    /// Whether `target` resolves to a note the caller can see.
    resolved: bool,
}

/// What `GET /api/kiln/graph` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct KilnGraphResponse {
    notes: Vec<GraphNoteRow>,
    links: Vec<GraphLinkRow>,
}

/// `GET /api/kiln/file?path=<path>` — read a file's content.
///
/// The path must reside within an open kiln; otherwise the request is rejected.
#[utoipa::path(
    get,
    path = "/api/kiln/file",
    params(FilePathQuery),
    responses(
        (status = 200, body = KilnFileResponse),
        (status = 404, description = "No open kiln or readable project holds this path, or the file is not there"),
        (status = 415, description = "The file is not text; fetch it from `/api/file/raw`"),
        (status = 422, description = "The path carries a traversal sequence, or escapes its root"),
        (status = 502, description = "The daemon could not list the roots"),
    )
)]
async fn get_kiln_file(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FilePathQuery>,
) -> Result<Json<KilnFileResponse>, WebError> {
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
    let content = read_text_file(&canonical_file).await?;

    // The hash of the bytes just read, so a later PATCH can say whether the
    // file moved on. Not the index's hash, which lags a save.
    Ok(Json(KilnFileResponse {
        content_hash: disk_hash(&content),
        content,
    }))
}

/// What `GET /api/kiln/file` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct KilnFileResponse {
    /// BLAKE3 of the bytes just read, so a later write can say whether the
    /// file moved on. NOT the index's hash, which lags a save.
    content_hash: String,
    content: String,
}

/// Read a file this endpoint is able to represent, or say which way it failed.
///
/// `read_to_string` reports "not valid UTF-8" as an ordinary [`io::Error`], and
/// mapping every error from it to `NotFound` turned every image in the tree
/// into a 404 whose body read `File not found: stream did not contain valid
/// UTF-8` — a status claiming the file is absent, a message claiming it is
/// not, and a real file on disk that `/api/file/raw` serves without complaint.
/// Clients branch on the status, so that is the half that has to be true.
async fn read_text_file(path: &Path) -> Result<String, WebError> {
    match fs::read_to_string(path).await {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            Err(WebError::UnsupportedMediaType(format!(
                "{} is not a text file; fetch it from /api/file/raw",
                path.display()
            )))
        }
        Err(e) => Err(WebError::NotFound(format!("File not found: {e}"))),
    }
}

/// `GET /api/file/raw?path=<path>` — serve a file's raw bytes. Same
/// containment as reading via `/api/kiln/file` (kiln, or a project whose
/// `project_files` policy permits reads); used to load the media that markdown
/// and canvas cards reference by path (e.g. a README's `assets/demo.gif`).
///
/// The content type is NOT simply the guess: see [`raw_file_response`], which
/// serves media as itself (sandboxing the one scriptable media type) and forces
/// everything else to download.
#[utoipa::path(
    get,
    path = "/api/file/raw",
    params(FilePathQuery),
    responses(
        (
            status = 200,
            description = "The file's bytes. Media types the browser cannot run script from are served as themselves; everything else downloads as `application/octet-stream`.",
            content_type = "application/octet-stream",
            body = String,
        ),
        (status = 404, description = "No open kiln or readable project holds this path, or the file is not there"),
        (status = 422, description = "The path carries a traversal sequence, or escapes its root"),
        (status = 502, description = "The daemon could not list the roots"),
    )
)]
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

/// What a write answers when it lands.
///
/// One type for `PUT` and `PATCH`, because the daemon builds one object for
/// both: a patch never merges, so it writes no `merged` key, and a whole
/// write always does.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct FileWriteResponse {
    /// Always `true` here. A `false` carries a 409 and one of the
    /// [`FileWriteConflict`] shapes instead.
    ok: bool,
    /// Whether the caller's base was stale and its text was merged with the
    /// disk. Absent on a `PATCH`, which never merges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    merged: Option<bool>,
    /// What was written, present only when `merged` is `true`. The caller
    /// holds it nowhere else: its own text is not what landed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    /// BLAKE3 of what was written, so the caller needs no second read to
    /// learn the hash its next write must name.
    content_hash: String,
}

/// The error envelope a bare stale-base refusal carries.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct WriteErrorRow {
    /// The HTTP status, repeated in the body.
    code: u16,
    message: String,
}

/// What a write answers when it refuses with 409.
///
/// Untagged, and the variant order is load-bearing: serde takes the first that
/// fits. Each arm names a field the others do not have — `regions` for a merge
/// that could not settle, `failed` for a refused patch, `error` for a bare
/// refusal — so the three are disjoint and
/// `a_conflict_reads_back_as_the_arm_that_was_sent` asserts each direction.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
enum FileWriteConflict {
    /// A whole write whose base was stale and whose merge left a region the
    /// two sides disagree about. Nothing was written.
    Merge {
        /// Always `false`.
        ok: bool,
        /// BLAKE3 of the file as it is on disk NOW.
        current_hash: String,
        /// The disk's text, so the caller can resolve without a second read
        /// that would race the same way.
        current_content: String,
        /// The merge's own text, conflicts included.
        merged_content: String,
        /// Every cluster the two sides disagree about.
        regions: Vec<Region>,
        /// Always `true`.
        stale_base: bool,
    },
    /// A patch the daemon refused. `failed` is empty when the base alone
    /// refused the batch, before any anchor was read.
    Patch {
        /// Always `false`.
        ok: bool,
        current_hash: String,
        failed: Vec<EditRefusal>,
        /// Whether the file moved on since `base_hash` was read. `false` means
        /// the anchors themselves refused.
        stale_base: bool,
    },
    /// A whole write whose base was stale and which sent no `base_text`, so
    /// there was nothing to merge with. Nothing was written.
    Refused {
        /// Always `false`.
        ok: bool,
        current_hash: String,
        error: WriteErrorRow,
    },
}

/// The daemon owns containment, policy, compare, merge and write.
#[utoipa::path(
    put,
    path = "/api/kiln/file",
    request_body = PutFileRequest,
    responses(
        (status = 200, body = FileWriteResponse),
        (status = 403, description = "The root refuses writes"),
        (status = 404, description = "The path is in no open kiln and no registered project"),
        (status = 409, body = FileWriteConflict, description = "The file moved on since `base_hash` was read"),
        (status = 415, description = "The file on disk is not UTF-8 text"),
        (status = 422, description = "The path is invalid, escapes its root, or the content is too large"),
        (status = 500, description = "The write itself failed"),
        (status = 502, description = "The daemon could not be reached"),
    )
)]
async fn put_kiln_file(
    State(state): State<AppState>,
    Json(req): Json<PutFileRequest>,
) -> Result<Response, WebError> {
    use crucible_core::file_write::{FileChange, FileWriteRequest};
    let answer = state
        .daemon
        .fs_write(&FileWriteRequest {
            path: req.path,
            change: FileChange::Put {
                content: req.content,
                base_hash: req.base_hash,
                base_text: req.base_text,
            },
        })
        .await
        .daemon_err()?;
    write_response(answer)
}

#[utoipa::path(
    patch,
    path = "/api/kiln/file",
    request_body = PatchFileRequest,
    responses(
        (status = 200, body = FileWriteResponse),
        (status = 403, description = "The root refuses writes"),
        (status = 404, description = "The path is in no open kiln and no registered project, or the file is not there"),
        (status = 409, body = FileWriteConflict, description = "The file moved on, or an anchor did not apply"),
        (status = 415, description = "The file on disk is not UTF-8 text"),
        (status = 422, description = "The path is invalid, escapes its root, or the batch is empty"),
        (status = 500, description = "The write itself failed"),
        (status = 502, description = "The daemon could not be reached"),
    )
)]
async fn patch_kiln_file(
    State(state): State<AppState>,
    Json(req): Json<PatchFileRequest>,
) -> Result<Response, WebError> {
    use crucible_core::file_write::{FileChange, FileWriteRequest};
    let answer = state
        .daemon
        .fs_write(&FileWriteRequest {
            path: req.path,
            change: FileChange::Patch {
                edits: req.edits,
                base_hash: req.base_hash,
            },
        })
        .await
        .daemon_err()?;
    write_response(answer)
}

/// Translate domain failures into the existing HTTP contract.
pub(super) fn check_write(answer: &serde_json::Value) -> Result<(), WebError> {
    let message = answer["message"]
        .as_str()
        .unwrap_or("File write failed")
        .to_owned();
    match answer["failure"].as_str() {
        Some("invalid") => Err(WebError::Validation(message)),
        Some("not_found") => Err(WebError::NotFound(message)),
        Some("forbidden") => Err(WebError::Forbidden(message)),
        Some("unsupported") => Err(WebError::UnsupportedMediaType(message)),
        Some(_) => Err(WebError::Internal(message)),
        None if answer["ok"] == true => Ok(()),
        None => Err(WebError::StaleBase {
            current_hash: answer["current_hash"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        }),
    }
}

fn write_response(answer: serde_json::Value) -> Result<Response, WebError> {
    match check_write(&answer) {
        Ok(()) => Ok(Json(answer).into_response()),
        Err(error @ WebError::StaleBase { .. })
            if answer.get("regions").is_none() && answer.get("failed").is_none() =>
        {
            Ok(error.into_response())
        }
        Err(WebError::StaleBase { .. }) => Ok((StatusCode::CONFLICT, Json(answer)).into_response()),
        Err(error) => Err(error),
    }
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

    if let Some(root) = resolve_enclosing_root(file_path, &kilns, &projects) {
        return Ok(root);
    }

    // A project-less session works in the folder the daemon made for it,
    // which no registry lists. Asked only after the registries miss, so the
    // ordinary read costs no extra round trip. Read-write like a project with
    // no policy file: nothing in a scratch folder can say otherwise.
    let session_folders: Vec<(PathBuf, ProjectFileAccess)> = state
        .daemon
        .session_list(None, None, None, None, Some(true))
        .await
        .daemon_err()?
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.get("workspace").and_then(serde_json::Value::as_str))
        .map(|w| (PathBuf::from(w), ProjectFileAccess::ReadWrite))
        .collect();

    resolve_enclosing_root(file_path, &[], &session_folders)
        .ok_or_else(|| WebError::NotFound("File not within any open kiln".to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::helpers::{
        reject_path_traversal, validate_parent_within_kiln, validate_write_target_within_kiln,
    };
    use super::*;
    use crate::test_support::{
        arb_safe_path, arb_traversal_path, request_json_in_kilns, shape, shape_in_kilns, survives,
    };
    use crucible_daemon::rpc_client::NoteListRow;
    use proptest::prelude::*;
    use tempfile::{tempdir, TempDir};

    #[cfg(unix)]
    use std::os::unix::fs::symlink as symlink_dir;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir;

    // =====================================================================
    // Each route answers the shape it declares
    // =====================================================================

    /// A kiln on disk holding one note, so the file routes have a real root
    /// and real bytes to work with. Answers the kiln, the note's absolute
    /// path, and the hash of what was written.
    async fn kiln_with_note(text: &str) -> (TempDir, String, String) {
        let kiln = TempDir::new().unwrap();
        let note = kiln.path().join("Seed.md");
        tokio::fs::write(&note, text).await.unwrap();
        (kiln, note.to_string_lossy().into_owned(), disk_hash(text))
    }

    #[tokio::test]
    async fn list_kiln_files_answers_the_declared_shape() {
        let listing: KilnFilesResponse =
            shape("GET", "/api/kiln/files?kiln=/daemon/kiln", None).await;

        assert_eq!(listing.files.len(), 2);
        assert_eq!(listing.files[0].name, "Kilns");
        assert_eq!(listing.files[0].path, "notes/kilns.md");
        assert!(
            !listing.files[0].is_dir,
            "this listing walks the note index, which holds no directories"
        );
    }

    /// The notes route answers the same projection under the same key, so one
    /// reply type serves both. A change to either must therefore move both.
    #[tokio::test]
    async fn list_kiln_notes_answers_the_same_shape_under_the_same_key() {
        let files: KilnFilesResponse =
            shape("GET", "/api/kiln/files?kiln=/daemon/kiln", None).await;
        let notes: KilnFilesResponse =
            shape("GET", "/api/kiln/notes?kiln=/daemon/kiln", None).await;

        assert_eq!(
            serde_json::to_value(&files).unwrap(),
            serde_json::to_value(&notes).unwrap()
        );
    }

    #[tokio::test]
    async fn kiln_graph_answers_the_declared_shape() {
        let graph: KilnGraphResponse =
            shape("GET", "/api/kiln/graph?kiln=/daemon/kiln", None).await;

        assert_eq!(graph.notes.len(), 2);
        assert_eq!(graph.notes[0].path, "Alpha.md");
        assert_eq!(graph.notes[0].title, "Alpha");
        assert_eq!(graph.notes[0].tags, ["rust"]);

        // A resolved edge names a note in `notes`; a dangling one names the
        // target as it was written and resolves to nothing.
        assert!(graph.links[0].resolved);
        assert_eq!(graph.links[0].target, "Beta.md");
        assert!(!graph.links[1].resolved);
        assert_eq!(graph.links[1].target, "ghost");
    }

    #[tokio::test]
    async fn get_kiln_file_answers_the_declared_shape() {
        let text = "# Seed\n\nBody.\n";
        let (kiln, note, hash) = kiln_with_note(text).await;

        let answer: KilnFileResponse = shape_in_kilns(
            "GET",
            &format!("/api/kiln/file?path={note}"),
            None,
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(answer.content, text);
        assert_eq!(
            answer.content_hash, hash,
            "the hash is of the bytes just read, not of the index's copy"
        );
    }

    #[tokio::test]
    async fn put_kiln_file_answers_the_declared_shape() {
        let (kiln, note, hash) = kiln_with_note("before\n").await;

        let answer: FileWriteResponse = shape_in_kilns(
            "PUT",
            "/api/kiln/file",
            Some(serde_json::json!({
                "path": note,
                "content": "after\n",
                "base_hash": hash,
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert!(answer.ok);
        assert_eq!(answer.merged, Some(false), "the base was current");
        assert_eq!(
            answer.content, None,
            "nothing was merged, so nothing to send back"
        );
        assert_eq!(answer.content_hash, disk_hash("after\n"));
    }

    #[tokio::test]
    async fn patch_kiln_file_answers_the_declared_shape() {
        let (kiln, note, hash) = kiln_with_note("- [ ] task\n").await;

        let answer: FileWriteResponse = shape_in_kilns(
            "PATCH",
            "/api/kiln/file",
            Some(serde_json::json!({
                "path": note,
                "base_hash": hash,
                "edits": [{ "expect": "- [ ] task", "replace": "- [x] task" }],
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert!(answer.ok);
        assert_eq!(
            answer.merged, None,
            "a patch never merges, so it writes no `merged` key"
        );
        assert_eq!(answer.content_hash, disk_hash("- [x] task\n"));
    }

    #[tokio::test]
    async fn get_raw_file_answers_the_bytes_and_not_json() {
        let kiln = TempDir::new().unwrap();
        let shot = kiln.path().join("shot.png");
        // A real PNG signature: 0x89 is not valid UTF-8 in any position, so a
        // text-only reader cannot serve this and JSON cannot carry it.
        let bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        tokio::fs::write(&shot, bytes).await.unwrap();

        let (_mock, client) =
            crate::test_support::start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()])
                .await;
        let app =
            crate::test_support::build_test_app(crate::test_support::build_mock_state(client));

        use tower::ServiceExt;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/file/raw?path={}", shot.display()))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/png"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), bytes);
    }

    // =====================================================================
    // The conflict arms
    // =====================================================================

    /// A whole write whose base is stale and that sent no `base_text` has
    /// nothing to merge with, so it refuses with the bare arm.
    #[tokio::test]
    async fn a_stale_whole_write_answers_the_refused_arm() {
        let (kiln, note, _) = kiln_with_note("on disk\n").await;

        let (status, body) = request_json_in_kilns(
            "PUT",
            "/api/kiln/file",
            Some(serde_json::json!({
                "path": note,
                "content": "mine\n",
                "base_hash": "0".repeat(64),
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        let conflict: FileWriteConflict = serde_json::from_value(body.clone())
            .unwrap_or_else(|e| panic!("the conflict type cannot read the body: {e}\n{body}"));
        let FileWriteConflict::Refused {
            ok,
            current_hash,
            error,
        } = conflict
        else {
            panic!("expected the bare refusal arm, got {body}");
        };
        assert!(!ok);
        assert_eq!(current_hash, disk_hash("on disk\n"));
        assert_eq!(error.code, 409);
        assert_eq!(
            tokio::fs::read_to_string(&note).await.unwrap(),
            "on disk\n",
            "the file is untouched"
        );
    }

    /// A whole write that sent its `base_text` is merged instead, and a
    /// cluster both sides changed comes back as a region.
    #[tokio::test]
    async fn a_merge_that_cannot_settle_answers_the_merge_arm() {
        let (kiln, note, _) = kiln_with_note("line one\ntheirs\n").await;
        let base = "line one\nbase\n";

        let (status, body) = request_json_in_kilns(
            "PUT",
            "/api/kiln/file",
            Some(serde_json::json!({
                "path": note,
                "content": "line one\nours\n",
                "base_hash": disk_hash(base),
                "base_text": base,
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        let conflict: FileWriteConflict = serde_json::from_value(body.clone())
            .unwrap_or_else(|e| panic!("the conflict type cannot read the body: {e}\n{body}"));
        let FileWriteConflict::Merge {
            current_content,
            regions,
            stale_base,
            ..
        } = conflict
        else {
            panic!("expected the merge arm, got {body}");
        };
        assert!(stale_base);
        assert_eq!(current_content, "line one\ntheirs\n");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].ours, "ours\n");
        assert_eq!(regions[0].theirs, "theirs\n");
    }

    /// A patch whose anchor does not apply comes back with the refusal, which
    /// names the edit and why.
    #[tokio::test]
    async fn a_patch_whose_anchor_misses_answers_the_patch_arm() {
        let (kiln, note, _) = kiln_with_note("- [ ] task\n").await;

        let (status, body) = request_json_in_kilns(
            "PATCH",
            "/api/kiln/file",
            Some(serde_json::json!({
                "path": note,
                "edits": [{ "expect": "nothing like this", "replace": "x" }],
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        let conflict: FileWriteConflict = serde_json::from_value(body.clone())
            .unwrap_or_else(|e| panic!("the conflict type cannot read the body: {e}\n{body}"));
        let FileWriteConflict::Patch {
            failed, stale_base, ..
        } = conflict
        else {
            panic!("expected the patch arm, got {body}");
        };
        assert!(!stale_base, "the anchors refused, not the base");
        assert_eq!(failed, vec![EditRefusal::NotFound { index: 0 }]);
    }

    /// The three arms are untagged, and serde takes the first that fits. Each
    /// one is written and read back, so a reordering that made one arm swallow
    /// another's body fails here rather than in a browser.
    #[test]
    fn a_conflict_reads_back_as_the_arm_that_was_sent() {
        let merge = FileWriteConflict::Merge {
            ok: false,
            current_hash: "a".repeat(64),
            current_content: "theirs\n".to_string(),
            merged_content: "ours\n".to_string(),
            regions: vec![Region {
                start_line: 2,
                end_line: 3,
                base: "base\n".to_string(),
                ours: "ours\n".to_string(),
                theirs: "theirs\n".to_string(),
            }],
            stale_base: true,
        };
        let patch = FileWriteConflict::Patch {
            ok: false,
            current_hash: "b".repeat(64),
            failed: vec![EditRefusal::Ambiguous {
                index: 1,
                matches: 3,
            }],
            stale_base: false,
        };
        let refused = FileWriteConflict::Refused {
            ok: false,
            current_hash: "c".repeat(64),
            error: WriteErrorRow {
                code: 409,
                message: "The file moved on".to_string(),
            },
        };

        for (sent, arm) in [(&merge, "merge"), (&patch, "patch"), (&refused, "refused")] {
            let wire = serde_json::to_value(sent).unwrap();
            let read: FileWriteConflict = serde_json::from_value(wire.clone())
                .unwrap_or_else(|e| panic!("the {arm} arm does not read back: {e}\n{wire}"));
            assert_eq!(
                serde_json::to_value(read).unwrap(),
                wire,
                "the {arm} arm was read as a different arm"
            );
        }
    }

    // =====================================================================
    // The rows write back what the daemon sent
    // =====================================================================

    /// A file entry survives [`FileEntryRow`], built from the daemon's own
    /// [`NoteListRow`] through the projection the route uses.
    #[test]
    fn a_file_entry_writes_back_what_note_list_sent() {
        let row = NoteListRow {
            name: "Kilns".to_string(),
            path: "notes/kilns.md".to_string(),
            title: Some("Kilns".to_string()),
            tags: vec!["knowledge".to_string()],
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            properties: Default::default(),
        };

        survives::<FileEntryRow>(&note_to_file_json(row));
    }

    #[tokio::test]
    async fn a_file_that_is_not_text_is_not_reported_as_missing() {
        // A PNG opened from the file tree used to answer
        //   404 {"message": "File not found: stream did not contain valid UTF-8"}
        // — a status that says the file is absent and a body that says it is
        // not, for a file sitting on disk that `/api/file/raw` serves fine.
        // The status is what clients branch on, so this is the half that has
        // to carry the meaning.
        let dir = tempdir().unwrap();
        let png = dir.path().join("shot.png");
        // A real PNG signature, and 0x89 is not valid UTF-8 in any position.
        tokio::fs::write(&png, b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR")
            .await
            .unwrap();

        let err = read_text_file(&png).await.expect_err("a PNG is not text");
        assert!(
            matches!(err, WebError::UnsupportedMediaType(_)),
            "expected 415, got {err:?}"
        );
        let message = err.to_string();
        assert!(
            message.contains("/api/file/raw"),
            "the error has to name the endpoint that CAN serve it: {message}"
        );
        assert!(
            !message.to_lowercase().contains("not found"),
            "the file is not missing: {message}"
        );
    }

    #[tokio::test]
    async fn a_missing_file_is_still_reported_as_missing() {
        let dir = tempdir().unwrap();
        let err = read_text_file(&dir.path().join("nope.md"))
            .await
            .expect_err("no such file");
        assert!(
            matches!(err, WebError::NotFound(_)),
            "expected 404, got {err:?}"
        );
    }

    #[tokio::test]
    async fn a_text_file_reads_back_unchanged() {
        let dir = tempdir().unwrap();
        let note = dir.path().join("note.md");
        tokio::fs::write(&note, "# hello\n\nwörld ✅\n")
            .await
            .unwrap();

        assert_eq!(
            read_text_file(&note).await.unwrap(),
            "# hello\n\nwörld ✅\n"
        );
    }

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
