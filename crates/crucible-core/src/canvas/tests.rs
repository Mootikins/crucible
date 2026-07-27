use super::*;
use serde_json::json;

/// A canvas exercising every spec node type, every edge attribute, and unknown
/// keys at all three levels (document, node, edge) — the shape a real Obsidian
/// vault with plugins installed produces.
fn adversarial_canvas() -> Value {
    json!({
        "nodes": [
            {
                "id": "text-1", "type": "text",
                "x": -420, "y": 96, "width": 250, "height": 120,
                "text": "# Heading\n\nA [[Wikilink]] and `code`.",
                "color": "3",
                "styleAttributes": { "border": "dashed", "shape": "diamond" }
            },
            {
                "id": "file-1", "type": "file",
                "x": 0, "y": 0, "width": 400, "height": 400,
                "file": "Help/Wikilinks.md",
                "subpath": "#Syntax",
                "color": "#FF00AA"
            },
            {
                "id": "file-2", "type": "file",
                "x": 500, "y": 0, "width": 400, "height": 400,
                "file": "Meta/Systems.md"
            },
            {
                "id": "link-1", "type": "link",
                "x": 0, "y": 500, "width": 300, "height": 200,
                "url": "https://jsoncanvas.org"
            },
            {
                "id": "group-1", "type": "group",
                "x": -50, "y": -50, "width": 1000, "height": 500,
                "label": "Q3 work",
                "background": "assets/bg.png",
                "backgroundStyle": "cover"
            }
        ],
        "edges": [
            {
                "id": "edge-1",
                "fromNode": "file-1", "fromSide": "right", "fromEnd": "arrow",
                "toNode": "file-2", "toSide": "left", "toEnd": "none",
                "color": "5", "label": "supersedes",
                "customRouting": { "waypoints": [[10, 20]] }
            },
            { "id": "edge-2", "fromNode": "text-1", "toNode": "file-1" }
        ],
        "metadata": { "version": "obsidian-1.5.3", "frontmatter": null }
    })
}

/// THE contract. Unknown keys at every level survive parse → serialize.
///
/// If this fails, opening a canvas in Crucible and saving it destroys data that
/// Obsidian or its plugins authored. Fix the types; never relax the test.
#[test]
fn unknown_keys_survive_a_round_trip_at_every_level() {
    let original = adversarial_canvas();
    let source = serde_json::to_string_pretty(&original).unwrap();

    let canvas = Canvas::parse(&source).expect("adversarial canvas should parse");
    let reserialized: Value = serde_json::from_str(&canvas.to_json_pretty().unwrap()).unwrap();

    assert_eq!(
        reserialized, original,
        "round trip must be lossless, including keys the spec does not define"
    );
}

#[test]
fn round_trip_is_idempotent() {
    let source = serde_json::to_string_pretty(&adversarial_canvas()).unwrap();

    let once = Canvas::parse(&source).unwrap().to_json_pretty().unwrap();
    let twice = Canvas::parse(&once).unwrap().to_json_pretty().unwrap();

    assert_eq!(once, twice, "serializing twice must not drift");
}

#[test]
fn document_level_unknown_keys_are_preserved() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    assert_eq!(
        canvas.extra.get("metadata").and_then(|m| m.get("version")),
        Some(&json!("obsidian-1.5.3")),
        "a document-level key the spec does not define must land in extra"
    );
}

#[test]
fn node_level_unknown_keys_are_preserved() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    let node = canvas.node("text-1").unwrap();
    assert_eq!(
        node.extra
            .get("styleAttributes")
            .and_then(|s| s.get("shape")),
        Some(&json!("diamond")),
        "plugin-authored node styling must survive"
    );
}

#[test]
fn edge_level_unknown_keys_are_preserved() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    let edge = canvas.edges.iter().find(|e| e.id == "edge-1").unwrap();
    assert!(
        edge.extra.contains_key("customRouting"),
        "plugin-authored edge data must survive"
    );
}

// ===========================================================================
// Spec surface
// ===========================================================================

