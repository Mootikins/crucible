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

use axum::{extract::State, routing::get, Json, Router};
use crucible_core::canvas::containment::{resolve_file_ref, validate_canvas, RejectedRef};
use crucible_core::canvas::{Canvas, NodeKind};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;

use super::helpers::{
    reject_path_traversal, validate_file_within_kiln, validate_write_target_within_kiln,
    MAX_CONTENT_SIZE,
};
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};

pub fn canvas_routes() -> Router<AppState> {
    Router::new().route("/api/canvas", get(get_canvas).put(put_canvas))
}

#[derive(Debug, Deserialize)]
struct CanvasPathQuery {
    path: String,
}

#[derive(Debug, Deserialize)]
struct PutCanvasRequest {
    path: String,
    content: String,
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
async fn get_canvas(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<CanvasPathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
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

    Ok(Json(serde_json::json!({
        "canvas": canvas,
        "rejected": rejected_json(&rejected),
        "kiln": kiln,
    })))
}

/// `PUT /api/canvas` — write a canvas document.
///
/// This is the authoritative layer: the document is parsed and every reference
/// checked before anything touches disk. A canvas naming a file outside its
/// kiln is refused wholesale with the offending node ids, rather than being
/// written and cleaned up later.
async fn put_canvas(
    State(state): State<AppState>,
    Json(req): Json<PutCanvasRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    reject_path_traversal(&req.path)?;

    if req.content.len() > MAX_CONTENT_SIZE {
        return Err(WebError::Validation(format!(
            "Canvas too large: {} bytes (max {MAX_CONTENT_SIZE})",
            req.content.len()
        )));
    }

    let path = PathBuf::from(&req.path);
    let kiln = enclosing_kiln(&state, &path).await?;
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

    // Re-serialize from the parsed document rather than writing the request
    // bytes through. The round-trip is lossless for unknown keys, and this way
    // anything that parsed as a canvas is what lands on disk.
    let serialized = canvas
        .to_json_pretty()
        .map_err(|e| WebError::Validation(format!("Could not serialize canvas: {e}")))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(WebError::Io)?;
    }
    fs::write(&path, &serialized).await.map_err(WebError::Io)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Resolve the open kiln containing `path`, refusing project roots.
///
/// A canvas in a project but outside any kiln is not served at all, regardless
/// of that project's `project_files` policy — canvases are kiln content by
/// definition, and honouring the looser policy here would let a canvas act as a
/// reader for arbitrary repository files.
async fn enclosing_kiln(state: &AppState, path: &Path) -> Result<PathBuf, WebError> {
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
    kilns
        .into_iter()
        .filter_map(|kiln| {
            let canonical = kiln.canonicalize().ok()?;
            (path.starts_with(&canonical) || path.starts_with(&kiln)).then_some(canonical)
        })
        .max_by_key(|k| k.components().count())
        .ok_or_else(|| WebError::NotFound("Canvas is not within an open kiln".to_string()))
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
    let escapes = |reference: &str| resolve_file_ref(reference, kiln_root).is_err();
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

fn rejected_json(rejected: &[RejectedRef]) -> Vec<serde_json::Value> {
    rejected
        .iter()
        .map(|r| {
            serde_json::json!({
                "nodeId": r.node_id,
                // The offending path is deliberately NOT echoed back — the
                // reason is enough for the UI to explain the placeholder, and
                // echoing it would hand a probe its own answer.
                "reason": r.reason.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::canvas::Canvas;
    use tempfile::TempDir;

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

        let json = serde_json::to_string(&rejected_json(&rejected)).unwrap();
        assert!(
            json.contains("n1"),
            "the node id is needed to place the placeholder"
        );
        assert!(
            !json.contains("shadow"),
            "the path must not be reflected back: {json}"
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
