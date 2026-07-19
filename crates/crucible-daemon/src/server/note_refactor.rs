//! `note.rename` / `note.move` — rename a note and rewrite every inbound
//! wikilink that resolves to it (Phase 3 of the file-tree work).
//!
//! The algorithm is the convergent recipe shared by Foam/SilverBullet/Dendron
//! (see the Phase-3 plan §3): query the RESOLVED link index for exactly the
//! rows pointing at the old path, splice each row's target token by stored
//! byte span in DESCENDING order (so earlier offsets stay valid), rename the
//! file, write the edited sources, re-index. Splicing only the target token
//! preserves aliases (`[[Old|x]]`), heading/block refs (`[[Old#h]]`,
//! `[[Old#^b]]`), and embed markers (`![[Old]]`) because they all lie outside
//! the spliced range.
//!
//! Safety properties:
//! - **Ambiguous rows are never rewritten** (`is_ambiguous = 1` — the stem is
//!   shared by ≥2 notes); they are returned as warnings instead.
//! - **Span verification before splice**: the bytes at a stored span must
//!   equal the stored raw target, otherwise the row is stale (file changed
//!   under us) and is skipped with a warning — a drifted index can never
//!   corrupt a note.
//! - **Author link style is preserved**: `[[notes/Old]]` (path form) becomes
//!   `[[archive/New]]`, while `[[Old]]` (bare form) becomes `[[New]]`.
//!
//! Watcher reconciliation is the idempotent strategy (Phase-3 plan §5d B):
//! we reindex inline; the watcher's later debounced reprocess of the same
//! files is a content-hash no-op.

use crate::kiln_manager::KilnManager;
use crate::protocol::{Request, Response, INTERNAL_ERROR, INVALID_PARAMS};
use crate::rpc_helpers::require_param;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// One inbound reference that was intentionally left untouched.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SkippedRef {
    pub source_path: String,
    pub raw_target: String,
    /// `"ambiguous"` (stem shared by several notes) or `"stale-span"`
    /// (file bytes no longer match the index; reindex will catch up).
    pub reason: &'static str,
}

/// Outcome of a rename/move, returned to the caller for UX ("N links
/// updated, M ambiguous links skipped").
#[derive(Debug, serde::Serialize)]
pub(crate) struct RenameOutcome {
    pub from: String,
    pub to: String,
    pub rewritten_sources: Vec<String>,
    pub skipped: Vec<SkippedRef>,
}

