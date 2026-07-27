//! Whether a canvas may reference a given path.
//!
//! # The rule
//!
//! A `file` node (and a group's `background` image, which is equally a file
//! reference) must resolve **inside the kiln that owns the `.canvas` file**.
//! Not "any open kiln", and never a project root.
//!
//! This is deliberately stricter than the [`ProjectFileAccess`] policy that
//! governs the web file browser, which permits reading project files by
//! default. The reasons differ: the file browser is a browser, whereas a canvas
//! is portable knowledge content. A canvas that reaches outside its kiln
//! silently breaks the moment the kiln is copied, synced, or published — and a
//! canvas that reaches into a project root turns a knowledge document into an
//! exfiltration path for source code.
//!
//! [`ProjectFileAccess`]: crate::config::ProjectFileAccess
//!
//! # Defence in depth
//!
//! This module is layer 2 and 3 of three:
//!
//! 1. **UI (advisory)** — the drag-drop target filters to the owning kiln and
//!    explains rejections. Good UX, zero security value; a request can always
//!    be made directly.
//! 2. **Write path (authoritative)** — [`validate_canvas`] runs before a
//!    `.canvas` is persisted, and the write is refused naming the offending
//!    nodes.
//! 3. **Read path (fail-safe)** — the same check runs when a canvas is loaded,
//!    so a file hand-edited on disk to contain `../../../etc/passwd` renders as
//!    a quarantined broken node and its content is never fetched.
//!
//! Layer 3 is the one that matters. Without it, `vim` is a bypass for layers 1
//! and 2, and the render path would happily resolve whatever the JSON said.

use std::path::{Component, Path, PathBuf};

use super::{Canvas, NodeKind};

/// Why a canvas file reference was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RefError {
    /// The reference names nothing.
    ///
    /// Not a containment violation — an empty path cannot escape anything — and
    /// [`validate_canvas`] treats it as such, because it is exactly the state
    /// redaction leaves behind and rejecting it would make a redacted canvas
    /// unsavable. It is still an error rather than an `Ok`, so no caller is
    /// handed a path (the kiln root) that would *look* contained.
    #[error("reference is empty")]
    Empty,
    #[error("reference is an absolute path; canvas references must be kiln-relative")]
    Absolute,
    #[error("reference escapes the kiln via a parent-directory component")]
    Traversal,
    #[error("reference contains an interior NUL byte")]
    InteriorNul,
    #[error("reference resolves outside the kiln that owns the canvas")]
    OutsideKiln,
}

/// A rejected reference, carrying enough detail for the write path to tell the
/// caller exactly which nodes to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedRef {
    /// Id of the node holding the bad reference.
    pub node_id: String,
    /// The raw reference as written in the document.
    pub reference: String,
    pub reason: RefError,
}

