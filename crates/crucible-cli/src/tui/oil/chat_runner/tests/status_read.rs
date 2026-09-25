//! The first status list of a TUI session.
//!
//! The runner reads `session.status` after the session's events flow, so
//! no change falls between the read and the subscription. A read that fails,
//! or an item that this client cannot decode, shows a notice (rule 7): a
//! status list that is shorter without a word would hide an `ask` or a
//! `stop` from the user.

use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;
use crucible_daemon::rpc_client::decode_status_items;

fn item(color_group: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "plugin_turns:goal", "text": "goal · ask", "priority": 0,
        "color_group": color_group, "action": "plugin_approval", "pinned": true,
        "plugin": "goal", "kind": "plugin_turns", "progress": null,
    })
}

#[test]
fn a_read_status_list_reaches_the_app() {
    let reply = serde_json::json!({ "status": [item("warn")] });
    let msg = OilChatRunner::status_items_msg(decode_status_items(reply));
    let ChatAppMsg::StatusItemsLoaded(items) = msg else {
        panic!("expected the list, got {msg:?}");
    };
    assert_eq!(items[0].text, "goal · ask");
}

#[test]
fn an_item_that_does_not_decode_shows_a_notice() {
    let reply = serde_json::json!({ "status": [item("chartreuse")] });
    let msg = OilChatRunner::status_items_msg(decode_status_items(reply));
    let ChatAppMsg::Error(ref text) = msg else {
        panic!("expected a notice, got {msg:?}");
    };
    assert!(text.contains("status"), "{text}");
    let mut app = OilChatApp::default();
    app.on_message(msg);
    let screen = crate::tui::oil::tests::helpers::vt_render(&mut app);
    assert!(screen.contains("status"), "the notice shows:\n{screen}");
}

#[test]
fn a_reply_without_a_list_shows_a_notice() {
    let msg = OilChatRunner::status_items_msg(decode_status_items(serde_json::json!({})));
    assert!(matches!(msg, ChatAppMsg::Error(_)), "{msg:?}");
}
