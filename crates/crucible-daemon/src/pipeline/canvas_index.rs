//! Turning a `.canvas` document into an indexable [`NoteRecord`].
//!
//! A canvas earns a row in the note index because it *links*. Its `file` nodes
//! name notes, and its `text` nodes hold markdown that can contain wikilinks.
//! Without indexing it, opening a note would show no sign that a canvas
//! references it — which is exactly the gap Obsidian has, where canvas
//! references never appear in backlinks.
//!
//! # Why the links carry no spans
//!
//! [`NoteRecord::links`] holds byte spans so a rename can splice the new target
//! into the source text. That machinery is right for markdown and wrong here: a
//! canvas reference lives inside a JSON string, where splicing would have to
//! reason about escaping, and where the surrounding document must round-trip
//! unknown keys intact. Canvas renames therefore go through the typed model
//! (see the rename path), and this module emits only `links_to`.
//!
//! That is a supported shape rather than a workaround — `write_links` gives
//! span-less callers negative sentinel spans, which stay resolvable and
//! backlinkable but are never spliced.

use std::path::Path;

use anyhow::{Context, Result};
use crucible_core::canvas::{Canvas, NodeKind};
use crucible_core::parser::BlockHash;
use crucible_core::storage::note_store::NoteRecord;

/// What a canvas contributes to the knowledge graph.
#[derive(Debug, Default, PartialEq)]
pub struct CanvasLinks {
    /// Raw link targets: `file` node paths, then wikilink targets found in
    /// `text` nodes, in document order.
    pub links_to: Vec<String>,
    /// Tags found in `text` node markdown.
    pub tags: Vec<String>,
}

/// Extract the links and tags a canvas contributes.
///
/// `file` node paths are emitted as-is; they are kiln-relative paths and the
/// link resolver already accepts a path-form target. Wikilinks inside `text`
/// nodes are parsed with the ordinary markdown parser, so a canvas card
/// containing `[[Some Note]]` links exactly as a note would.
pub async fn extract_links(
    canvas: &Canvas,
    parser: &dyn crucible_core::parser::MarkdownParser,
    source_path: &Path,
) -> CanvasLinks {
    let mut links_to = Vec::new();
    let mut tags = Vec::new();

    for node in &canvas.nodes {
        match &node.kind {
            NodeKind::File { file, .. } if !file.is_empty() => {
                links_to.push(file.clone());
            }
            NodeKind::Text { text } => {
                // A parse failure in one card must not cost the whole canvas
                // its other links — text nodes are user prose, not structure.
                if let Ok(parsed) = parser.parse_content(text, source_path).await {
                    links_to.extend(parsed.wikilinks.iter().map(|w| w.target.clone()));
                    tags.extend(parsed.all_tags());
                }
            }
            _ => {}
        }
    }

    tags.sort();
    tags.dedup();

    CanvasLinks { links_to, tags }
}

