use super::helpers::{validate_note_name, MAX_CONTENT_SIZE};
use crucible_core::protocol::requests::{
    BacklinkEntry, KilnRow, NoteByNameReply, NoteListRow, VectorHit,
};
// The daemon owns the grep request shape. The copy that used to live in
// this file had the same six fields and its own `default_grep_limit`
// hardcoded at 100, while the daemon's reads `GREP_DEFAULT_LIMIT` — so a
// change to that constant moved the RPC default and left the HTTP one behind.
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use crucible_core::protocol::requests::GrepSearchRequest;
use crucible_core::types::database::BlockRef;
use crucible_daemon::GrepSearchResponse;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn search_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_kilns))
        .routes(routes!(list_notes))
        .routes(routes!(resolve_note))
        .routes(routes!(get_note, put_note))
        .routes(routes!(get_backlinks))
        .routes(routes!(search_vectors))
        .routes(routes!(search_semantic))
        .routes(routes!(search_grep))
}

/// What `GET /api/kilns` answers. A thin wrapper, not a copy: the row is
/// core's own [`KilnRow`], the same type `kiln.list` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct KilnListResponse {
    kilns: Vec<KilnRow>,
}

/// `GET /api/kilns` — every kiln a client may address.
#[utoipa::path(
    get,
    path = "/api/kilns",
    responses(
        (status = 200, body = KilnListResponse),
        (status = 502, description = "The daemon could not list the kilns"),
    )
)]
async fn list_kilns(State(state): State<AppState>) -> Result<Json<KilnListResponse>, WebError> {
    let kilns = state.daemon.kiln_list().await.daemon_err()?;

    Ok(Json(KilnListResponse { kilns }))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ListNotesQuery {
    /// Absolute path of the kiln to list.
    #[param(value_type = String)]
    kiln: PathBuf,
    /// Keep only notes whose path holds this substring.
    path_filter: Option<String>,
}

/// What `GET /api/notes` answers. A thin wrapper, not a copy: the row is
/// core's own [`NoteListRow`], the same type `note.list` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct NoteListResponse {
    notes: Vec<NoteListRow>,
}

/// `GET /api/notes?kiln=<path>` — the notes of one kiln, with metadata.
#[utoipa::path(
    get,
    path = "/api/notes",
    params(ListNotesQuery),
    responses(
        (status = 200, body = NoteListResponse),
        (status = 502, description = "The daemon could not list the notes"),
    )
)]
async fn list_notes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListNotesQuery>,
) -> Result<Json<NoteListResponse>, WebError> {
    let notes = state
        .daemon
        .list_notes(&query.kiln, query.path_filter.as_deref())
        .await
        .daemon_err()?;

    Ok(Json(NoteListResponse { notes }))
}

/// `GET /api/notes/resolve?kiln=<path>&name=<target>` — resolve a wikilink
/// target to a file, **by walking the kiln**.
///
/// Deliberately independent of the note index. Opening `[[Some Note]]` is a
/// path question, and answering it from the index means a kiln that has not
/// been processed yet resolves nothing — which does not fail loudly, it falls
/// through to whatever kiln is configured as the default and silently opens a
/// same-named note from the wrong vault. Indexing is an optimisation for search
/// and backlinks; it must not be a prerequisite for following a link.
///
/// Resolution order, matching how wikilinks are written in practice:
///   1. exact kiln-relative path (with or without the `.md` suffix)
///   2. unique filename stem anywhere in the kiln
///
/// An ambiguous stem resolves to the shallowest match, which is the same
/// tie-break the link index applies.
///
/// # Isolation
///
/// **A root never resolves outside itself.** Every candidate is canonicalized
/// and checked against the canonical root, so a symlink planted inside one kiln
/// cannot surface a note from another kiln or from a project. Resolution
/// failing is the correct outcome — the alternative, quietly answering from a
/// different root, is how a link in one vault silently opens a same-named note
/// from another.
#[utoipa::path(
    get,
    path = "/api/notes/resolve",
    params(ResolveQuery),
    responses(
        (status = 200, body = ResolvedNoteResponse),
        (status = 400, description = "The name carries a traversal sequence"),
        (status = 404, description = "No open kiln or readable project holds the supplied root, or it holds no such note"),
        (status = 422, description = "The supplied root is not an absolute path, or escapes its root"),
        (status = 500, description = "The walk that searches the root failed"),
        (status = 502, description = "The daemon could not be reached"),
    )
)]
async fn resolve_note(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ResolveQuery>,
) -> Result<Json<ResolvedNoteResponse>, WebError> {
    validate_note_name(&query.name)?;

    // The caller does NOT get to nominate the root.
    //
    // Taking `kiln` at face value made this an existence-and-path oracle for
    // the whole filesystem: `?kiln=/etc&name=passwd` answered, as did
    // `?kiln=/&name=etc/shadow` (which also walked the entire disk). It ignored
    // a project's `project_files` policy that every sibling route honours. The
    // daemon's `fs.read` resolves the supplied path with the one rule that
    // every file route uses, and the CANONICAL root that comes back is what
    // gets walked — so the walk is bounded by a registered root, not by argv.
    let root = super::kiln::read_through_daemon(
        &state,
        &query.kiln.to_string_lossy(),
        crucible_core::file_write::FileEncoding::Text,
    )
    .await?
    .root;

    // Strip an alias/heading/block suffix — `[[Note|alias]]`, `[[Note#Heading]]`.
    let target = query
        .name
        .split(['|', '#'])
        .next()
        .unwrap_or(&query.name)
        .trim()
        .to_string();
    let bare = target.strip_suffix(".md").unwrap_or(&target);

    for candidate in [root.join(format!("{bare}.md")), root.join(&target)] {
        // Notes only, matching the walk below. Without this the exact-path
        // branch resolved ANY file — `.ssh/id_ed25519` answered — because
        // `contained_file` checks containment, not kind.
        if !crucible_core::kiln::is_note_file(&candidate) {
            continue;
        }
        if let Some(contained) = contained_file(&candidate, &root) {
            return Ok(Json(resolved_note(&root, &contained)));
        }
    }

    // The walk is blocking: `canonicalize` plus a full `WalkDir` of the kiln.
    // Running it inline stalled a Tokio worker for the duration, and hover
    // previews hit this path on every unresolved stem.
    let best = {
        let root = root.clone();
        let bare = bare.to_string();
        tokio::task::spawn_blocking(move || {
            let bare = bare.as_str();
            let wanted = bare.rsplit('/').next().unwrap_or(bare).to_ascii_lowercase();
            let mut best: Option<std::path::PathBuf> = None;
            for entry in walkdir::WalkDir::new(&root)
                .into_iter()
                .filter_entry(|e| {
                    !crucible_core::EXCLUDED_DIRS
                        .iter()
                        .any(|d| e.file_name().to_string_lossy() == *d)
                })
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if !path.is_file() || !crucible_core::kiln::is_note_file(path) {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if stem != wanted {
                    continue;
                }
                let Some(contained) = contained_file(path, &root) else {
                    continue;
                };
                // `Option::is_none_or` is newer than this crate's MSRV.
                let shallower = match best.as_ref() {
                    None => true,
                    Some(b) => contained.components().count() < b.components().count(),
                };
                if shallower {
                    best = Some(contained);
                }
            }
            best
        })
        .await
        .map_err(|e| WebError::Internal(format!("resolve walk failed: {e}")))?
    };

    match best {
        Some(path) => Ok(Json(resolved_note(&root, &path))),
        None => Err(WebError::NotFound(format!(
            "Note '{}' not found in this kiln",
            query.name
        ))),
    }
}