/// Resolve a canvas file reference against the kiln that owns the canvas.
///
/// Returns the absolute path the reference denotes. A reference to a file that
/// does not exist is *not* an error — a canvas may legitimately point at a note
/// that has been deleted, and that should render as a broken card rather than
/// refusing the whole document. What is refused is anything that could name a
/// location outside the kiln.
///
/// Existence-dependent checks (symlinks) only apply when the target is really
/// there; the lexical checks apply always, so a non-existent path can never
/// sneak through by virtue of being absent.
pub fn resolve_file_ref(reference: &str, kiln_root: &Path) -> Result<PathBuf, RefError> {
    if reference.is_empty() {
        return Err(RefError::Empty);
    }
    if reference.contains('\0') {
        return Err(RefError::InteriorNul);
    }

    let raw = Path::new(reference);
    if raw.is_absolute() {
        return Err(RefError::Absolute);
    }

    // Reject `..` lexically rather than relying on canonicalization, because
    // canonicalization requires the path to exist and a reference to a deleted
    // note is legal. A prefix/root component can only appear on Windows
    // (`C:foo`, `\foo`) and is absolute in spirit, so it is refused too.
    for component in raw.components() {
        match component {
            Component::ParentDir => return Err(RefError::Traversal),
            Component::Prefix(_) | Component::RootDir => return Err(RefError::Absolute),
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let joined = kiln_root.join(raw);

    // A symlink inside the kiln can still point out of it, and that resolution
    // is only visible once the target exists.
    if joined.exists() {
        let canonical_root = kiln_root
            .canonicalize()
            .map_err(|_| RefError::OutsideKiln)?;
        let canonical = joined.canonicalize().map_err(|_| RefError::OutsideKiln)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(RefError::OutsideKiln);
        }
        return Ok(canonical);
    }

    Ok(joined)
}

/// Check every file reference a canvas makes, returning the rejections.
///
/// An empty result means the document is safe to persist or render. Both `file`
/// nodes and group `background` images are covered — a background is a file
/// reference like any other, and omitting it would leave a hole in the check
/// that is easy to miss and trivial to exploit.
pub fn validate_canvas(canvas: &Canvas, kiln_root: &Path) -> Vec<RejectedRef> {
    let mut rejected = Vec::new();

    for node in &canvas.nodes {
        let references = match &node.kind {
            NodeKind::File { file, .. } => vec![file.as_str()],
            NodeKind::Group { background, .. } => background.iter().map(String::as_str).collect(),
            // Text nodes hold inline markdown and link nodes hold URLs; neither
            // names a path on this machine.
            NodeKind::Text { .. } | NodeKind::Link { .. } => Vec::new(),
        };

        for reference in references {
            // `Empty` means "names nothing", which is the state redaction
            // leaves behind. Reporting it would make a redacted canvas
            // unsavable, and the user could not repair it because the offending
            // path was deliberately never sent to them.
            if let Err(reason) = resolve_file_ref(reference, kiln_root) {
                if reason == RefError::Empty {
                    continue;
                }
                rejected.push(RejectedRef {
                    node_id: node.id.clone(),
                    reference: reference.to_string(),
                    reason,
                });
            }
        }
    }

    rejected
}

/// Whether a canvas is safe to persist or render as-is.
pub fn is_contained(canvas: &Canvas, kiln_root: &Path) -> bool {
    validate_canvas(canvas, kiln_root).is_empty()
}

/// The root-independent half of [`resolve_file_ref`].
///
/// Absolute paths, `..` traversal and interior NULs are refusable without
/// knowing where the kiln is; only symlink resolution needs a root. Callers that
/// may not have one use this so their predicate still fails *closed* on the
/// dangerous shapes — a containment check whose no-root default is "allow" is
/// the sort that stops protecting anything the moment a caller changes.
pub fn is_lexically_contained(reference: &str) -> bool {
    if reference.is_empty() {
        return true;
    }
    if reference.contains('\0') {
        return false;
    }
    let raw = Path::new(reference);
    if raw.is_absolute() {
        return false;
    }
    raw.components()
        .all(|component| matches!(component, Component::CurDir | Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn canvas_with_file_ref(reference: &str) -> Canvas {
        Canvas::parse(
            &serde_json::json!({
                "nodes": [{
                    "id": "n1", "type": "file",
                    "x": 0, "y": 0, "width": 10, "height": 10,
                    "file": reference
                }]
            })
            .to_string(),
        )
        .unwrap()
    }

    #[test]
    fn a_plain_relative_reference_resolves_inside_the_kiln() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path();
        std::fs::create_dir_all(kiln.join("Help")).unwrap();
        std::fs::write(kiln.join("Help/Wikilinks.md"), "# W").unwrap();

        let resolved = resolve_file_ref("Help/Wikilinks.md", kiln).unwrap();
        assert!(resolved.starts_with(kiln.canonicalize().unwrap()));
    }

    /// A canvas may point at a note that has since been deleted — that renders
    /// as a broken card, it does not invalidate the document.
    #[test]
    fn a_reference_to_a_missing_file_is_allowed() {
        let tmp = TempDir::new().unwrap();
        assert!(resolve_file_ref("Notes/Gone.md", tmp.path()).is_ok());
    }

    #[test]
    fn parent_directory_traversal_is_refused() {
        let tmp = TempDir::new().unwrap();
        for attempt in [
            "../secrets.md",
            "Help/../../secrets.md",
            "./../../etc/passwd",
            "a/b/../../../out.md",
        ] {
            assert_eq!(
                resolve_file_ref(attempt, tmp.path()),
                Err(RefError::Traversal),
                "{attempt} must be refused"
            );
        }
    }

    #[test]
    fn absolute_references_are_refused() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            resolve_file_ref("/etc/passwd", tmp.path()),
            Err(RefError::Absolute)
        );
    }

    #[test]
    fn nul_references_are_refused() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            resolve_file_ref("a\0b.md", tmp.path()),
            Err(RefError::InteriorNul)
        );
    }

    /// The read path blanks a rejected reference. If that state were itself a
    /// violation, the document could never be saved again — and the user could
    /// not repair it, because the offending path was deliberately never sent.
    #[test]
    fn an_empty_reference_is_inert_not_a_violation() {
        let tmp = TempDir::new().unwrap();

        // An error, so no caller is handed the kiln root as a "contained" path…
        assert_eq!(resolve_file_ref("", tmp.path()), Err(RefError::Empty));

        // …but not a violation, so a redacted canvas stays savable.
        assert!(
            is_contained(&canvas_with_file_ref(""), tmp.path()),
            "a redacted canvas must remain savable"
        );
    }

    /// The lexical checks pass here — no `..`, not absolute — so only symlink
    /// resolution catches this. It is exactly the shape a `.canvas` hand-edited
    /// on disk would take to reach a project root.
    #[cfg(unix)]
    #[test]
    fn a_symlink_escaping_the_kiln_is_refused() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("kiln");
        let outside = tmp.path().join("project");
        std::fs::create_dir_all(&kiln).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secrets.rs"), "const KEY: &str = \"…\";").unwrap();

        std::os::unix::fs::symlink(&outside, kiln.join("escape")).unwrap();

        assert_eq!(
            resolve_file_ref("escape/secrets.rs", &kiln),
            Err(RefError::OutsideKiln),
            "a symlink out of the kiln must not be reachable from a canvas"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_staying_inside_the_kiln_is_allowed() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path();
        std::fs::create_dir_all(kiln.join("real")).unwrap();
        std::fs::write(kiln.join("real/note.md"), "# N").unwrap();
        std::os::unix::fs::symlink(kiln.join("real"), kiln.join("alias")).unwrap();

        assert!(resolve_file_ref("alias/note.md", kiln).is_ok());
    }

    #[test]
    fn validate_canvas_names_every_offending_node() {
        let tmp = TempDir::new().unwrap();
        let canvas = Canvas::parse(
            &serde_json::json!({
                "nodes": [
                    { "id": "ok", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                      "file": "Fine.md" },
                    { "id": "bad-1", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                      "file": "../../etc/passwd" },
                    { "id": "bad-2", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                      "file": "/absolute.md" }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let rejected = validate_canvas(&canvas, tmp.path());
        let ids: Vec<&str> = rejected.iter().map(|r| r.node_id.as_str()).collect();

        assert_eq!(ids, ["bad-1", "bad-2"]);
        assert_eq!(rejected[0].reason, RefError::Traversal);
        assert_eq!(rejected[1].reason, RefError::Absolute);
        assert!(!is_contained(&canvas, tmp.path()));
    }

    /// A group's background is a file reference too. Checking only `file` nodes
    /// leaves a hole that is easy to miss and trivial to walk through.
    #[test]
    fn a_group_background_is_validated_like_any_other_reference() {
        let tmp = TempDir::new().unwrap();
        let canvas = Canvas::parse(
            &serde_json::json!({
                "nodes": [{
                    "id": "g", "type": "group",
                    "x": 0, "y": 0, "width": 100, "height": 100,
                    "background": "../../../etc/shadow"
                }]
            })
            .to_string(),
        )
        .unwrap();

        let rejected = validate_canvas(&canvas, tmp.path());
        assert_eq!(rejected.len(), 1, "group backgrounds must be checked");
        assert_eq!(rejected[0].reason, RefError::Traversal);
    }

    #[test]
    fn text_and_link_nodes_name_no_paths() {
        let tmp = TempDir::new().unwrap();
        let canvas = Canvas::parse(
            &serde_json::json!({
                "nodes": [
                    { "id": "t", "type": "text", "x": 0, "y": 0, "width": 1, "height": 1,
                      "text": "../../not a path, just prose" },
                    { "id": "l", "type": "link", "x": 0, "y": 0, "width": 1, "height": 1,
                      "url": "file:///etc/passwd" }
                ]
            })
            .to_string(),
        )
        .unwrap();

        assert!(
            is_contained(&canvas, tmp.path()),
            "prose and URLs are not filesystem references"
        );
    }

    #[test]
    fn a_canvas_with_only_valid_references_is_contained() {
        let tmp = TempDir::new().unwrap();
        assert!(is_contained(
            &canvas_with_file_ref("Notes/Fine.md"),
            tmp.path()
        ));
    }
}
