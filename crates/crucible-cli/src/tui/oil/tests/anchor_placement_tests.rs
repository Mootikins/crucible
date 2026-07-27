//! Every anchor must actually place a bar.
//!
//! The regression this guards: anchors parsed, validated, crossed the wire and
//! were stored, but the renderer only ever drew the bar named `main`. A bar at
//! any other anchor was silently dropped, and no test noticed because every
//! test drove `main` directly.

use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::test_harness::AppHarness;
use crucible_lua::statusline_items::{Anchor, StatusBarDef, StatusBars, StatusItem, DEFAULT_ORDER};

fn bar_saying(text: &str, anchor: Anchor, order: i32) -> StatusBarDef {
    StatusBarDef {
        anchor,
        order,
        items: vec![StatusItem::Text(text.to_string())],
    }
}

#[test]
fn every_anchor_places_its_bar() {
    let mut bars = StatusBars::new();
    for (name, anchor) in [
        ("t", Anchor::Top),
        ("b", Anchor::Bottom),
        ("above", Anchor::FooterAboveInput),
        ("below", Anchor::FooterBelowInput),
    ] {
        bars.insert(
            name.to_string(),
            bar_saying(&format!("<{name}>"), anchor, DEFAULT_ORDER),
        );
    }
    crate::tui::oil::theme::bars::set(bars);

    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();
    let frame = harness.viewport().to_string();

    for marker in ["<t>", "<b>", "<above>", "<below>"] {
        assert!(
            frame.contains(marker),
            "{marker} never rendered — that anchor places nothing.\n{frame}"
        );
    }
}

#[test]
fn two_bars_in_one_slot_stack_in_order() {
    let mut bars = StatusBars::new();
    bars.insert(
        "second".to_string(),
        bar_saying("<SECOND>", Anchor::FooterBelowInput, 20),
    );
    bars.insert(
        "first".to_string(),
        bar_saying("<FIRST>", Anchor::FooterBelowInput, 10),
    );
    crate::tui::oil::theme::bars::set(bars);

    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();
    let frame = harness.viewport().to_string();

    let first = frame.find("<FIRST>").expect("first bar rendered");
    let second = frame.find("<SECOND>").expect("second bar rendered");
    assert!(
        first < second,
        "order 10 must render above order 20, not alphabetically\n{frame}"
    );
}