/// Build the index record for a canvas at `path`.
///
/// `storage_path` is the kiln-relative path used as the primary key, matching
/// how notes are stored.
pub async fn canvas_to_record(
    path: &Path,
    storage_path: &str,
    parser: &dyn crucible_core::parser::MarkdownParser,
    kiln_root: Option<&Path>,
) -> Result<NoteRecord> {
    let source = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("reading canvas '{}'", path.display()))?;

    let canvas =
        Canvas::parse(&source).with_context(|| format!("parsing canvas '{}'", path.display()))?;

    let links = extract_links(&canvas, parser, path).await;

    let content_hash = BlockHash::new(*blake3::hash(source.as_bytes()).as_bytes());

    // A canvas has no frontmatter and no headings, so the filename is the only
    // title available — the same fallback a note without either gets.
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(storage_path)
        .to_string();

    let mut properties = std::collections::HashMap::new();
    super::note_pipeline::stamp_scope_on_properties(&mut properties, kiln_root);

    Ok(NoteRecord {
        path: storage_path.to_string(),
        content_hash,
        embedding: None,
        embedding_model: None,
        embedding_dimensions: None,
        title,
        tags: links.tags,
        links_to: links.links_to,
        // Deliberately empty — see the module docs.
        links: Vec::new(),
        properties,
        updated_at: chrono::Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::parser::implementation::CrucibleParser;

    fn parser() -> CrucibleParser {
        CrucibleParser::new()
    }

    fn canvas(json: serde_json::Value) -> Canvas {
        Canvas::parse(&json.to_string()).unwrap()
    }

    #[tokio::test]
    async fn file_nodes_become_links() {
        let c = canvas(serde_json::json!({
            "nodes": [
                { "id": "a", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                  "file": "Help/Wikilinks.md" },
                { "id": "b", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                  "file": "Meta/Systems.md" }
            ]
        }));

        let links = extract_links(&c, &parser(), Path::new("board.canvas")).await;
        assert_eq!(links.links_to, ["Help/Wikilinks.md", "Meta/Systems.md"]);
    }

    /// Obsidian does not surface wikilinks written inside canvas text cards in
    /// backlinks. Doing so is a deliberate improvement, so it needs a test that
    /// would notice if it silently stopped working.
    #[tokio::test]
    async fn wikilinks_inside_text_nodes_become_links() {
        let c = canvas(serde_json::json!({
            "nodes": [{
                "id": "t", "type": "text", "x": 0, "y": 0, "width": 1, "height": 1,
                "text": "See [[Another Note]] and [[Third|aliased]] #topic"
            }]
        }));

        let links = extract_links(&c, &parser(), Path::new("board.canvas")).await;
        assert!(links.links_to.contains(&"Another Note".to_string()));
        assert!(links.links_to.contains(&"Third".to_string()));
        assert!(links.tags.contains(&"topic".to_string()));
    }

    #[tokio::test]
    async fn link_and_group_nodes_contribute_nothing() {
        let c = canvas(serde_json::json!({
            "nodes": [
                { "id": "l", "type": "link", "x": 0, "y": 0, "width": 1, "height": 1,
                  "url": "https://example.com/[[not-a-wikilink]]" },
                { "id": "g", "type": "group", "x": 0, "y": 0, "width": 1, "height": 1,
                  "label": "[[also not a link]]" }
            ]
        }));

        let links = extract_links(&c, &parser(), Path::new("board.canvas")).await;
        assert!(links.links_to.is_empty(), "{:?}", links.links_to);
    }

    /// A redacted file node (containment rejected it) has an empty path. It must
    /// not be emitted as a link to "".
    #[tokio::test]
    async fn a_redacted_file_node_contributes_no_link() {
        let c = canvas(serde_json::json!({
            "nodes": [{ "id": "a", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                        "file": "" }]
        }));

        let links = extract_links(&c, &parser(), Path::new("board.canvas")).await;
        assert!(links.links_to.is_empty());
    }

    #[tokio::test]
    async fn a_record_carries_links_but_never_spans() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("board.canvas");
        std::fs::write(
            &path,
            serde_json::json!({
                "nodes": [{ "id": "a", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                            "file": "Target.md" }]
            })
            .to_string(),
        )
        .unwrap();

        let record = canvas_to_record(&path, "board.canvas", &parser(), Some(tmp.path()))
            .await
            .unwrap();

        assert_eq!(record.path, "board.canvas");
        assert_eq!(record.title, "board");
        assert_eq!(record.links_to, ["Target.md"]);
        assert!(
            record.links.is_empty(),
            "canvas links must not carry spans; renames go through the typed model"
        );
    }

    #[tokio::test]
    async fn a_malformed_canvas_is_an_error_not_an_empty_record() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("broken.canvas");
        std::fs::write(&path, "{ this is not json").unwrap();

        assert!(
            canvas_to_record(&path, "broken.canvas", &parser(), Some(tmp.path()))
                .await
                .is_err(),
            "silently indexing a broken canvas as empty would erase its real links"
        );
    }
}
