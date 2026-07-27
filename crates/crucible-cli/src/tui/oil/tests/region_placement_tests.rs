//! Every region must actually place what the author put in it.
//!
//! The regression this guards: placement config round-tripped through Lua, the
//! wire and the store, and then the renderer drew only one hardcoded bar. No
//! test noticed, because every test drove that one bar directly. A knob that
//! round-trips is not a shipped feature — assert on the rendered frame.

use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::test_harness::AppHarness;
use crucible_lua::statusline_items::{Element, Layout, StatusItem};

fn row(text: &str) -> Element {
    Element::Row(vec![StatusItem::Text(text.to_string())])
}

fn frame_with(layout: Layout) -> String {
    crate::tui::oil::theme::bars::set(layout);
    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();
    harness.viewport().to_string()
}

#[test]
fn every_region_places_its_rows() {
    let frame = frame_with(Layout {
        top: vec![row("<TOP>")],
        prompt: vec![row("<ABOVE>"), Element::Input, row("<BELOW>")],
        bottom: vec![row("<BOTTOM>")],
    });

    for marker in ["<TOP>", "<ABOVE>", "<BELOW>", "<BOTTOM>"] {
        assert!(
            frame.contains(marker),
            "{marker} never rendered — that region places nothing.\n{frame}"
        );
    }
}

/// Position in the list is the arrangement, with nothing else to consult.
#[test]
fn rows_render_in_the_order_they_were_written() {
    let frame = frame_with(Layout {
        prompt: vec![Element::Input, row("<FIRST>"), row("<SECOND>")],
        ..Layout::default()
    });

    let first = frame.find("<FIRST>").expect("first row rendered");
    let second = frame.find("<SECOND>").expect("second row rendered");
    assert!(first < second, "rows must render in list order\n{frame}");
}

/// The input is an element, so writing rows around it places them around it —
/// which is what anchors needed a second concept to express.
#[test]
fn the_input_separates_the_rows_written_around_it() {
    let frame = frame_with(Layout {
        prompt: vec![row("<ABOVE>"), Element::Input, row("<BELOW>")],
        ..Layout::default()
    });

    let above = frame.find("<ABOVE>").expect("above row rendered");
    let below = frame.find("<BELOW>").expect("below row rendered");
    assert!(
        above < below,
        "the input did not separate the rows\n{frame}"
    );
}