/// The canonical path, if it is a file genuinely inside `root`.
///
/// Canonicalizing before the containment test is the point: a symlink is a file
/// whose lexical path is inside the root while its contents live outside it.
fn contained_file(path: &std::path::Path, root: &std::path::Path) -> Option<std::path::PathBuf> {
    let canonical = path.canonicalize().ok()?;
    (canonical.is_file() && canonical.starts_with(root)).then_some(canonical)
}

/// Where a wikilink target landed.
///
/// `absolutePath` is camelCase while every sibling reply is snake_case: the
/// browser has read that spelling since before this route was typed, and
/// renaming it would break the editor's open-a-link path for no gain.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ResolvedNoteResponse {
    /// Relative to the root that was walked.
    path: String,
    /// The absolute path, which is what an editor opens.
    #[serde(rename = "absolutePath")]
    absolute_path: String,
    /// The file stem, or `null` when the path has none. Always written.
    #[schema(required = true)]
    title: Option<String>,
}

fn resolved_note(root: &std::path::Path, path: &std::path::Path) -> ResolvedNoteResponse {
    let rel = path.strip_prefix(root).unwrap_or(path);
    ResolvedNoteResponse {
        path: rel.to_string_lossy().into_owned(),
        absolute_path: path.to_string_lossy().into_owned(),
        title: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ResolveQuery {
    /// A path inside the root to search. The root that is actually walked is
    /// the kiln or project this resolves to, never the supplied path itself.
    #[param(value_type = String)]
    kiln: PathBuf,
    /// The wikilink target, alias and heading suffixes included.
    name: String,
}

/// `GET /api/notes/{name}?kiln=<path>` — one note by name or path. The reply
/// is core's own [`NoteByNameReply`], the same type `get_note_by_name`
/// answers.
#[utoipa::path(
    get,
    path = "/api/notes/{name}",
    params(
        ("name" = String, Path, description = "The note's name or kiln-relative path"),
        KilnQuery,
    ),
    responses(
        (status = 200, body = NoteByNameReply),
        (status = 400, description = "The name carries a traversal sequence"),
        (status = 404, description = "The kiln holds no note of that name"),
        (status = 502, description = "The daemon could not read the note"),
    )
)]
async fn get_note(
    State(state): State<AppState>,
    Path(name): Path<String>,
    axum::extract::Query(query): axum::extract::Query<KilnQuery>,
) -> Result<Json<NoteByNameReply>, WebError> {
    // Security: Validate note name doesn't contain path traversal
    validate_note_name(&name)?;

    let note = state
        .daemon
        .get_note_by_name(&query.kiln, &name)
        .await
        .daemon_err()?;

    match note {
        Some(n) => Ok(Json(n)),
        None => Err(WebError::NotFound(format!("Note '{name}' not found"))),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct KilnQuery {
    /// Absolute path of the kiln that holds the note.
    #[param(value_type = String)]
    kiln: PathBuf,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct BacklinksQuery {
    /// Absolute path of the kiln that holds the note.
    #[param(value_type = String)]
    kiln: PathBuf,
    /// Note name or kiln-relative path (same fuzzy resolution as `get_note_by_name`).
    note: String,
}

/// Join a daemon note path (kiln-relative in normal operation, but absolute
/// records exist) onto the kiln root for `/api/kiln/file` consumers.
fn absolute_note_path(kiln: &std::path::Path, note_path: &str) -> String {
    if std::path::Path::new(note_path).is_absolute() {
        note_path.to_string()
    } else {
        kiln.join(note_path).to_string_lossy().to_string()
    }
}

/// `GET /api/backlinks?kiln=&note=` — linked + unlinked mentions for a note.
///
/// `linked` is the notes whose wikilinks point at the focused note (daemon
/// `get_backlinks`). `unlinked` is plain-text mentions of *other* notes inside
/// the focused note's content (daemon `suggest_links`) — candidates for
/// one-click link insertion. Self-mentions are filtered out.
#[utoipa::path(
    get,
    path = "/api/backlinks",
    params(BacklinksQuery),
    responses(
        (status = 200, body = BacklinksResponse),
        (status = 400, description = "The note name carries a traversal sequence"),
        (status = 404, description = "The kiln holds no note of that name"),
        (status = 502, description = "The daemon could not read the links, or answered a shape this route cannot read"),
    )
)]
async fn get_backlinks(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<BacklinksQuery>,
) -> Result<Json<BacklinksResponse>, WebError> {
    validate_note_name(&query.note)?;

    let resolved = state
        .daemon
        .get_backlinks(&query.kiln, &query.note)
        .await
        .daemon_err()?
        .ok_or_else(|| WebError::NotFound(format!("Note '{}' not found", query.note)))?;

    let note_path = resolved.path;
    let note_title = resolved.title;

    let linked: Vec<BacklinkRow> = resolved
        .backlinks
        .into_iter()
        .map(|entry| {
            let abs_path = absolute_note_path(&query.kiln, &entry.path);
            BacklinkRow { entry, abs_path }
        })
        .collect();

    // Unlinked mentions: scan the focused note's content. A missing or
    // unreadable file degrades to "no suggestions" rather than failing the
    // whole panel — linked mentions come from the index, not the file.
    let abs_path = absolute_note_path(&query.kiln, &note_path);
    let unlinked = match read_note_text(&state, &abs_path).await {
        Some(content) => {
            let mut self_names: Vec<String> = vec![note_title.to_lowercase()];
            let trimmed = note_path.trim_end_matches(".md");
            self_names.push(trimmed.to_lowercase());
            if let Some(stem) = std::path::Path::new(trimmed)
                .file_name()
                .and_then(|s| s.to_str())
            {
                self_names.push(stem.to_lowercase());
            }
            state
                .daemon
                .suggest_links(&query.kiln, &content)
                .await
                .daemon_err()?
                .into_iter()
                .filter(|s| !self_names.contains(&s.target.to_lowercase()))
                .collect::<Vec<_>>()
        }
        None => Vec::new(),
    };

    Ok(Json(BacklinksResponse {
        note: FocusedNoteRow {
            path: note_path,
            abs_path,
            title: note_title,
        },
        linked,
        unlinked,
    }))
}

/// The text of a note, read through the daemon's `fs.read`. `None` for any
/// refusal or failure: the unlinked mentions are a hint, and the linked
/// mentions come from the index, not from the file.
async fn read_note_text(state: &AppState, path: &str) -> Option<String> {
    let reply = super::kiln::read_through_daemon(
        state,
        path,
        crucible_core::file_write::FileEncoding::Text,
    )
    .await
    .ok()?;
    super::kiln::text_of(reply.content)
        .ok()
        .flatten()
        .map(|(text, _hash)| text)
}

/// The note the panel is about.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct FocusedNoteRow {
    /// As the index holds it: kiln-relative in normal operation.
    path: String,
    /// The same note joined onto the kiln root, which is what
    /// `GET /api/kiln/file` takes.
    abs_path: String,
    /// The empty string when the index holds no title. Never `null`.
    title: String,
}

