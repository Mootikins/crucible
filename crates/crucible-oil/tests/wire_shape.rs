//! The serialized node tree is a wire contract now, not a debug aid.
//!
//! `plugin.view_render` hands this JSON to the web, which draws from it. A
//! field that changes name, or an enum whose casing shifts, breaks a renderer
//! in another language that no Rust test would notice. So the shape is pinned
//! here, in the crate that owns it.
#![cfg(feature = "serde")]

use crucible_oil::{action, col, styled, text, Node, Style};
use std::collections::BTreeMap;

#[test]
fn a_tree_serializes_to_the_shape_the_web_renders() {
    let mut params = BTreeMap::new();
    params.insert("ticket".to_string(), "alpha.md".to_string());
    params.insert("to".to_string(), "doing".to_string());

    let tree = col(vec![
        styled("Doing", Style::new().bold()),
        action("move", params, text("alpha")),
    ]);

    let json = serde_json::to_value(&tree).expect("serializes");
    println!("{}", serde_json::to_string_pretty(&json).unwrap());

    // Externally tagged, snake_case: the discriminant the renderer switches on.
    let bx = json.get("box").expect("a col is a box node");
    let children = bx.get("children").expect("children").as_array().unwrap();
    assert_eq!(children.len(), 2);

    let head = children[0].get("text").expect("first child is text");
    assert_eq!(head.get("content").unwrap(), "Doing");
    assert_eq!(head.get("style").unwrap().get("bold").unwrap(), true);

    let act = children[1]
        .get("action")
        .expect("second child is an action");
    assert_eq!(act.get("action").unwrap(), "move");
    assert_eq!(
        act.get("params").unwrap().get("ticket").unwrap(),
        "alpha.md"
    );
    assert!(act.get("child").unwrap().get("text").is_some());

    // Defaults are omitted, so a payload stays small over the wire.
    assert!(bx.get("justify").is_none(), "default justify is not sent");
    assert!(
        children[0].get("text").unwrap().get("no_shrink").is_none(),
        "default no_shrink is not sent"
    );
}

#[test]
fn empty_is_a_bare_string_not_an_object() {
    // A unit variant of an externally tagged enum. The renderer must accept a
    // string where it otherwise expects an object, so this is worth pinning.
    assert_eq!(
        serde_json::to_value(Node::Empty).unwrap(),
        serde_json::Value::String("empty".to_string())
    );
}
