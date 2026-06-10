//! Contract tests for the `serde` feature: Oil nodes → JSON for browser rendering.
//!
//! The JSON shape is a public contract consumed by the web UI's node renderer:
//! externally-tagged enums with snake_case discriminants, default-valued fields
//! omitted (missing key ⇒ default). Snapshots here are the contract spec — do not
//! accept changes without verifying the consumer-facing shape is still correct.
#![cfg(feature = "serde")]

use crucible_oil::node::{
    col, overlay_from_bottom, popup, popup_item, raw, row, slot, spinner, styled, text, text_input,
    Node, SpinnerStyle,
};
use crucible_oil::style::{Border, Color, Gap, JustifyContent, Padding, Style};

fn to_json(node: &Node) -> String {
    serde_json::to_string_pretty(node).expect("node serializes")
}

#[test]
fn text_with_styles_serializes_to_tagged_json() {
    let node = styled(
        "Error: kiln not found",
        Style::new().fg(Color::Red).bg(Color::Black).bold().dim(),
    );
    insta::assert_snapshot!(to_json(&node));
}

#[test]
fn col_row_with_gap_serializes_directions_and_gap() {
    let node = col([
        text("header"),
        row([text("left"), text("right")]).gap(Gap::column(2)),
    ])
    .gap(Gap::row(1))
    .with_padding(Padding::xy(2, 1))
    .with_border(Border::Rounded);
    insta::assert_snapshot!(to_json(&node));
}

#[test]
fn nested_containers_full_tree_snapshot() {
    // Representative chat-frame tree: sized boxes, styled text, spinner, input.
    let node = col([
        crucible_oil::node::fixed(1, styled("Crucible", Style::new().bold())),
        crucible_oil::node::flex(
            1,
            col([
                text("How can I help?"),
                row([
                    crucible_oil::node::badge("tool", Style::new().bg(Color::Rgb(30, 34, 42))),
                    text("read_file"),
                ]),
            ])
            .gap(Gap::row(1)),
        ),
        Node::Spinner(
            crucible_oil::node::SpinnerNode::new(3)
                .label("thinking")
                .style_variant(SpinnerStyle::Braille),
        ),
        text_input("dra", 3),
    ])
    .justify(JustifyContent::SpaceBetween);
    insta::assert_snapshot!(to_json(&node));
}

#[test]
fn slots_and_fragments_serialize_transparently() {
    let node = col([
        slot("content", [text("body"), Node::Empty]),
        crucible_oil::node::fragment([text("a"), text("b")]),
        slot("footer", []),
    ]);
    insta::assert_snapshot!(to_json(&node));
}

#[test]
fn popup_and_overlay_serialize() {
    let items = vec![
        popup_item("Open kiln").desc("kiln.open").kind("command"),
        popup_item("Search"),
    ];
    let node = overlay_from_bottom(popup(items, 1, 5), 2);
    insta::assert_snapshot!(to_json(&node));
}

#[test]
fn default_fields_are_omitted() {
    // Lean-payload contract: missing key ⇒ default. A bare text node carries
    // only its content; a default box is an empty object.
    let json = serde_json::to_string(&text("x")).unwrap();
    assert_eq!(json, r#"{"text":{"content":"x"}}"#);

    let json = serde_json::to_string(&col([])).unwrap();
    assert_eq!(json, r#"{"box":{}}"#);

    let json = serde_json::to_string(&Node::Empty).unwrap();
    assert_eq!(json, r#""empty""#);
}

#[test]
fn color_variants_serialize() {
    let named = serde_json::to_string(&Color::DarkGray).unwrap();
    assert_eq!(named, r#""dark_gray""#);

    let rgb = serde_json::to_string(&Color::Rgb(30, 34, 42)).unwrap();
    assert_eq!(rgb, r#"{"rgb":[30,34,42]}"#);
}

#[test]
fn raw_node_serializes_dimensions() {
    let node = raw("\x1b_Gi=1;abc\x1b\\", 10, 5);
    let json = serde_json::to_string(&node).unwrap();
    // serde_json escapes ESC (0x1b) as \u001b.
    assert_eq!(
        json,
        r#"{"raw":{"content":"\u001b_Gi=1;abc\u001b\\","display_width":10,"display_height":5}}"#
    );
}

#[test]
fn spinner_defaults_omitted_but_frame_kept_when_nonzero() {
    let node = spinner(None, 0);
    let json = serde_json::to_string(&node).unwrap();
    assert_eq!(json, r#"{"spinner":{}}"#);
}