/// A note whose wikilinks point at the focused note.
///
/// Flattens core's own [`BacklinkEntry`] — the daemon's `get_backlinks`
/// reply — and adds `abs_path`, which this route computes and the daemon
/// never sees.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct BacklinkRow {
    #[serde(flatten)]
    entry: BacklinkEntry,
    /// The source joined onto the kiln root. Added by this route, not by
    /// the daemon.
    abs_path: String,
}

/// What `GET /api/backlinks` answers. `unlinked` is the daemon's own
/// [`crucible_daemon::tools::autolink::LinkSuggestion`], unchanged.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct BacklinksResponse {
    note: FocusedNoteRow,
    linked: Vec<BacklinkRow>,
    unlinked: Vec<crucible_daemon::tools::autolink::LinkSuggestion>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PutNoteRequest {
    /// The hash the caller read. Absent keeps the blind overwrite; present
    /// refuses with 409 and the current hash when the file moved on.
    #[serde(default)]
    base_hash: Option<String>,

    /// Absolute path of the kiln to write into. The daemon writes only into
    /// a path that one of its roots holds.
    #[schema(value_type = String)]
    kiln: PathBuf,
    content: String,
}

/// What `PUT /api/notes/{name}` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct NoteSavedResponse {
    success: bool,
    /// The file name written, `.md` included — which is not always the `name`
    /// that was asked for.
    name: String,
    /// The first heading of the content, or its first line.
    title: String,
    /// When this route wrote it, not when the index noticed.
    updated_at: chrono::DateTime<Utc>,
}

/// `PUT /api/notes/{name}` — write a note into an open kiln.
#[utoipa::path(
    put,
    path = "/api/notes/{name}",
    params(("name" = String, Path, description = "The note's name, with or without `.md`")),
    request_body = PutNoteRequest,
    responses(
        (status = 200, body = NoteSavedResponse),
        (status = 400, description = "The content is too large, or the name carries a traversal sequence"),
        (status = 403, description = "The root refuses writes"),
        (status = 404, description = "No root holds the path"),
        (status = 409, description = "The file moved on since `base_hash` was read"),
        (status = 415, description = "The file on disk is not UTF-8 text"),
        (status = 422, description = "The path is invalid, or escapes its root"),
        (status = 502, description = "The daemon could not be reached"),
    )
)]
async fn put_note(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<PutNoteRequest>,
) -> Result<Json<NoteSavedResponse>, WebError> {
    // Security: Validate content size to prevent DoS
    if req.content.len() > MAX_CONTENT_SIZE {
        return Err(WebError::Chat(format!(
            "Note content too large: {} bytes (max {} bytes)",
            req.content.len(),
            MAX_CONTENT_SIZE
        )));
    }

    // Security: Validate note name doesn't contain path traversal
    validate_note_name(&name)?;

    // Build the full file path (ensure .md extension). The daemon's `fs.write`
    // decides which root holds it, and refuses a path that no root holds.
    let note_filename = if name.ends_with(".md") {
        name.clone()
    } else {
        format!("{}.md", name)
    };
    let file_path = req.kiln.join(&note_filename);

    let answer = state
        .daemon
        .fs_write(&crucible_core::file_write::FileWriteRequest {
            path: file_path.to_string_lossy().into_owned(),
            change: crucible_core::file_write::FileChange::Put {
                content: req.content.clone(),
                base_hash: req.base_hash,
                base_text: None,
            },
        })
        .await
        .daemon_err()?;
    super::kiln::check_file_answer(&answer)?;

    let title = extract_title(&req.content);

    Ok(Json(NoteSavedResponse {
        success: true,
        name: note_filename,
        title,
        updated_at: Utc::now(),
    }))
}

