//! US-908: what a `surface_changed` event tells the app.
//!
//! Two halves, and the first decides whether the second runs at all.
//!
//! **Off the event.** A withdrawal is marked `withdrawn`, so the app acts on it
//! without asking. Every other change withholds the rows on purpose, so it
//! starts a refetch. Nothing may read an ordinary change as a withdrawal: that
//! would close the panel it was told to refresh.
//!
//! **Off the refetch.** It has three outcomes, and two of them are empty. This
//! suite holds the two empty ones apart:
//!
//! - The daemon answers that the surface is absent — a removal that raced the
//!   change event, or a withdrawal this client never received. The app must
//!   stop drawing the panel.
//! - The refetch fails. The daemon may be busy or briefly unreachable, so the
//!   surface may still exist and the panel must stay.
//!
//! The whole defect this covers is the two cases sharing one silent path.

use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::actions::refresh_outcome;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

/// One `surface_changed` event off the wire.
fn event(extra: serde_json::Value) -> Vec<ChatAppMsg> {
    let mut data = serde_json::json!({ "plugin": "p", "name": "sessions", "version": 2 });
    let (Some(obj), Some(more)) = (data.as_object_mut(), extra.as_object()) else {
        panic!("both must be objects");
    };
    for (k, v) in more {
        obj.insert(k.clone(), v.clone());
    }
    session_event_to_chat_msgs(
        crucible_core::protocol::SystemPayload::SURFACE_CHANGED,
        &data,
    )
}

/// A withdrawal is acted on straight off the event, with no refetch.
///
/// The daemon knows the surface is gone at the moment it drops the entry. The
/// TUI used to re-derive that by asking for the surface and reading the empty
/// answer — a round trip per client to learn what the event could have said.
#[test]
fn a_withdrawn_event_needs_no_refetch() {
    match event(serde_json::json!({ "withdrawn": true })).as_slice() {
        [ChatAppMsg::SurfaceWithdrawn(name)] => assert_eq!(name, "sessions"),
        other => panic!("expected one withdrawal and no refetch, got {other:?}"),
    }
}

/// **The negative.** An ordinary change still refetches.
///
/// The event withholds the rows on purpose, so this is the one path that must
/// keep asking. A client that read every change as a withdrawal would close the
/// panel it was told to refresh.
#[test]
fn an_ordinary_change_still_refetches() {
    match event(serde_json::json!({ "withdrawn": false })).as_slice() {
        [ChatAppMsg::RefreshSurface(name)] => assert_eq!(name, "sessions"),
        other => panic!("expected a refetch, got {other:?}"),
    }
}

/// An event with no `withdrawn` field refetches.
///
/// The daemon omits the field when it is false, so absent must mean present.
/// Reading it the other way would erase every panel on an ordinary change.
#[test]
fn an_event_without_the_field_refetches() {
    match event(serde_json::json!({})).as_slice() {
        [ChatAppMsg::RefreshSurface(name)] => assert_eq!(name, "sessions"),
        other => panic!("expected a refetch, got {other:?}"),
    }
}

fn loaded() -> ChatAppMsg {
    ChatAppMsg::SurfaceLoaded {
        name: "sessions".into(),
        title: "Sessions".into(),
        rows: vec![],
        version: 2,
        open_if_closed: false,
    }
}

/// A surface that answers rows refreshes the panel, and never opens one.
#[test]
fn rows_refresh_the_panel() {
    match refresh_outcome("sessions", Ok(Some(loaded()))) {
        Some(ChatAppMsg::SurfaceLoaded {
            name,
            open_if_closed,
            ..
        }) => {
            assert_eq!(name, "sessions");
            assert!(!open_if_closed, "a refresh must never open a panel");
        }
        other => panic!("expected the loaded surface, got {other:?}"),
    }
}

/// The daemon answered, and the answer is that the surface is gone. That is a
/// withdrawal, and the app must hear about it.
#[test]
fn an_absent_surface_reports_a_withdrawal() {
    match refresh_outcome("sessions", Ok(None)) {
        Some(ChatAppMsg::SurfaceWithdrawn(name)) => assert_eq!(name, "sessions"),
        other => panic!("expected a withdrawal, got {other:?}"),
    }
}

/// **A failed refetch is not a withdrawal.** A daemon that is briefly
/// unreachable must not close a panel the user reads. This is the assertion
/// that keeps the two empty answers apart.
#[test]
fn a_failed_refetch_reports_nothing() {
    let outcome = refresh_outcome("sessions", Err(anyhow::anyhow!("daemon unreachable")));
    assert!(
        outcome.is_none(),
        "a failed refresh must say nothing, got {outcome:?}"
    );
}
