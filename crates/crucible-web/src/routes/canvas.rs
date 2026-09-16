//! `.canvas` document endpoints.
//!
//! These exist separately from the generic file endpoints because canvas
//! containment is stricter than the `project_files` policy those obey. A canvas
//! is portable knowledge content: it must live in a kiln and may only reference
//! files inside that same kiln. See [`crucible_core::canvas::containment`] for
//! why, and for the three-layer scheme this implements two thirds of.
//!
//! The read path does not merely *report* bad references — it removes them from
//! the payload. A client that never learns the offending path cannot request
//! it, so quarantine is enforced here rather than trusted to the renderer.

use axum::{extract::State, Json};
use crucible_core::canvas::containment::{
    resolve_file_ref, validate_canvas, RefError, RejectedRef,
};
use crucible_core::canvas::{Canvas, NodeKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::helpers::{
    reject_path_traversal, validate_file_within_kiln, validate_write_target_within_kiln,
    MAX_CONTENT_SIZE,
};
use crucible_core::config::{read_project_config, ProjectFileAccess};

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};

pub fn canvas_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_canvas, put_canvas))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct CanvasPathQuery {
    /// Absolute path of the `.canvas` file.
    path: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PutCanvasRequest {
    path: String,
    content: String,
    /// The hash the caller read. Absent keeps the blind overwrite; present
    /// refuses with 409 and the current hash when the file moved on.
    #[serde(default)]
    base_hash: Option<String>,
}

/// A reference the read path refused, as the browser receives it.
///
/// Web-owned on purpose, and NOT a re-export of
/// [`crucible_core::canvas::containment::RejectedRef`]. The core type carries
/// `reference` — the offending path itself — and this reply deliberately drops
/// it: a client that never learns the path cannot ask for it, which is what
/// makes redaction an enforcement rather than a warning. The node id is
/// `nodeId` here because the canvas format is camelCase throughout, and the
/// browser reads it beside `fromNode` and `toNode`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct RejectedRefDto {
    /// Id of the node holding the bad reference.
    #[serde(rename = "nodeId")]
    node_id: String,
    /// Why it was refused, as a sentence to show the reader.
    ///
    /// A rendered [`RefError`], not a token. The browser prints it; nothing
    /// branches on it, and `the_refusal_reasons_stay_whole_sentences` pins
    /// every variant's wording so a reworded error is a visible change rather
    /// than a silent one.
    reason: String,
}

/// What `GET /api/canvas` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct CanvasResponse {
    /// The document, with every refused reference already removed.
    canvas: Canvas,
    /// One entry per reference the document held and this reply withholds.
    rejected: Vec<RejectedRefDto>,
    /// The root the canvas belongs to, which bounds every reference in it.
    #[schema(value_type = String)]
    kiln: PathBuf,
}

/// What `PUT /api/canvas` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct CanvasSavedResponse {
    ok: bool,
}

/// `GET /api/canvas?path=<path>` — read and validate a canvas document.
///
/// Returns `{ canvas, rejected, kiln }`. Any node whose reference fails
/// containment is **redacted in the returned canvas** — a `file` node loses its
/// path, a `group` loses its background — and described in `rejected` so the UI
/// can render a quarantined placeholder explaining why.
///
/// This is the fail-safe layer. A `.canvas` hand-edited on disk to point at
/// `../../../etc/passwd` reaches this handler like any other, and the offending
/// path never leaves the process.
#[utoipa::path(
    get,
    path = "/api/canvas",
    params(CanvasPathQuery),
    responses(
        (status = 200, body = CanvasResponse),
        (status = 404, description = "No open kiln or readable project holds this path, or the file is not there"),
        (status = 422, description = "The path carries a traversal sequence, or the file is not a canvas"),
        (status = 502, description = "The daemon could not list the kilns or the projects"),
    )
)]
async fn get_canvas(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<CanvasPathQuery>,
) -> Result<Json<CanvasResponse>, WebError> {
    reject_path_traversal(&query.path)?;

    let path = PathBuf::from(&query.path);
    let kiln = enclosing_kiln(&state, &path).await?;
    // `enclosing_kiln` matches lexically; this resolves symlinks, so a
    // `.canvas` that is itself a link out of the kiln cannot be read through.
    let path = validate_file_within_kiln(&path, &kiln, &query.path)?;

    let source = fs::read_to_string(&path)
        .await
        .map_err(|e| WebError::NotFound(format!("Canvas not found: {e}")))?;

    let mut canvas =
        Canvas::parse(&source).map_err(|e| WebError::Validation(format!("Invalid canvas: {e}")))?;

    let rejected = validate_canvas(&canvas, &kiln);
    redact(&mut canvas, &kiln);

    Ok(Json(CanvasResponse {
        canvas,
        rejected: rejected_dtos(&rejected),
        kiln,
    }))
}