#[test]
fn every_node_type_parses_with_its_own_fields() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    assert_eq!(canvas.nodes.len(), 5);

    assert!(matches!(
        canvas.node("text-1").unwrap().kind,
        NodeKind::Text { .. }
    ));
    assert!(matches!(
        canvas.node("link-1").unwrap().kind,
        NodeKind::Link { .. }
    ));

    match &canvas.node("file-1").unwrap().kind {
        NodeKind::File { file, subpath } => {
            assert_eq!(file, "Help/Wikilinks.md");
            assert_eq!(subpath.as_deref(), Some("#Syntax"));
        }
        other => panic!("expected a file node, got {other:?}"),
    }

    match &canvas.node("group-1").unwrap().kind {
        NodeKind::Group {
            label,
            background_style,
            ..
        } => {
            assert_eq!(label.as_deref(), Some("Q3 work"));
            assert_eq!(*background_style, Some(BackgroundStyle::Cover));
        }
        other => panic!("expected a group node, got {other:?}"),
    }
}

#[test]
fn an_empty_canvas_is_legal() {
    let canvas = Canvas::parse("{}").expect("a canvas with no keys is valid per spec");
    assert!(canvas.nodes.is_empty());
    assert!(canvas.edges.is_empty());
}

/// The two ends default differently: an edge with neither key set renders as a
/// one-way arrow, not a plain line. Getting this backwards silently reverses
/// the meaning of every unadorned edge in a vault.
#[test]
fn edge_end_defaults_are_asymmetric() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    let bare = canvas.edges.iter().find(|e| e.id == "edge-2").unwrap();

    assert_eq!(bare.from_end_or_default(), End::None);
    assert_eq!(bare.to_end_or_default(), End::Arrow);

    let explicit = canvas.edges.iter().find(|e| e.id == "edge-1").unwrap();
    assert_eq!(explicit.from_end_or_default(), End::Arrow);
    assert_eq!(explicit.to_end_or_default(), End::None);
}

/// Preset colours are numeric strings. Emitting `1` instead of `"1"` makes
/// Obsidian stop recognising the colour.
#[test]
fn preset_colors_stay_strings_on_the_wire() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    let text = canvas.node("text-1").unwrap();
    assert_eq!(text.color, Some(Color("3".into())));

    let json = serde_json::to_value(text).unwrap();
    assert_eq!(
        json["color"],
        json!("3"),
        "preset must serialize as a string"
    );
    assert_eq!(
        serde_json::to_value(canvas.node("file-1").unwrap()).unwrap()["color"],
        json!("#FF00AA"),
        "hex passes through untouched"
    );
}

/// Obsidian writes a fresh canvas as `{"nodes":[],"edges":[]}`. Dropping the
/// empty arrays would rewrite every empty canvas on first save.
#[test]
fn empty_arrays_are_preserved_rather_than_collapsed() {
    let source = r#"{"nodes":[],"edges":[]}"#;
    let round_tripped: Value =
        serde_json::from_str(&Canvas::parse(source).unwrap().to_json_pretty().unwrap()).unwrap();
    assert_eq!(round_tripped, json!({ "nodes": [], "edges": [] }));
}

#[test]
fn node_order_is_z_order_and_is_preserved() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    let ids: Vec<&str> = canvas.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["text-1", "file-1", "file-2", "link-1", "group-1"]);
}

// ===========================================================================
// Graph projection
// ===========================================================================

#[test]
fn file_paths_lists_only_file_nodes_in_document_order() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    let paths: Vec<&str> = canvas.file_paths().collect();
    assert_eq!(paths, ["Help/Wikilinks.md", "Meta/Systems.md"]);
}

/// The canvas-specific graph signal: an edge between two file nodes is a
/// labelled note-to-note relation. An edge touching a text node is not.
#[test]
fn note_relations_covers_only_edges_between_two_file_nodes() {
    let canvas = Canvas::parse(&serde_json::to_string(&adversarial_canvas()).unwrap()).unwrap();
    let relations: Vec<_> = canvas
        .note_relations()
        .map(|(from, to, edge)| (from, to, edge.label.as_deref()))
        .collect();

    assert_eq!(
        relations,
        [("Help/Wikilinks.md", "Meta/Systems.md", Some("supersedes"))],
        "edge-2 touches a text node and must not become a note relation"
    );
}