/// Handle `note.rename` / `note.move` (one operation, two RPC names).
/// Params: `{ kiln, from_rel, to_rel }`. The kiln must be OPEN (fail-closed,
/// same reasoning as `fs.move`), both paths must be `.md`, and all path
/// containment is delegated to the same validated move used by `fs.move`.
pub(crate) async fn handle_note_rename(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln = require_param!(req, "kiln", as_str);
    let from_rel = require_param!(req, "from_rel", as_str);
    let to_rel = require_param!(req, "to_rel", as_str);

    let Ok(canonical) = Path::new(kiln).canonicalize() else {
        return Response::error(req.id, INVALID_PARAMS, "kiln path does not exist");
    };
    if km.get(&canonical).await.is_none() {
        return Response::error(req.id, INVALID_PARAMS, "kiln is not open");
    }

    match rename_note(km, &canonical, from_rel, to_rel).await {
        Ok(outcome) => match serde_json::to_value(&outcome) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        Err(RenameError::Move(e)) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
        Err(RenameError::NotMarkdown) => Response::error(
            req.id,
            INVALID_PARAMS,
            "note.rename operates on .md notes (use fs.move for other files)",
        ),
        Err(RenameError::Other(e)) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RenameError {
    #[error(transparent)]
    Move(#[from] crate::server::fs::FsMoveError),
    #[error("not a markdown note")]
    NotMarkdown,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Rename `from_rel` → `to_rel` inside the (open, canonical) kiln, rewriting
/// unambiguous inbound links. See the module docs for the safety properties.
pub(crate) async fn rename_note(
    km: &Arc<KilnManager>,
    kiln_root: &Path,
    from_rel: &str,
    to_rel: &str,
) -> Result<RenameOutcome, RenameError> {
    if !from_rel.ends_with(".md") || !to_rel.ends_with(".md") {
        return Err(RenameError::NotMarkdown);
    }

    let store = km
        .get(kiln_root)
        .await
        .ok_or_else(|| anyhow::anyhow!("kiln not open"))?
        .as_note_store();

    // 1. Compute staged rewrites BEFORE touching the disk.
    let inbound = store
        .inbound_links(from_rel)
        .await
        .map_err(|e| anyhow::anyhow!("inbound_links: {e}"))?;

    let mut by_source: BTreeMap<String, Vec<&crucible_core::storage::InboundLink>> =
        BTreeMap::new();
    let mut skipped = Vec::new();
    for link in &inbound {
        if link.is_ambiguous {
            skipped.push(SkippedRef {
                source_path: link.source_path.clone(),
                raw_target: link.raw_target.clone(),
                reason: "ambiguous",
            });
            continue;
        }
        if link.span_start < 0 {
            // Span-less legacy row (links_to fallback) — resolvable but not
            // spliceable; the source will still re-resolve after the move.
            skipped.push(SkippedRef {
                source_path: link.source_path.clone(),
                raw_target: link.raw_target.clone(),
                reason: "stale-span",
            });
            continue;
        }
        by_source
            .entry(link.source_path.clone())
            .or_default()
            .push(link);
    }

    let mut staged: Vec<(String, std::path::PathBuf, Vec<u8>)> = Vec::new();
    for (source, mut refs) in by_source {
        // A note's own row set never includes itself pointing at `from` after
        // the move — but a self-link is still a valid rewrite target when the
        // note IS `from`; its file is about to move, so stage against `to`.
        let source_is_moved_note = source == from_rel;
        let disk_rel = &source;
        let path = kiln_root.join(disk_rel);
        let mut bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                for r in refs {
                    skipped.push(SkippedRef {
                        source_path: r.source_path.clone(),
                        raw_target: r.raw_target.clone(),
                        reason: "stale-span",
                    });
                }
                tracing::warn!(source = %source, error = %e, "rename rewrite: source unreadable, skipped");
                continue;
            }
        };

        refs.sort_by_key(|r| std::cmp::Reverse(r.span_start));
        let mut edited = false;
        for r in refs {
            let (start, end) = (r.span_start as usize, r.span_end as usize);
            // Span verification: never splice bytes that don't match the
            // indexed raw target (stale index must not corrupt files).
            if end > bytes.len() || &bytes[start..end] != r.raw_target.as_bytes() {
                skipped.push(SkippedRef {
                    source_path: r.source_path.clone(),
                    raw_target: r.raw_target.clone(),
                    reason: "stale-span",
                });
                continue;
            }
            let replacement = render_target(&r.raw_target, to_rel);
            // A pure folder move leaves stem-addressed links textually
            // identical — skip the no-op so unrelated files keep their
            // mtimes and the watcher has nothing to chew on (the index
            // repoints via re-resolution, not text).
            if replacement.as_bytes() == &bytes[start..end] {
                continue;
            }
            bytes.splice(start..end, replacement.into_bytes());
            edited = true;
        }
        if edited {
            let write_path = if source_is_moved_note {
                kiln_root.join(to_rel)
            } else {
                path
            };
            staged.push((source, write_path, bytes));
        }
    }

    // 2. Rename on disk — the same validated move fs.move uses (containment,
    //    overwrite refusal, symlink-as-link semantics). Unlike a DnD drop
    //    (which always targets an existing folder) an explicit rename may
    //    name a new folder: whitelist the components ourselves, then create
    //    the parents inside the root before the validated move re-checks.
    let to_p = Path::new(to_rel);
    if to_p.is_absolute()
        || to_rel.contains('\0')
        || to_p
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(RenameError::Move(crate::server::fs::FsMoveError::Escape));
    }
    if let Some(parent) = to_p.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(kiln_root.join(parent))
            .map_err(|e| anyhow::anyhow!("creating destination folder: {e}"))?;
    }
    crate::server::fs::move_within(kiln_root, from_rel, to_rel)?;

    // 3. Write the staged inbound-source edits.
    let mut rewritten_sources = Vec::new();
    for (source, path, bytes) in staged {
        match std::fs::write(&path, &bytes) {
            Ok(()) => rewritten_sources.push(source),
            Err(e) => {
                tracing::warn!(source = %source, error = %e, "rename rewrite: write failed");
                skipped.push(SkippedRef {
                    source_path: source,
                    raw_target: String::new(),
                    reason: "stale-span",
                });
            }
        }
    }

    // 4. Re-index inline (the watcher's later pass is an idempotent no-op):
    //    old identity out, new identity in, edited sources re-parsed. The
    //    upsert/delete re-resolution steps repoint every remaining bare-stem
    //    row automatically.
    if let Err(e) = km
        .handle_file_deleted(kiln_root, &kiln_root.join(from_rel))
        .await
    {
        tracing::warn!(error = %e, "rename: deleting old index row failed");
    }
    // Forced: the destination path's change-detection state can be stale
    // but hash-identical (A→B→A round trip), which would silently skip the
    // reindex that repoints every inbound row.
    if let Err(e) = km
        .process_file_forced(kiln_root, &kiln_root.join(to_rel))
        .await
    {
        tracing::warn!(error = %e, "rename: indexing new path failed");
    }
    for source in &rewritten_sources {
        if source == from_rel {
            continue; // already indexed under `to_rel`
        }
        if let Err(e) = km.process_file(kiln_root, &kiln_root.join(source)).await {
            tracing::warn!(source = %source, error = %e, "rename: reindexing edited source failed");
        }
    }

    Ok(RenameOutcome {
        from: from_rel.to_string(),
        to: to_rel.to_string(),
        rewritten_sources,
        skipped,
    })
}