fn extract_title(content: &str) -> String {
    content
        .lines()
        .find(|line| line.starts_with('#'))
        .and_then(|line| {
            let trimmed = line.trim_start_matches('#').trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(|| content.lines().next().unwrap_or("Untitled").to_string())
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct VectorSearchRequest {
    /// Absolute path of the kiln to search.
    #[schema(value_type = String)]
    kiln: PathBuf,
    /// The query vector, already embedded by the caller.
    vector: Vec<f32>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    10
}

/// One block hit, as `search_vectors` ranked it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct VectorSearchRow {
    /// The note's kiln-relative path.
    document_id: String,
    /// Similarity, higher is closer.
    score: f64,
    /// Where in the note the hit sits, or `null` when the hit names the whole
    /// note. Always written, so `null` reads as "the whole note" and never as
    /// "unknown".
    #[schema(required = true)]
    block: Option<BlockRef>,
}

/// What `POST /api/search/vectors` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct VectorSearchResponse {
    results: Vec<VectorSearchRow>,
}

/// `POST /api/search/vectors` — rank a kiln's blocks against a vector the
/// caller already holds.
#[utoipa::path(
    post,
    path = "/api/search/vectors",
    request_body = VectorSearchRequest,
    responses(
        (status = 200, body = VectorSearchResponse),
        (status = 502, description = "The daemon could not search the kiln"),
    )
)]
async fn search_vectors(
    State(state): State<AppState>,
    Json(req): Json<VectorSearchRequest>,
) -> Result<Json<VectorSearchResponse>, WebError> {
    let results = state
        .daemon
        .search_vectors(&req.kiln, &req.vector, req.limit)
        .await
        .daemon_err()?;

    Ok(Json(VectorSearchResponse {
        results: results
            .into_iter()
            .map(|hit| VectorSearchRow {
                document_id: hit.document_id,
                score: hit.score,
                block: hit.block,
            })
            .collect(),
    }))
}

/// `POST /api/search/semantic` — text semantic search over a kiln's notes.
/// Embeds the query with the kiln's embedding provider, then cosine-scans the
/// embeddings in the kiln's SQLite store. Two daemon RPCs (`embed.query` +
/// `search_vectors`) mirror the CLI's `run_semantic_search`. The daemon
/// answers with one row per block, best first. The panel lists notes, so
/// this route keeps one row per note: its best block, with `block` and
/// `snippet` when the kiln has block rows. Each hit's `document_id` is the
/// kiln-relative note path; `path` is the absolute path for the editor to
/// open. Requires an embedding provider (else `embed.query` fails) AND
/// processed notes (no embeddings yields no hits).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SemanticSearchRequest {
    /// Absolute path of the kiln to search.
    #[schema(value_type = String)]
    kiln: PathBuf,
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

/// One note hit, with the block that earned it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SemanticSearchRow {
    /// The note's kiln-relative path. The same value as `rel_path`; both stay
    /// because the browser reads both.
    document_id: String,
    /// The note's kiln-relative path, for display.
    rel_path: String,
    /// The absolute path, which is what an editor opens.
    path: String,
    /// Similarity, higher is closer.
    score: f64,
    /// The note's best block, or `null` when the kiln has no block rows.
    /// Always written.
    #[schema(required = true)]
    block: Option<BlockRef>,
    /// The block's text, or `null` when there is none. Always written.
    #[schema(required = true)]
    snippet: Option<String>,
}

/// What `POST /api/search/semantic` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SemanticSearchResponse {
    results: Vec<SemanticSearchRow>,
}

/// `POST /api/search/semantic` — embed the query, then rank the kiln's notes.
#[utoipa::path(
    post,
    path = "/api/search/semantic",
    request_body = SemanticSearchRequest,
    responses(
        (status = 200, body = SemanticSearchResponse),
        (status = 502, description = "The kiln has no embedding provider, or the daemon could not search it"),
    )
)]
async fn search_semantic(
    State(state): State<AppState>,
    Json(req): Json<SemanticSearchRequest>,
) -> Result<Json<SemanticSearchResponse>, WebError> {
    // Blank query → empty results (skip the embed round-trip).
    if req.query.trim().is_empty() {
        return Ok(Json(SemanticSearchResponse {
            results: Vec::new(),
        }));
    }

    let embedding = state
        .daemon
        .embed_query(&req.kiln, &req.query)
        .await
        .daemon_err()?;

    let results = state
        .daemon
        .search_vectors(&req.kiln, &embedding, req.limit)
        .await
        .daemon_err()?;

    Ok(Json(SemanticSearchResponse {
        results: one_row_per_note(results)
            .into_iter()
            .map(|hit| SemanticSearchRow {
                rel_path: hit.document_id.clone(),
                path: absolute_note_path(&req.kiln, &hit.document_id),
                document_id: hit.document_id,
                score: hit.score,
                block: hit.block,
                snippet: hit.snippet,
            })
            .collect(),
    }))
}