#[test]
fn a_dangling_edge_endpoint_yields_no_relation() {
    let canvas = Canvas::parse(
        &json!({
            "nodes": [{
                "id": "a", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                "file": "A.md"
            }],
            "edges": [{ "id": "e", "fromNode": "a", "toNode": "missing" }]
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(canvas.note_relations().count(), 0);
}

// ===========================================================================
// On-disk fidelity
//
// A canvas usually lives in a git repo or under Obsidian Sync. If saving one
// reformats it, moving a single card becomes a whole-file diff, and Crucible
// and Obsidian then rewrite the file back and forth on every alternating edit.
// ===========================================================================

/// The sample from the spec repository (MIT, obsidianmd/jsoncanvas), used as
/// ground truth for what Obsidian actually writes.
const OBSIDIAN_SAMPLE: &str = include_str!("../../tests/fixtures/canvas/sample.canvas");

#[test]
fn saving_an_untouched_obsidian_canvas_produces_no_diff() {
    let canvas = Canvas::parse(OBSIDIAN_SAMPLE).expect("the spec's own sample must parse");
    let written = canvas.to_json_pretty().unwrap();

    assert_eq!(
        written, OBSIDIAN_SAMPLE,
        "round-tripping a canvas Obsidian wrote must be byte-identical"
    );
}

#[test]
fn the_written_form_uses_tabs_and_one_object_per_line() {
    let canvas = Canvas::parse(OBSIDIAN_SAMPLE).unwrap();
    let written = canvas.to_json_pretty().unwrap();

    assert!(written.contains("\n\t\"nodes\":[\n"), "{written}");
    assert!(
        written.lines().filter(|l| l.starts_with("\t\t{")).count() >= 5,
        "each node belongs on its own line: {written}"
    );
    assert!(
        !written.contains("\n    "),
        "space indentation would rewrite every line of a real vault's canvas"
    );
}

/// The fidelity must survive an edit, not just an untouched load — otherwise
/// the first real change reformats everything anyway.
#[test]
fn an_edited_canvas_keeps_the_canonical_shape() {
    let mut canvas = Canvas::parse(OBSIDIAN_SAMPLE).unwrap();
    canvas.nodes[0].x += 40;

    let written = canvas.to_json_pretty().unwrap();
    let changed: Vec<&str> = written
        .lines()
        .zip(OBSIDIAN_SAMPLE.lines())
        .filter(|(a, b)| a != b)
        .map(|(a, _)| a)
        .collect();

    assert_eq!(
        changed.len(),
        1,
        "moving one card must change exactly one line, not the whole file: {changed:?}"
    );
}

/// The shape a real vault canvas takes when it has cards but no connections.
/// Obsidian writes the empty array compactly; expanding it would change two
/// lines of every such file on its first save.
#[test]
fn an_empty_edges_array_is_written_compactly() {
    const NO_EDGES: &str = concat!(
        "{\n",
        "\t\"nodes\":[\n",
        "\t\t{\"id\":\"a\",\"type\":\"file\",\"file\":\"Pasted image 1.png\",\"x\":-1992,\"y\":160,\"width\":952,\"height\":529}\n",
        "\t],\n",
        "\t\"edges\":[]\n",
        "}"
    );

    let canvas = Canvas::parse(NO_EDGES).unwrap();
    assert_eq!(canvas.to_json_pretty().unwrap(), NO_EDGES);
}

/// A canvas whose referenced files are missing — images left behind when the
/// canvas was copied — must still load. It renders as broken cards, not a
/// refused document.
#[test]
fn references_to_missing_files_still_parse() {
    let canvas = Canvas::parse(
        &json!({
            "nodes": [{
                "id": "a", "type": "file", "file": "Pasted image 20241101155136.png",
                "x": -1992, "y": 160, "width": 952, "height": 529
            }],
            "edges": []
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(canvas.file_paths().count(), 1);
}