/// Render the rewritten target token, preserving the author's link style:
/// a path-form raw (`notes/Old` or `notes/Old.md`) becomes the new PATH
/// (extension kept only if the raw had it); a bare raw (`Old`) becomes the
/// new bare STEM.
pub(crate) fn render_target(raw: &str, to_rel: &str) -> String {
    let to_extless = to_rel.strip_suffix(".md").unwrap_or(to_rel);
    // Path form follows the new path; bare form follows the new stem —
    // a bare-with-extension raw (`[[target.md]]`) addresses by stem, so it
    // keeps the stem AND its extension.
    let base = if raw.contains('/') {
        to_extless.to_string()
    } else {
        to_extless
            .rsplit('/')
            .next()
            .unwrap_or(to_extless)
            .to_string()
    };
    if raw.ends_with(".md") {
        format!("{base}.md")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn render_preserves_link_style() {
        // bare → bare stem
        assert_eq!(render_target("Old", "archive/New.md"), "New");
        // path form → new path, extension-less
        assert_eq!(render_target("notes/Old", "archive/New.md"), "archive/New");
        // path form WITH extension keeps it
        assert_eq!(
            render_target("notes/Old.md", "archive/New.md"),
            "archive/New.md"
        );
        // bare form WITH extension keeps stem addressing + extension
        assert_eq!(render_target("Old.md", "archive/New.md"), "New.md");
        // move to root
        assert_eq!(render_target("notes/Old", "New.md"), "New");
        assert_eq!(render_target("Old", "New.md"), "New");
    }

    // =====================================================================
    // Rename fixture corpus (Phase-3 plan §6) — real temp kiln, real index.
    // =====================================================================

    async fn kiln_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Arc<KilnManager>, PathBuf) {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        for (rel, content) in files {
            let p = root.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
        }
        let km = Arc::new(KilnManager::new());
        km.open_and_process(&root, false).await.unwrap();
        (tmp, km, root)
    }

    fn read(root: &Path, rel: &str) -> String {
        std::fs::read_to_string(root.join(rel)).unwrap()
    }

    /// Alias / heading / block-ref / embed / combined forms all survive a
    /// rename because only the target token is spliced.
    #[tokio::test]
    async fn rename_rewrites_all_link_forms_preserving_decorations() {
        let (_tmp, km, root) = kiln_with(&[
            ("Old.md", "# Old\n"),
            (
                "linker.md",
                "a [[Old]] b [[Old|Display]] c [[Old#Setup]] d [[Old#^abc]] e ![[Old]] f ![[Old#S|see]]",
            ),
        ])
        .await;

        let out = rename_note(&km, &root, "Old.md", "New.md").await.unwrap();
        assert_eq!(out.rewritten_sources, vec!["linker.md"]);
        assert!(out.skipped.is_empty());
        assert_eq!(
            read(&root, "linker.md"),
            "a [[New]] b [[New|Display]] c [[New#Setup]] d [[New#^abc]] e ![[New]] f ![[New#S|see]]"
        );
        assert!(!root.join("Old.md").exists());
        assert!(root.join("New.md").exists());
    }

    /// Ambiguous-stem links are provably untouched and surfaced as warnings.
    #[tokio::test]
    async fn rename_never_touches_ambiguous_links() {
        let (_tmp, km, root) = kiln_with(&[
            ("concurrency/async.md", "# A\n"),
            ("rust/async.md", "# B\n"),
            ("linker.md", "see [[async]] here"),
        ])
        .await;

        let out = rename_note(&km, &root, "rust/async.md", "rust/tokio.md")
            .await
            .unwrap();
        assert_eq!(read(&root, "linker.md"), "see [[async]] here", "untouched");
        assert!(
            out.skipped.iter().any(|s| s.reason == "ambiguous"),
            "warning emitted: {:?}",
            out.skipped
        );
        assert!(out.rewritten_sources.is_empty());
    }

    /// Path-style raw links keep their path style; bare links stay bare.
    #[tokio::test]
    async fn rename_preserves_author_path_style() {
        let (_tmp, km, root) = kiln_with(&[
            ("notes/Old.md", "# Old\n"),
            ("linker.md", "p [[notes/Old]] q [[Old]] r [[notes/Old.md]]"),
        ])
        .await;

        rename_note(&km, &root, "notes/Old.md", "archive/New.md")
            .await
            .unwrap();
        assert_eq!(
            read(&root, "linker.md"),
            "p [[archive/New]] q [[New]] r [[archive/New.md]]"
        );
    }

    /// Multi-byte UTF-8 before a link: splice happens at the right BYTES.
    #[tokio::test]
    async fn rename_splices_correct_bytes_after_multibyte() {
        let (_tmp, km, root) = kiln_with(&[
            ("Old.md", "# Old\n"),
            ("linker.md", "emoji 🎉🎉 then [[Old]] and [[Old|x]] done"),
        ])
        .await;

        rename_note(&km, &root, "Old.md", "Renamed.md")
            .await
            .unwrap();
        assert_eq!(
            read(&root, "linker.md"),
            "emoji 🎉🎉 then [[Renamed]] and [[Renamed|x]] done"
        );
    }

    /// Wikilinks inside code are never indexed, so never rewritten.
    #[tokio::test]
    async fn rename_leaves_code_blocks_alone() {
        let (_tmp, km, root) = kiln_with(&[
            ("Old.md", "# Old\n"),
            (
                "linker.md",
                "real [[Old]]\n\n```\nfenced [[Old]]\n```\n\ninline `[[Old]]` end",
            ),
        ])
        .await;

        rename_note(&km, &root, "Old.md", "New.md").await.unwrap();
        assert_eq!(
            read(&root, "linker.md"),
            "real [[New]]\n\n```\nfenced [[Old]]\n```\n\ninline `[[Old]]` end"
        );
    }

    /// A note that links to itself: the moved file's own occurrence is
    /// rewritten and lands at the NEW path.
    #[tokio::test]
    async fn rename_rewrites_self_references() {
        let (_tmp, km, root) = kiln_with(&[("Old.md", "I link to [[Old]] (myself)")]).await;

        let out = rename_note(&km, &root, "Old.md", "sub/New.md")
            .await
            .unwrap();
        assert_eq!(out.rewritten_sources, vec!["Old.md"]);
        assert_eq!(read(&root, "sub/New.md"), "I link to [[New]] (myself)");
    }

    /// Dangling links to other targets and unrelated files stay untouched;
    /// a leaf note (no inbound) renames cleanly.
    #[tokio::test]
    async fn rename_ignores_dangling_and_unrelated() {
        let (_tmp, km, root) = kiln_with(&[
            ("Old.md", "# Old\n"),
            ("other.md", "links [[Ghost]] and [[unrelated/path]]"),
        ])
        .await;

        let out = rename_note(&km, &root, "Old.md", "New.md").await.unwrap();
        assert!(out.rewritten_sources.is_empty());
        assert!(out.skipped.is_empty());
        assert_eq!(
            read(&root, "other.md"),
            "links [[Ghost]] and [[unrelated/path]]"
        );
    }

    /// THE move invariant, full pipeline: moving a note into a folder keeps
    /// bare-stem links resolving (index repoints, text may stay identical)
    /// and rewrites path-style links; moving back out converges again.
    #[tokio::test]
    async fn move_into_and_out_of_folder_keeps_links_resolving() {
        let (_tmp, km, root) = kiln_with(&[
            ("target.md", "# T\n"),
            ("bare.md", "see [[target]]"),
            ("pathy.md", "see [[target.md]]"),
        ])
        .await;
        let store = km.get(&root).await.unwrap().as_note_store();

        // IN: root → folder
        rename_note(&km, &root, "target.md", "deep/nested/target.md")
            .await
            .unwrap();
        assert_eq!(read(&root, "bare.md"), "see [[target]]", "bare style kept");
        // bare-with-extension addresses by stem: text unchanged, still resolves
        assert_eq!(read(&root, "pathy.md"), "see [[target.md]]");
        let mut back = store.backlinks("deep/nested/target.md").await.unwrap();
        back.sort();
        assert_eq!(back, vec!["bare.md".to_string(), "pathy.md".to_string()]);
        assert!(store.backlinks("target.md").await.unwrap().is_empty());

        // OUT: folder → root
        rename_note(&km, &root, "deep/nested/target.md", "target.md")
            .await
            .unwrap();
        assert_eq!(read(&root, "bare.md"), "see [[target]]");
        assert_eq!(read(&root, "pathy.md"), "see [[target.md]]");
        let mut back = store.backlinks("target.md").await.unwrap();
        back.sort();
        assert_eq!(back, vec!["bare.md".to_string(), "pathy.md".to_string()]);
    }

    /// Overwrites are refused before anything is touched.
    #[tokio::test]
    async fn rename_refuses_overwrite() {
        let (_tmp, km, root) = kiln_with(&[
            ("a.md", "# A\n"),
            ("b.md", "# B\n"),
            ("linker.md", "see [[a]]"),
        ])
        .await;

        let err = rename_note(&km, &root, "a.md", "b.md").await.unwrap_err();
        assert!(matches!(err, RenameError::Move(_)));
        assert_eq!(read(&root, "b.md"), "# B\n");
        assert_eq!(read(&root, "linker.md"), "see [[a]]", "nothing rewritten");
    }

    /// Frontmatter [[links]] are not extracted (parser strips frontmatter
    /// before extensions run) → never rewritten.
    #[tokio::test]
    async fn rename_leaves_frontmatter_links_alone() {
        let (_tmp, km, root) = kiln_with(&[
            ("Old.md", "# Old\n"),
            (
                "linker.md",
                "---\nrelated: \"[[Old]]\"\n---\nbody [[Old]] end",
            ),
        ])
        .await;

        rename_note(&km, &root, "Old.md", "New.md").await.unwrap();
        assert_eq!(
            read(&root, "linker.md"),
            "---\nrelated: \"[[Old]]\"\n---\nbody [[New]] end"
        );
    }
}