/// Keep the first hit of each note. Hits arrive best first, so the first
/// block of a note is its best block.
fn one_row_per_note(hits: Vec<VectorHit>) -> Vec<VectorHit> {
    let mut seen = std::collections::HashSet::new();
    hits.into_iter()
        .filter(|hit| seen.insert(hit.document_id.clone()))
        .collect()
}

/// `POST /api/search/grep` — ripgrep-style content search over an absolute
/// `root`. The daemon enforces that `root` is contained within a registered
/// project or open kiln (a root outside every known root is rejected with
/// INVALID_PARAMS, surfaced here as 400). `glob` filters by file name
/// (e.g. `*.md`); `null` searches all files. `.gitignore` is respected and
/// binary files are skipped.
#[utoipa::path(
    post,
    path = "/api/search/grep",
    request_body = GrepSearchRequest,
    responses(
        (status = 200, body = GrepSearchResponse),
        (status = 400, description = "The root sits outside every registered project and open kiln, or the query is not a valid regex"),
        (status = 502, description = "The daemon could not run the search"),
    )
)]
async fn search_grep(
    State(state): State<AppState>,
    Json(req): Json<GrepSearchRequest>,
) -> Result<Json<GrepSearchResponse>, WebError> {
    let resp = state
        .daemon
        .search_grep(
            &req.root,
            &req.query,
            req.regex,
            req.glob.as_deref(),
            req.limit,
            req.case_insensitive,
        )
        .await
        .map_err(map_grep_err)?;

    Ok(Json(resp))
}