/// `PUT /api/canvas` — write a canvas document.
///
/// This is the authoritative layer: the document is parsed and every reference
/// checked before anything touches disk. A canvas naming a file outside its
/// kiln is refused wholesale with the offending node ids, rather than being
/// written and cleaned up later.
#[utoipa::path(
    put,
    path = "/api/canvas",
    request_body = PutCanvasRequest,
    responses(
        (status = 200, body = CanvasSavedResponse),
        (status = 403, description = "The project is read-only, or the canvas references a file outside its root"),
        (status = 404, description = "No open kiln or registered project holds this path"),
        (status = 409, description = "The file moved on since `base_hash` was read"),
        (status = 415, description = "The file on disk is not UTF-8 text"),
        (status = 422, description = "The path carries a traversal sequence, the content is too large, or it is not a canvas"),
        (status = 502, description = "The daemon could not list the roots, or could not write"),
    )
)]
async fn put_canvas(
    State(state): State<AppState>,
    Json(req): Json<PutCanvasRequest>,
) -> Result<Json<CanvasSavedResponse>, WebError> {
    reject_path_traversal(&req.path)?;

    if req.content.len() > MAX_CONTENT_SIZE {
        return Err(WebError::Validation(format!(
            "Canvas too large: {} bytes (max {MAX_CONTENT_SIZE})",
            req.content.len()
        )));
    }

    let path = PathBuf::from(&req.path);
    let (kiln, policy) = enclosing_root(&state, &path).await?;
    if !policy.can_write() {
        return Err(WebError::Forbidden(
            "Project files are read-only".to_string(),
        ));
    }

    // Without this, `fs::write` follows a pre-planted symlink and writes
    // outside the kiln even though the parent directory is legitimate — the
    // exact case `validate_write_target_within_kiln` documents. Every other
    // kiln file route already calls it; this one was the gap.
    validate_write_target_within_kiln(&path, &kiln)?;

    let canvas = Canvas::parse(&req.content)
        .map_err(|e| WebError::Validation(format!("Invalid canvas: {e}")))?;

    let rejected = validate_canvas(&canvas, &kiln);
    if !rejected.is_empty() {
        return Err(WebError::Forbidden(format!(
            "Canvas references {} file(s) outside the kiln: {}",
            rejected.len(),
            rejected
                .iter()
                .map(|r| format!("node `{}` → {} ({})", r.node_id, r.reference, r.reason))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    // Restore any reference the READ path blanked.
    //
    // Redaction deliberately withholds a failing path from the client, so the
    // document the client holds — and sends back — has an empty reference where
    // that path was. Writing it through would permanently erase a reference the
    // user was never shown and cannot recover: a vault synced from a machine
    // where a note sat behind a symlink, or a canvas authored elsewhere with a
    // `../shared/note.md` reference, would lose it on the first accidental
    // click. The bad reference stays exactly as it was; it is not made worse.
    let mut canvas = canvas;
    if let Ok(on_disk) = fs::read_to_string(&path).await {
        if let Ok(previous) = Canvas::parse(&on_disk) {
            restore_redacted(&mut canvas, &previous, &kiln);
        }
    }

    // Re-serialize from the parsed document rather than writing the request
    // bytes through. The round-trip is lossless for unknown keys, and this way
    // anything that parsed as a canvas is what lands on disk.
    let serialized = canvas
        .to_json_pretty()
        .map_err(|e| WebError::Validation(format!("Could not serialize canvas: {e}")))?;

    let answer = state
        .daemon
        .fs_write(&crucible_core::file_write::FileWriteRequest {
            path: path.to_string_lossy().into_owned(),
            change: crucible_core::file_write::FileChange::Put {
                content: serialized,
                base_hash: req.base_hash,
                base_text: None,
            },
        })
        .await
        .daemon_err()?;
    super::kiln::check_write(&answer)?;

    Ok(Json(CanvasSavedResponse { ok: true }))
}

/// Resolve the root a canvas belongs to: its kiln, or failing that its project.
///
/// A canvas in a code repository is a legitimate thing to keep — an
/// architecture board that references source files lives with the code, not in
/// a notes vault. So a canvas outside any kiln resolves against its **project**
/// root instead, and its references are contained to that root.
///
/// The containment rule is unchanged in substance: a canvas may only reference
/// files inside the one root that owns it. What changes is which root that can
/// be. A project canvas additionally obeys that project's
/// [`ProjectFileAccess`] policy, so a repository configured `read-only` serves
/// its canvases but refuses to save them, and one configured `off` does not
/// serve them at all — exactly as for any other file in that project.
async fn enclosing_kiln(state: &AppState, path: &Path) -> Result<PathBuf, WebError> {
    enclosing_root(state, path).await.map(|(root, _)| root)
}

/// As [`enclosing_kiln`], but also reporting whether writes are permitted.
pub(crate) async fn enclosing_root(
    state: &AppState,
    path: &Path,
) -> Result<(PathBuf, ProjectFileAccess), WebError> {
    let kilns: Vec<PathBuf> = state
        .daemon
        .kiln_list()
        .await
        .daemon_err()?
        .iter()
        .filter_map(|v| v.get("path").and_then(|p| p.as_str()).map(PathBuf::from))
        .collect();

    // Longest match, not first. With nested kilns (`/vault` and `/vault/sub`
    // both open) taking whichever the daemon happened to list first would
    // attribute a canvas in the inner kiln to the outer one, letting it
    // reference anything under `/vault` — wider than "that same kiln".
    let best_kiln = kilns
        .into_iter()
        .filter_map(|kiln| {
            let canonical = kiln.canonicalize().ok()?;
            (path.starts_with(&canonical) || path.starts_with(&kiln)).then_some(canonical)
        })
        .max_by_key(|k| k.components().count());

    // A kiln always wins over the project containing it: kiln notes are
    // read-write regardless of the project's file policy.
    if let Some(kiln) = best_kiln {
        return Ok((kiln, ProjectFileAccess::ReadWrite));
    }

    let projects = state.daemon.project_list().await.daemon_err()?;
    let best_project = projects
        .into_iter()
        .filter_map(|p| {
            let canonical = p.path.canonicalize().ok()?;
            (path.starts_with(&canonical) || path.starts_with(&p.path))
                .then_some((canonical, p.path))
        })
        .max_by_key(|(canonical, _)| canonical.components().count());

    match best_project {
        Some((canonical, raw)) => {
            let policy = read_project_config(&raw)
                .map(|c| c.security.project_files)
                .unwrap_or_default();
            if !policy.can_read() {
                return Err(WebError::NotFound(
                    "Canvas is not within an open kiln or readable project".to_string(),
                ));
            }
            Ok((canonical, policy))
        }
        None => Err(WebError::NotFound(
            "Canvas is not within an open kiln or registered project".to_string(),
        )),
    }
}

/// Put back any reference the read path blanked before this document was sent.
///
/// Nodes are matched by **id**. Pairing by position broke on the commonest
/// edit there is — adding a card shifts every index, the pairing goes wrong,
/// and the withheld reference is written away.
/// Only a node whose incoming reference is empty and whose on-disk counterpart
/// held a genuinely uncontained path is touched — a user legitimately clearing
/// a reference is left alone, because the on-disk value would have been
/// contained and therefore never redacted in the first place.
fn restore_redacted(incoming: &mut Canvas, on_disk: &Canvas, kiln_root: &Path) {
    let was_redacted = |reference: &str| matches!(resolve_file_ref(reference, kiln_root), Err(e) if e != RefError::Empty);

    // Matched by id, not position. Zipping the two lists paired the wrong
    // nodes the moment a card was added, deleted or brought to front (node
    // order IS z-order), the id guard then declined, and the withheld
    // reference was written away — precisely what this function exists to
    // prevent, on the commonest edit there is.
    //
    // Ids are not guaranteed unique, so a duplicated id restores from the
    // first on-disk node carrying it: the same conservative direction
    // `redact` takes, and it can only ever put back a reference that was
    // already on disk.
    for node in incoming.nodes.iter_mut() {
        let Some(previous) = on_disk.nodes.iter().find(|n| n.id == node.id) else {
            continue;
        };
        match (&mut node.kind, &previous.kind) {
            (
                NodeKind::File { file, subpath },
                NodeKind::File {
                    file: old,
                    subpath: old_subpath,
                },
            ) if file.is_empty() && was_redacted(old) => {
                *file = old.clone();
                *subpath = old_subpath.clone();
            }
            (
                NodeKind::Group { background, .. },
                NodeKind::Group {
                    background: Some(old),
                    ..
                },
            ) if background.is_none() && was_redacted(old) => {
                *background = Some(old.clone());
            }
            _ => {}
        }
    }
}

/// Strip every reference that fails containment from the document.
///
/// Deliberately re-checks each node rather than matching the rejection list by
/// node id. Ids are not enforced unique — `Canvas::node` documents as much, and
/// a hand-edited file can repeat one — so an id-keyed lookup finds the *first*
/// node with that id and leaves the malicious twin untouched, path and all.
/// Walking the nodes makes the redaction structurally complete instead of
/// dependent on a property the format does not guarantee.
fn redact(canvas: &mut Canvas, kiln_root: &Path) {
    let escapes = |reference: &str| matches!(resolve_file_ref(reference, kiln_root), Err(e) if e != RefError::Empty);
    for node in &mut canvas.nodes {
        match &mut node.kind {
            NodeKind::File { file, subpath } if escapes(file) => {
                file.clear();
                *subpath = None;
            }
            NodeKind::Group { background, .. } if background.as_deref().is_some_and(escapes) => {
                *background = None;
            }
            _ => {}
        }
    }
}

fn rejected_dtos(rejected: &[RejectedRef]) -> Vec<RejectedRefDto> {
    rejected
        .iter()
        .map(|r| RejectedRefDto {
            node_id: r.node_id.clone(),
            // The offending path is deliberately NOT echoed back — the
            // reason is enough for the UI to explain the placeholder, and
            // echoing it would hand a probe its own answer.
            reason: r.reason.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{request_json_in_kilns, shape_in_kilns, survives};
    use crucible_core::canvas::containment::is_contained;
    use crucible_core::canvas::Canvas;
    use tempfile::TempDir;

    // =====================================================================
    // The routes answer the shapes they declare
    // =====================================================================

    /// A kiln on disk holding one canvas, so the read path has a real root to
    /// resolve against and a real file to parse.
    async fn kiln_with_canvas(document: serde_json::Value) -> (TempDir, String) {
        let kiln = TempDir::new().unwrap();
        let path = kiln.path().join("Board.canvas");
        tokio::fs::write(&path, document.to_string()).await.unwrap();
        let path = path.to_string_lossy().into_owned();
        (kiln, path)
    }

    /// One node per kind, so no arm of the canvas union reaches the browser
    /// untested, plus a key outside the spec at every level.
    fn every_kind_of_node() -> serde_json::Value {
        serde_json::json!({
            "nodes": [
                { "id": "t", "type": "text", "x": 0, "y": 0, "width": 1, "height": 1,
                  "text": "# Note", "color": "3" },
                { "id": "f", "type": "file", "x": 1, "y": 0, "width": 1, "height": 1,
                  "file": "Notes/Fine.md", "subpath": "#Heading" },
                { "id": "l", "type": "link", "x": 2, "y": 0, "width": 1, "height": 1,
                  "url": "https://example.invalid" },
                { "id": "g", "type": "group", "x": 3, "y": 0, "width": 9, "height": 9,
                  "label": "Cluster", "backgroundStyle": "cover",
                  "styleAttributes": { "border": "dashed" } }
            ],
            "edges": [
                { "id": "e", "fromNode": "t", "toNode": "f", "fromSide": "right",
                  "toEnd": "arrow", "label": "explains", "weight": 3 }
            ],
            "plugin-state": { "zoom": 1.5 }
        })
    }

    #[tokio::test]
    async fn get_canvas_answers_the_declared_shape() {
        let (kiln, path) = kiln_with_canvas(every_kind_of_node()).await;

        let answer: CanvasResponse = shape_in_kilns(
            "GET",
            &format!("/api/canvas?path={path}"),
            None,
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(answer.canvas.nodes.len(), 4, "one node per kind");
        assert_eq!(answer.canvas.edges.len(), 1);
        assert!(
            answer.rejected.is_empty(),
            "every reference in this document is contained"
        );
        assert_eq!(answer.kiln, kiln.path().canonicalize().unwrap());
        // The keys outside the spec survive the reply, which is the whole
        // reason `Canvas` carries an `extra` bag at every level.
        assert!(answer.canvas.extra.contains_key("plugin-state"));
    }

    /// The reply that carries a refusal, which is the shape the placeholder
    /// renderer reads.
    #[tokio::test]
    async fn a_refused_reference_reaches_the_reply_without_its_path() {
        let (kiln, path) = kiln_with_canvas(serde_json::json!({
            "nodes": [{ "id": "bad", "type": "file", "x": 0, "y": 0,
                        "width": 1, "height": 1, "file": "../../../etc/passwd" }]
        }))
        .await;

        let answer: CanvasResponse = shape_in_kilns(
            "GET",
            &format!("/api/canvas?path={path}"),
            None,
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(answer.rejected.len(), 1);
        assert_eq!(answer.rejected[0].node_id, "bad");
        assert!(!answer.rejected[0].reason.is_empty());
        let body = serde_json::to_string(&answer).unwrap();
        assert!(
            !body.contains("passwd"),
            "the offending path must not survive into the reply: {body}"
        );
    }

    #[tokio::test]
    async fn put_canvas_answers_the_declared_shape() {
        let kiln = TempDir::new().unwrap();
        let path = kiln
            .path()
            .join("New.canvas")
            .to_string_lossy()
            .into_owned();

        let answer: CanvasSavedResponse = shape_in_kilns(
            "PUT",
            "/api/canvas",
            Some(serde_json::json!({
                "path": path,
                "content": every_kind_of_node().to_string(),
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert!(answer.ok);
        assert!(
            tokio::fs::read_to_string(&path).await.is_ok(),
            "the document reached the disk"
        );
    }

    /// A canvas naming a file outside its root is refused wholesale, and the
    /// refusal is a 403 rather than a body the reply type could read.
    #[tokio::test]
    async fn put_canvas_refuses_a_reference_outside_the_root() {
        let kiln = TempDir::new().unwrap();
        let path = kiln
            .path()
            .join("Bad.canvas")
            .to_string_lossy()
            .into_owned();

        let (status, body) = request_json_in_kilns(
            "PUT",
            "/api/canvas",
            Some(serde_json::json!({
                "path": path,
                "content": serde_json::json!({
                    "nodes": [{ "id": "bad", "type": "file", "x": 0, "y": 0,
                                "width": 1, "height": 1, "file": "../escape.md" }]
                })
                .to_string(),
            })),
            vec![kiln.path().to_path_buf()],
        )
        .await;

        assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "{body}");
    }

    // =====================================================================
    // The reply writes back the document it was given
    // =====================================================================

    /// Every node kind, every edge attribute and every key outside the spec
    /// survives [`CanvasResponse`].
    ///
    /// Built from a parsed [`Canvas`] rather than from a literal, so a field
    /// added to the core type and written to the wire fails here instead of
    /// vanishing between the parser and the browser. That loss is the risk this
    /// route took on when it stopped answering `serde_json::Value`.
    #[test]
    fn a_canvas_reply_writes_back_the_document_it_parsed() {
        let document = every_kind_of_node();
        let canvas = Canvas::parse(&document.to_string()).unwrap();

        survives::<CanvasResponse>(&serde_json::json!({
            "canvas": canvas,
            "rejected": [],
            "kiln": "/vault",
        }));
    }

    // =====================================================================
    // The refusal reasons
    // =====================================================================

    /// Every reason a reference can be refused, as the sentence the reply
    /// carries.
    ///
    /// The wire value is a rendered [`RefError`], not a token, because the
    /// browser prints it. An exhaustive match, so a variant added to the core
    /// enum fails to compile here rather than reaching the reader as wording
    /// nobody chose; and each sentence is pinned, so a rewording is a visible
    /// change to this file.
    #[test]
    fn the_refusal_reasons_stay_whole_sentences() {
        fn sentence(reason: RefError) -> &'static str {
            match reason {
                RefError::Empty => "reference is empty",
                RefError::Absolute => {
                    "reference is an absolute path; canvas references must be kiln-relative"
                }
                RefError::Traversal => {
                    "reference escapes the kiln via a parent-directory component"
                }
                RefError::InteriorNul => "reference contains an interior NUL byte",
                RefError::OutsideKiln => "reference resolves outside the kiln that owns the canvas",
            }
        }

        const EVERY_REASON: &[RefError] = &[
            RefError::Empty,
            RefError::Absolute,
            RefError::Traversal,
            RefError::InteriorNul,
            RefError::OutsideKiln,
        ];

        for &reason in EVERY_REASON {
            assert_eq!(
                reason.to_string(),
                sentence(reason),
                "the wording the browser prints moved"
            );
        }
    }

    fn canvas_with(reference: &str) -> Canvas {
        Canvas::parse(
            &serde_json::json!({
                "nodes": [{
                    "id": "n1", "type": "file",
                    "x": 0, "y": 0, "width": 1, "height": 1,
                    "file": reference, "subpath": "#Heading"
                }]
            })
            .to_string(),
        )
        .unwrap()
    }

    /// The redaction is the enforcement. A client that is merely *told* a node
    /// is bad could still fetch the path; a client that never receives the path
    /// cannot.
    #[test]
    fn redaction_removes_the_offending_path_from_the_payload() {
        let tmp = TempDir::new().unwrap();
        let mut canvas = canvas_with("../../../etc/passwd");
        let rejected = validate_canvas(&canvas, tmp.path());
        assert_eq!(rejected.len(), 1);

        redact(&mut canvas, tmp.path());

        let serialized = serde_json::to_string(&canvas).unwrap();
        assert!(
            !serialized.contains("etc/passwd"),
            "the rejected path must not survive into the response: {serialized}"
        );
        assert!(
            !serialized.contains("#Heading"),
            "the subpath must be cleared alongside the file reference"
        );
    }

    /// Node ids are not enforced unique (a hand-edited file can repeat one), so
    /// redaction must never key off them: `find` returns the first match and
    /// the malicious twin survives with its path intact.
    #[test]
    fn duplicate_node_ids_do_not_defeat_redaction() {
        let tmp = TempDir::new().unwrap();
        let mut canvas = Canvas::parse(
            &serde_json::json!({
                "nodes": [
                    { "id": "dup", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                      "file": "Fine.md" },
                    { "id": "dup", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                      "file": "../../../etc/passwd" }
                ]
            })
            .to_string(),
        )
        .unwrap();

        redact(&mut canvas, tmp.path());

        let serialized = serde_json::to_string(&canvas).unwrap();
        assert!(
            !serialized.contains("etc/passwd"),
            "a duplicated id must not let the offending path escape: {serialized}"
        );
        assert!(
            serialized.contains("Fine.md"),
            "the valid twin must survive"
        );
    }

    /// The read path withholds a failing reference, so the document coming back
    /// has a blank where it was. Writing that through would permanently erase a
    /// reference the user was never shown and cannot recover.
    #[test]
    fn a_save_never_overwrites_a_withheld_reference_with_the_blank() {
        let tmp = TempDir::new().unwrap();

        // What is on disk: a reference that fails containment.
        let on_disk = canvas_with("../shared/Design.md");
        // What the client holds after GET: the same document, redacted.
        let mut served = on_disk.clone();
        redact(&mut served, tmp.path());
        assert_eq!(served.file_paths().collect::<Vec<_>>(), [""]);

        // The client sends that document back after any edit.
        let mut incoming = served.clone();
        restore_redacted(&mut incoming, &on_disk, tmp.path());

        assert_eq!(
            incoming.file_paths().collect::<Vec<_>>(),
            ["../shared/Design.md"],
            "the withheld reference must survive a round trip through the client"
        );
        assert!(
            !is_contained(&incoming, tmp.path()),
            "restoring must not pretend the reference became valid"
        );
    }

    /// Restoration must not resurrect a reference the user deliberately cleared.
    #[test]
    fn a_user_clearing_a_valid_reference_is_respected() {
        let tmp = TempDir::new().unwrap();
        let on_disk = canvas_with("Notes/Fine.md");

        let mut incoming = canvas_with("");
        restore_redacted(&mut incoming, &on_disk, tmp.path());

        assert_eq!(
            incoming.file_paths().collect::<Vec<_>>(),
            [""],
            "a contained path was never redacted, so clearing it was the user"
        );
    }

    /// A restructured document must decline to restore rather than pair the
    /// wrong nodes together.
    #[test]
    fn restoration_declines_when_nodes_no_longer_line_up() {
        let tmp = TempDir::new().unwrap();
        let on_disk = canvas_with("../escape.md");

        let mut incoming = Canvas::parse(
            &serde_json::json!({
                "nodes": [{
                    "id": "different", "type": "file",
                    "x": 0, "y": 0, "width": 1, "height": 1, "file": ""
                }]
            })
            .to_string(),
        )
        .unwrap();
        restore_redacted(&mut incoming, &on_disk, tmp.path());

        assert_eq!(incoming.file_paths().collect::<Vec<_>>(), [""]);
    }

    #[test]
    fn redaction_leaves_valid_nodes_untouched() {
        let tmp = TempDir::new().unwrap();
        let mut canvas = canvas_with("Notes/Fine.md");
        let rejected = validate_canvas(&canvas, tmp.path());
        assert!(rejected.is_empty());

        redact(&mut canvas, tmp.path());

        assert!(serde_json::to_string(&canvas)
            .unwrap()
            .contains("Notes/Fine.md"));
    }

    #[test]
    fn the_rejection_report_does_not_echo_the_offending_path() {
        let tmp = TempDir::new().unwrap();
        let canvas = canvas_with("/etc/shadow");
        let rejected = validate_canvas(&canvas, tmp.path());

        let json = serde_json::to_string(&rejected_dtos(&rejected)).unwrap();
        assert!(
            json.contains("n1"),
            "the node id is needed to place the placeholder"
        );
        assert!(
            !json.contains("shadow"),
            "the path must not be reflected back: {json}"
        );
    }

    /// The commonest edit there is — adding a card — used to shift every
    /// index, so a position-paired restore declined and wrote the blank away.
    #[test]
    fn a_withheld_reference_survives_a_node_being_added_above_it() {
        let tmp = TempDir::new().unwrap();
        let on_disk = canvas_with("../shared/Design.md");

        let mut served = on_disk.clone();
        redact(&mut served, tmp.path());

        // The client prepends a card, which shifts every index by one.
        let mut incoming = served.clone();
        incoming.nodes.insert(
            0,
            Canvas::parse(
                &serde_json::json!({
                    "nodes": [{ "id": "added", "type": "text", "x": 0, "y": 0,
                                "width": 1, "height": 1, "text": "new" }]
                })
                .to_string(),
            )
            .unwrap()
            .nodes
            .remove(0),
        );

        restore_redacted(&mut incoming, &on_disk, tmp.path());

        assert_eq!(
            incoming.file_paths().collect::<Vec<_>>(),
            ["../shared/Design.md"],
            "restoring must match by id, not by position"
        );
    }

    #[test]
    fn a_group_background_is_redacted_too() {
        let tmp = TempDir::new().unwrap();
        let mut canvas = Canvas::parse(
            &serde_json::json!({
                "nodes": [{
                    "id": "g", "type": "group",
                    "x": 0, "y": 0, "width": 10, "height": 10,
                    "label": "keep me", "background": "../../outside.png"
                }]
            })
            .to_string(),
        )
        .unwrap();

        redact(&mut canvas, tmp.path());

        let serialized = serde_json::to_string(&canvas).unwrap();
        assert!(!serialized.contains("outside.png"));
        assert!(
            serialized.contains("keep me"),
            "redaction must be surgical, not destroy the node"
        );
    }
}
