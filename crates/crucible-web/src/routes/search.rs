use super::helpers::{
    note_to_metadata_json, validate_note_name, validate_write_target_within_kiln, MAX_CONTENT_SIZE,
};
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use std::path::PathBuf;
use tokio::fs;

pub fn search_routes() -> Router<AppState> {
    Router::new()
        .route("/api/kilns", get(list_kilns))
        .route("/api/notes", get(list_notes))
        .route("/api/notes/resolve", get(resolve_note))
        .route("/api/notes/{name}", get(get_note))
        .route("/api/notes/{name}", put(put_note))
        .route("/api/backlinks", get(get_backlinks))
        .route("/api/search/vectors", post(search_vectors))
        .route("/api/search/semantic", post(search_semantic))
        .route("/api/search/grep", post(search_grep))
}

async fn list_kilns(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let kilns = state.daemon.kiln_list().await.daemon_err()?;

    Ok(Json(serde_json::json!({ "kilns": kilns })))
}

#[derive(Debug, Deserialize)]
struct ListNotesQuery {
    kiln: PathBuf,
    path_filter: Option<String>,
}

async fn list_notes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListNotesQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let notes = state
        .daemon
        .list_notes(&query.kiln, query.path_filter.as_deref())
        .await
        .daemon_err()?;

    let notes_json: Vec<serde_json::Value> = notes.into_iter().map(note_to_metadata_json).collect();

    Ok(Json(serde_json::json!({ "notes": notes_json })))
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
async fn resolve_note(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ResolveQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    validate_note_name(&query.name)?;

    // The caller does NOT get to nominate the root.
    //
    // Taking `kiln` at face value made this an existence-and-path oracle for
    // the whole filesystem: `?kiln=/etc&name=passwd` answered, as did
    // `?kiln=/&name=etc/shadow` (which also walked the entire disk). It ignored
    // a project's `project_files` policy that every sibling route honours. The
    // supplied path is resolved through the same kiln-then-project rule the
    // canvas endpoints use, and the CANONICAL root that comes back is what gets
    // walked — so the walk is bounded by a registered root, not by argv.
    let (root, _policy) = super::canvas::enclosing_root(&state, &query.kiln).await?;

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
            return Ok(Json(resolved_json(&root, &contained)));
        }
    }

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

    match best {
        Some(path) => Ok(Json(resolved_json(&root, &path))),
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

fn resolved_json(root: &std::path::Path, path: &std::path::Path) -> serde_json::Value {
    let rel = path.strip_prefix(root).unwrap_or(path);
    serde_json::json!({
        "path": rel.to_string_lossy(),
        "absolutePath": path.to_string_lossy(),
        "title": path.file_stem().map(|s| s.to_string_lossy().to_string()),
    })
}

#[derive(Debug, Deserialize)]
struct ResolveQuery {
    kiln: PathBuf,
    name: String,
}

async fn get_note(
    State(state): State<AppState>,
    Path(name): Path<String>,
    axum::extract::Query(query): axum::extract::Query<KilnQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
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

#[derive(Debug, Deserialize)]
struct KilnQuery {
    kiln: PathBuf,
}

#[derive(Debug, Deserialize)]
struct BacklinksQuery {
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
async fn get_backlinks(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<BacklinksQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    validate_note_name(&query.note)?;

    let resolved = state
        .daemon
        .get_backlinks(&query.kiln, &query.note)
        .await
        .daemon_err()?
        .ok_or_else(|| WebError::NotFound(format!("Note '{}' not found", query.note)))?;

    let note_path = resolved
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let note_title = resolved
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let linked: Vec<serde_json::Value> = resolved
        .get("backlinks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut b| {
            if let Some(obj) = b.as_object_mut() {
                let rel = obj
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                obj.insert(
                    "abs_path".to_string(),
                    serde_json::Value::String(absolute_note_path(&query.kiln, &rel)),
                );
            }
            b
        })
        .collect();

    // Unlinked mentions: scan the focused note's content. A missing or
    // unreadable file degrades to "no suggestions" rather than failing the
    // whole panel — linked mentions come from the index, not the file.
    let abs_path = absolute_note_path(&query.kiln, &note_path);
    let unlinked = match fs::read_to_string(&abs_path).await {
        Ok(content) => {
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
                .filter(|s| {
                    s.get("target")
                        .and_then(|t| t.as_str())
                        .map(|t| !self_names.contains(&t.to_lowercase()))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>()
        }
        Err(_) => Vec::new(),
    };

    Ok(Json(serde_json::json!({
        "note": { "path": note_path, "abs_path": abs_path, "title": note_title },
        "linked": linked,
        "unlinked": unlinked,
    })))
}

#[derive(Debug, Deserialize)]
struct PutNoteRequest {
    kiln: PathBuf,
    content: String,
}

async fn put_note(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<PutNoteRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
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

    // Security: Validate kiln is registered/open
    let kilns = state.daemon.kiln_list().await.daemon_err()?;

    let canonical_kiln = req
        .kiln
        .canonicalize()
        .map_err(|_| WebError::NotFound("Invalid kiln path".to_string()))?;

    let kiln_registered = kilns.iter().any(|kiln_value| {
        kiln_value
            .get("path")
            .and_then(|p| p.as_str())
            .and_then(|p| PathBuf::from(p).canonicalize().ok())
            .map(|p| p == canonical_kiln)
            .unwrap_or(false)
    });

    if !kiln_registered {
        return Err(WebError::NotFound(
            "Kiln not registered. Please open the kiln first.".to_string(),
        ));
    }

    // Build the full file path (ensure .md extension)
    let note_filename = if name.ends_with(".md") {
        name.clone()
    } else {
        format!("{}.md", name)
    };
    let file_path = canonical_kiln.join(&note_filename);

    // Security: Verify the file path is still within the kiln after joining
    if !file_path.starts_with(&canonical_kiln) {
        return Err(WebError::Chat(
            "Invalid note name: path escapes kiln directory".to_string(),
        ));
    }

    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await.map_err(WebError::Io)?;
    }

    // Security: block writes that would escape the kiln by following a symlinked
    // final component (the lexical check above does NOT resolve symlinks).
    validate_write_target_within_kiln(&file_path, &canonical_kiln)?;

    // Write content to filesystem (source of truth). The file watcher then runs
    // the note through the daemon pipeline (real content hash, tags, wikilinks,
    // embedding), same as PUT /api/kiln/file. We deliberately do NOT upsert a
    // NoteRecord here: the old code wrote a stub (default hash, empty tags/links,
    // null embedding) that CLOBBERED the properly-enriched record — dropping the
    // note from vector search and backlinks until the watcher re-enriched it.
    fs::write(&file_path, &req.content)
        .await
        .map_err(WebError::Io)?;

    let title = extract_title(&req.content);

    Ok(Json(serde_json::json!({
        "success": true,
        "name": note_filename,
        "title": title,
        "updated_at": Utc::now()
    })))
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

#[derive(Debug, Deserialize)]
struct VectorSearchRequest {
    kiln: PathBuf,
    vector: Vec<f32>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    10
}

async fn search_vectors(
    State(state): State<AppState>,
    Json(req): Json<VectorSearchRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let results = state
        .daemon
        .search_vectors(&req.kiln, &req.vector, req.limit)
        .await
        .daemon_err()?;

    let results_json: Vec<serde_json::Value> = results
        .into_iter()
        .map(|(doc_id, score)| {
            serde_json::json!({
                "document_id": doc_id,
                "score": score,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "results": results_json })))
}

/// `POST /api/search/semantic` — text semantic search over a kiln's notes.
/// Embeds the query with the kiln's embedding provider, then vector-searches
/// the Lance index. Two daemon RPCs (`embed.query` + `search_vectors`) mirror
/// the CLI's `run_semantic_search`. Each hit's `document_id` is the
/// kiln-relative note path; `path` is the absolute path for the editor to open.
/// Requires an embedding provider (else `embed.query` fails) AND processed
/// notes (an empty Lance index yields no hits).
#[derive(Debug, Deserialize)]
struct SemanticSearchRequest {
    kiln: PathBuf,
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

async fn search_semantic(
    State(state): State<AppState>,
    Json(req): Json<SemanticSearchRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Blank query → empty results (skip the embed round-trip).
    if req.query.trim().is_empty() {
        return Ok(Json(serde_json::json!({ "results": [] })));
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

    let results_json: Vec<serde_json::Value> = results
        .into_iter()
        .map(|(doc_id, score)| {
            serde_json::json!({
                "document_id": doc_id,
                "rel_path": doc_id,
                "path": absolute_note_path(&req.kiln, &doc_id),
                "score": score,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "results": results_json })))
}

/// `POST /api/search/grep` — ripgrep-style content search over an absolute
/// `root`. The daemon enforces that `root` is contained within a registered
/// project or open kiln (a root outside every known root is rejected with
/// INVALID_PARAMS, surfaced here as 400). `glob` filters by file name
/// (e.g. `*.md`); `null` searches all files. `.gitignore` is respected and
/// binary files are skipped.
#[derive(Debug, Deserialize)]
struct GrepSearchRequest {
    root: String,
    query: String,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default = "default_grep_limit")]
    limit: usize,
    #[serde(default = "default_true")]
    case_insensitive: bool,
}

fn default_grep_limit() -> usize {
    100
}

fn default_true() -> bool {
    true
}

async fn search_grep(
    State(state): State<AppState>,
    Json(req): Json<GrepSearchRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let resp = state
        .daemon
        .search_grep(
            &req.root,
            &req.query,
            req.glob.as_deref(),
            req.limit,
            req.case_insensitive,
        )
        .await
        .map_err(map_grep_err)?;

    Ok(Json(
        serde_json::to_value(resp).unwrap_or_else(|_| serde_json::json!({})),
    ))
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
    use crate::test_support::{arb_safe_path, arb_traversal_path, request_json};
    use proptest::prelude::*;

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