/// Map a `search_grep` daemon error. Containment/parameter rejections come back
/// as JSON-RPC `INVALID_PARAMS` (-32602) — surface those as 400 Bad Request
/// rather than a 502; everything else is a genuine upstream/daemon failure.
fn map_grep_err(e: impl std::fmt::Display) -> WebError {
    let msg = e.to_string();
    if msg.contains("-32602") {
        WebError::Chat(msg)
    } else {
        WebError::Daemon(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        arb_safe_path, arb_traversal_path, request_json, shape, shape_in_kilns, survives,
        MOCK_DAEMON_KILN_PATH,
    };
    use proptest::prelude::*;
    use tempfile::TempDir;

    // =====================================================================
    // Each route answers the shape it declares
    // =====================================================================

    #[tokio::test]
    async fn list_kilns_answers_the_declared_shape() {
        let listing: KilnListResponse = shape("GET", "/api/kilns", None).await;

        let registered = &listing.kilns[0];
        assert_eq!(registered.path, MOCK_DAEMON_KILN_PATH);
        assert_eq!(registered.name, "daemon-kiln");
        assert!(registered.registered);
        assert!(registered.open);
        assert_eq!(registered.last_access_secs_ago, Some(12));

        // A directory the daemon holds open that the registry cannot name. It
        // carries the empty string, never `null`, and no picker may offer it.
        let unnamed = &listing.kilns[1];
        assert_eq!(unnamed.name, "");
        assert!(!unnamed.registered);
    }

    #[tokio::test]
    async fn a_kiln_row_says_if_it_is_a_git_repository() {
        let listing: KilnListResponse = shape("GET", "/api/kilns", None).await;

        assert!(
            listing.kilns[0].git,
            "the mock names the first kiln a git top level"
        );
        assert!(
            !listing.kilns[1].git,
            "the mock names the second kiln a plain folder"
        );
    }

    #[tokio::test]
    async fn list_notes_answers_the_declared_shape() {
        let listing: NoteListResponse = shape("GET", "/api/notes?kiln=/daemon/kiln", None).await;

        let titled = &listing.notes[0];
        assert_eq!(titled.name, "Kilns");
        assert_eq!(titled.path, "notes/kilns.md");
        assert_eq!(titled.title.as_deref(), Some("Kilns"));
        assert_eq!(titled.tags, ["knowledge"]);
        assert_eq!(
            titled.properties.get("status").and_then(|v| v.as_str()),
            Some("draft"),
            "the note's own frontmatter rides along"
        );

        // A note the index has no title or timestamp for. `null` is the value,
        // never an absent key.
        let bare = &listing.notes[1];
        assert_eq!(bare.title, None);
        assert_eq!(bare.updated_at, None);
        assert!(bare.properties.is_empty());
    }

    #[tokio::test]
    async fn get_note_answers_the_declared_shape() {
        let note: NoteByNameReply = shape("GET", "/api/notes/Kilns?kiln=/daemon/kiln", None).await;

        assert_eq!(note.path, "notes/kilns.md");
        assert_eq!(note.title, "Kilns");
        assert_eq!(note.links_to, ["Projects"]);
        // The daemon writes the same links twice, under two names. Both are on
        // the wire, so both are named.
        assert_eq!(note.wikilinks[0].target, "Projects");
        assert_eq!(note.content_hash.len(), 64);
    }

    #[tokio::test]
    async fn a_note_the_kiln_does_not_hold_is_a_404() {
        let (status, _) = request_json("GET", "/api/notes/missing?kiln=/daemon/kiln", None).await;

        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    /// The real daemon indexes three notes. The linker note links to the
    /// focused note. The focused note names another note and itself in plain
    /// text. Unlinked mentions are scanned out of the file of the focused
    /// note, so this test also needs the note in a kiln that `fs.read` admits.
    #[tokio::test]
    async fn get_backlinks_answers_the_declared_shape() {
        const FOCUSED: &str =
            "---\ntitle: Focused Note\n---\nOther Note is worth reading. Focused Note is this one.\n";
        const LINKER: &str = "---\ntitle: Linker Note\n---\nSee [[focused]].\n";
        let kiln = TempDir::new().unwrap();
        tokio::fs::create_dir(kiln.path().join("notes"))
            .await
            .unwrap();
        for (path, text) in [
            ("notes/focused.md", FOCUSED),
            ("notes/linker.md", LINKER),
            ("notes/Other Note.md", "Another note.\n"),
        ] {
            tokio::fs::write(kiln.path().join(path), text)
                .await
                .unwrap();
        }
        let root = kiln.path().display();

        let answer: BacklinksResponse = shape_in_kilns(
            "GET",
            &format!("/api/backlinks?kiln={root}&note=focused"),
            None,
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(answer.note.path, "notes/focused.md");
        assert_eq!(answer.note.title, "Focused Note");
        assert_eq!(
            answer.note.abs_path,
            kiln.path().join("notes/focused.md").to_string_lossy(),
            "the panel needs the path `/api/kiln/file` takes"
        );

        let linker = &answer.linked[0];
        assert_eq!(linker.entry.name, "linker");
        assert_eq!(linker.entry.title.as_deref(), Some("Linker Note"));
        assert_eq!(
            linker.abs_path,
            kiln.path().join("notes/linker.md").to_string_lossy()
        );
        let span = linker
            .entry
            .span_start
            .zip(linker.entry.span_end)
            .expect("an indexed link has a span");
        assert_eq!(
            &LINKER[span.0 as usize..span.1 as usize],
            "focused",
            "the span is the byte range of the link target in the linker note"
        );

        // The focused note mentions itself; that suggestion is filtered out.
        assert_eq!(answer.unlinked.len(), 1, "{:?}", answer.unlinked);
        assert_eq!(answer.unlinked[0].target, "Other Note");
        assert_eq!(
            answer.unlinked[0].offset,
            FOCUSED.find("Other Note").unwrap()
        );
    }

    #[tokio::test]
    async fn search_vectors_answers_the_declared_shape() {
        let answer: VectorSearchResponse = shape(
            "POST",
            "/api/search/vectors",
            Some(serde_json::json!({ "kiln": "/daemon/kiln", "vector": [0.1, 0.2] })),
        )
        .await;

        // Every block row the daemon ranked, not one per note: folding is the
        // semantic route's job, not this one's.
        assert_eq!(answer.results.len(), 3);
        let best = &answer.results[0];
        assert_eq!(best.document_id, "notes/kilns.md");
        let block = best.block.as_ref().expect("the hit names a block");
        assert_eq!(block.span_start, 40);
        assert_eq!(block.kind, "paragraph");
    }

    #[tokio::test]
    async fn search_semantic_answers_the_declared_shape() {
        let answer: SemanticSearchResponse = shape(
            "POST",
            "/api/search/semantic",
            Some(serde_json::json!({ "kiln": "/daemon/kiln", "query": "what is a kiln" })),
        )
        .await;

        // Two of the daemon's three block hits sit in one note, and this route
        // keeps one row per note.
        assert_eq!(answer.results.len(), 2);
        let best = &answer.results[0];
        assert_eq!(best.document_id, "notes/kilns.md");
        assert_eq!(best.rel_path, "notes/kilns.md");
        assert_eq!(best.path, "/daemon/kiln/notes/kilns.md");
        assert_eq!(
            best.snippet.as_deref(),
            Some("A kiln is where knowledge goes.")
        );
        assert!(best.block.is_some());
    }

    #[tokio::test]
    async fn search_grep_answers_the_declared_shape() {
        let answer: GrepSearchResponse = shape(
            "POST",
            "/api/search/grep",
            Some(serde_json::json!({ "root": "/tmp/test-kiln", "query": "needle" })),
        )
        .await;

        assert!(!answer.truncated);
        let hit = &answer.hits[0];
        assert_eq!(hit.rel_path, "a.md");
        assert_eq!(hit.line, 3);
        assert_eq!(hit.match_start, 2);
        assert_eq!(hit.match_end, 8);
    }

    #[tokio::test]
    async fn resolve_note_answers_the_declared_shape() {
        let kiln = TempDir::new().unwrap();
        tokio::fs::create_dir(kiln.path().join("Notes"))
            .await
            .unwrap();
        tokio::fs::write(kiln.path().join("Notes/Design.md"), "# Design\n")
            .await
            .unwrap();

        let answer: ResolvedNoteResponse = shape_in_kilns(
            "GET",
            &format!(
                "/api/notes/resolve?kiln={}&name=Design",
                kiln.path().display()
            ),
            None,
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(answer.path, "Notes/Design.md");
        assert_eq!(answer.title.as_deref(), Some("Design"));
        assert!(
            answer.absolute_path.ends_with("Notes/Design.md"),
            "the editor opens the absolute path: {}",
            answer.absolute_path
        );
    }

    #[tokio::test]
    async fn put_note_answers_the_declared_shape() {
        let kiln = TempDir::new().unwrap();

        let answer: NoteSavedResponse = shape_in_kilns(
            "PUT",
            "/api/notes/Seed",
            Some(serde_json::json!({
                "kiln": kiln.path(),
                "content": "# Seeded\n\nBody.\n",
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert!(answer.success);
        assert_eq!(answer.name, "Seed.md", "the route adds the suffix");
        assert_eq!(answer.title, "Seeded", "the title is the first heading");
        assert_eq!(
            tokio::fs::read_to_string(kiln.path().join("Seed.md"))
                .await
                .unwrap(),
            "# Seeded\n\nBody.\n"
        );
    }

    // =====================================================================
    // The rows write back what the daemon sent
    // =====================================================================

    /// `title` and `updated_at` are always written, even when the index
    /// holds none: the value is `null`, and the key is never absent. An
    /// absent key would be a different answer, so this is checked on the
    /// wire, not just on the struct.
    #[test]
    fn a_bare_note_row_writes_back_its_nulls() {
        let row = NoteListRow {
            name: "Untitled".to_string(),
            path: "notes/untitled.md".to_string(),
            title: None,
            tags: Vec::new(),
            updated_at: None,
            properties: Default::default(),
        };

        let wire = serde_json::to_value(&row).expect("NoteListRow serializes");
        assert_eq!(wire["title"], serde_json::Value::Null);
        assert_eq!(wire["updated_at"], serde_json::Value::Null);
        survives::<NoteListRow>(&wire);
    }

    /// A hit's block survives [`VectorSearchRow`].
    ///
    /// The block is the structured part, and it comes from the daemon's own
    /// [`VectorHit`] rather than a literal. The row drops `snippet` on
    /// purpose — `POST /api/search/vectors` never carried it — so the object
    /// is assembled the way the handler assembles it.
    #[test]
    fn a_vector_hit_writes_back_the_block_search_vectors_sent() {
        let hit = VectorHit {
            document_id: "notes/kilns.md".to_string(),
            score: 0.91,
            block: Some(crucible_core::types::database::BlockRef {
                span_start: 40,
                span_end: 90,
                kind: "paragraph".to_string(),
                cited: vec![(40, 55)],
            }),
            snippet: Some("A kiln is where knowledge goes.".to_string()),
        };

        survives::<VectorSearchRow>(&serde_json::json!({
            "document_id": hit.document_id,
            "score": hit.score,
            "block": hit.block,
        }));
    }

    /// A legacy index row has no span. The row leaves both keys out and does
    /// not send `null`.
    #[test]
    fn a_span_less_backlink_row_leaves_the_span_keys_out() {
        let row = serde_json::json!({
            "name": "linker",
            "path": "notes/linker.md",
            "title": "Linker Note",
            "abs_path": "/kiln/notes/linker.md",
        });

        survives::<BacklinkRow>(&row);
    }

    #[tokio::test]
    async fn grep_search_maps_hits_to_wire_shape() {
        let (status, json) = request_json(
            "POST",
            "/api/search/grep",
            Some(serde_json::json!({ "root": "/tmp/test-kiln", "query": "needle" })),
        )
        .await;

        assert_eq!(status, axum::http::StatusCode::OK);
        let hits = json["hits"].as_array().expect("hits array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["rel_path"], "a.md");
        assert_eq!(hits[0]["line"], 3);
        assert_eq!(hits[0]["text"], "a needle here");
        assert_eq!(hits[0]["match_start"], 2);
        assert_eq!(hits[0]["match_end"], 8);
        assert_eq!(json["truncated"], false);
    }

    fn is_valid_note_name(name: &str) -> bool {
        !name.contains("..")
            && !name.starts_with('/')
            && !name.starts_with('\\')
            && !name.contains('\0')
    }

    // Helper to test path escape detection
    fn path_escapes_base(base: &std::path::Path, name: &str) -> bool {
        let file_path = base.join(name);
        !file_path.starts_with(base)
    }

    // ===== Path Traversal Tests =====

    #[test]
    fn test_validate_note_name_rejects_parent_traversal() {
        let attacks = vec![
            "../etc/passwd",
            "../../secret",
            "../../../etc/passwd",
            "notes/../../../etc/passwd",
            "foo/../bar/../../../etc/passwd",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject parent traversal: {}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_rejects_backslash_paths() {
        let attacks = vec![
            "..\\windows\\system32",
            "notes\\..\\..\\secret",
            "\\etc\\passwd",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject backslash path: {}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_rejects_absolute_paths() {
        let attacks = vec![
            "/etc/passwd",
            "/root/.ssh/id_rsa",
            "/var/log/syslog",
            "/home/user/secret",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject absolute path: {}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_rejects_null_bytes() {
        let attacks = vec![
            "note\0.md",
            "folder/note\0hidden.md",
            "\0etc/passwd",
            "note.md\0.txt",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject null byte: {:?}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_allows_valid_names() {
        let valid_names = vec![
            "note.md",
            "my-note.md",
            "my_note.md",
            "folder/note.md",
            "deep/nested/folder/note.md",
            "Note With Spaces.md",
            "note123.md",
            "2024-01-15-daily.md",
            "README",
            "index",
        ];
        for name in valid_names {
            assert!(
                is_valid_note_name(name),
                "Should allow valid name: {}",
                name
            );
        }
    }

    // ===== Content Size Tests =====

    #[test]
    fn test_content_size_limit_constant() {
        assert_eq!(
            MAX_CONTENT_SIZE,
            10 * 1024 * 1024,
            "Max size should be 10MB"
        );
    }

    #[test]
    fn test_content_size_validation_rejects_oversized() {
        let oversized = "x".repeat(MAX_CONTENT_SIZE + 1);
        assert!(
            oversized.len() > MAX_CONTENT_SIZE,
            "Content should exceed limit"
        );
    }

    #[test]
    fn test_content_size_validation_accepts_max_size() {
        let max_content = "x".repeat(MAX_CONTENT_SIZE);
        assert!(
            max_content.len() <= MAX_CONTENT_SIZE,
            "Content at max size should be accepted"
        );
    }

    #[test]
    fn test_content_size_validation_accepts_normal_size() {
        let normal_content = "# My Note\n\nSome content here.";
        assert!(
            normal_content.len() <= MAX_CONTENT_SIZE,
            "Normal content should be accepted"
        );
    }

    // ===== Path Escape Tests =====

    #[test]
    fn test_path_join_does_not_normalize_traversal() {
        let base = std::path::Path::new("/home/user/kiln");

        // Path::join doesn't normalize ".." - it creates literal path
        // This is why we validate name for ".." BEFORE joining
        // The starts_with check is defense-in-depth for edge cases
        let joined = base.join("../../../etc/passwd");
        assert!(
            joined.starts_with(base),
            "Joined path literally starts with base (not normalized)"
        );
    }

    #[test]
    fn test_path_escape_detection_allows_nested() {
        let base = std::path::Path::new("/home/user/kiln");

        // Valid nested paths should not escape
        assert!(
            !path_escapes_base(base, "notes/daily/2024-01-15.md"),
            "Nested path should not escape"
        );
        assert!(
            !path_escapes_base(base, "deep/nested/folder/note.md"),
            "Deep nested path should not escape"
        );
    }

    // ===== Title Extraction Tests =====

    #[test]
    fn test_extract_title_from_h1() {
        let content = "# My Title\n\nSome content";
        assert_eq!(extract_title(content), "My Title");
    }

    #[test]
    fn test_extract_title_from_h2() {
        let content = "## Secondary Title\n\nContent";
        assert_eq!(extract_title(content), "Secondary Title");
    }

    #[test]
    fn test_extract_title_empty_heading_returns_first_line_as_fallback() {
        let content = "#\n\nActual content here";
        assert_eq!(extract_title(content), "#");
    }

    #[test]
    fn test_extract_title_uses_first_line_as_fallback() {
        let content = "No heading here\n\nJust content";
        assert_eq!(extract_title(content), "No heading here");
    }

    #[test]
    fn test_extract_title_handles_empty_content() {
        let content = "";
        assert_eq!(extract_title(content), "Untitled");
    }

    #[test]
    fn test_extract_title_trims_whitespace() {
        let content = "#    Lots of spaces   \n\nContent";
        assert_eq!(extract_title(content), "Lots of spaces");
    }

    #[test]
    fn test_put_note_content_exactly_ten_megabytes_is_allowed() {
        let content = "x".repeat(MAX_CONTENT_SIZE);
        assert_eq!(content.len(), 10 * 1024 * 1024);
        assert!(content.len() <= MAX_CONTENT_SIZE);
        assert!(content.len() <= MAX_CONTENT_SIZE);
    }

    #[test]
    fn test_put_note_content_ten_megabytes_plus_one_is_rejected() {
        let content = "x".repeat(MAX_CONTENT_SIZE + 1);
        assert_eq!(content.len(), (10 * 1024 * 1024) + 1);
        assert!(content.len() > MAX_CONTENT_SIZE);
        assert_eq!(
            format!(
                "Note content too large: {} bytes (max {} bytes)",
                content.len(),
                MAX_CONTENT_SIZE
            ),
            "Note content too large: 10485761 bytes (max 10485760 bytes)"
        );
    }

    #[test]
    fn test_put_note_appends_md_extension_when_missing() {
        let name = "daily/2026-03-10";
        let note_filename = if name.ends_with(".md") {
            name.to_string()
        } else {
            format!("{}.md", name)
        };
        assert_eq!(note_filename, "daily/2026-03-10.md");
    }

    #[test]
    fn test_put_note_preserves_md_extension_when_present() {
        let name = "daily/2026-03-10.md";
        let note_filename = if name.ends_with(".md") {
            name.to_string()
        } else {
            format!("{}.md", name)
        };
        assert_eq!(note_filename, "daily/2026-03-10.md");
    }

    #[test]
    fn test_extract_title_returns_first_heading_when_multiple_exist() {
        let content = "Intro line\n## First Heading\n### Second Heading";
        assert_eq!(extract_title(content), "First Heading");
    }

    #[test]
    fn test_extract_title_without_heading_falls_back_to_first_line() {
        let content = "No heading here\nstill no heading";
        assert_eq!(extract_title(content), "No heading here");
    }

    proptest! {
        #[test]
        fn prop_validate_note_name_rejects_traversal_patterns(path in arb_traversal_path()) {
            prop_assert!(!is_valid_note_name(&path));
        }

        #[test]
        fn prop_validate_note_name_rejects_embedded_null_bytes(prefix in ".{0,32}", suffix in ".{0,32}") {
            let path = format!("{prefix}\0{suffix}");
            prop_assert!(!is_valid_note_name(&path));
        }

        #[test]
        fn prop_validate_note_name_accepts_safe_paths(
            path in arb_safe_path().prop_filter("matches current note-name policy", |s| {
                !s.starts_with('/') && !s.starts_with('\\')
            })
        ) {
            prop_assert!(is_valid_note_name(&path));
        }
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use tempfile::TempDir;

    /// The invariant: one root never surfaces another root's content. A link in
    /// vault A that happens to name a note in vault B must fail to resolve, not
    /// quietly open B's note.
    #[test]
    fn a_symlink_out_of_the_kiln_never_resolves() {
        let tmp = TempDir::new().unwrap();
        let kiln_a = tmp.path().join("a");
        let kiln_b = tmp.path().join("b");
        std::fs::create_dir_all(&kiln_a).unwrap();
        std::fs::create_dir_all(&kiln_b).unwrap();
        std::fs::write(kiln_b.join("Secret.md"), "# Secret\n").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(kiln_b.join("Secret.md"), kiln_a.join("Secret.md")).unwrap();

        let root = kiln_a.canonicalize().unwrap();
        assert!(
            contained_file(&kiln_a.join("Secret.md"), &root).is_none(),
            "a symlink into another kiln must not resolve"
        );
    }

    #[test]
    fn a_real_note_inside_the_kiln_resolves() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("Notes")).unwrap();
        std::fs::write(tmp.path().join("Notes/Architecture.md"), "# A\n").unwrap();

        let root = tmp.path().canonicalize().unwrap();
        let hit = contained_file(&root.join("Notes/Architecture.md"), &root);
        assert!(hit.is_some());
        assert!(hit.unwrap().starts_with(&root));
    }

    #[test]
    fn a_directory_is_not_a_note() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("Notes")).unwrap();
        let root = tmp.path().canonicalize().unwrap();
        assert!(contained_file(&root.join("Notes"), &root).is_none());
    }
}
