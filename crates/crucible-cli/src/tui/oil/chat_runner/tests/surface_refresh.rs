//! US-908: what a background surface refetch tells the app.
//!
//! A `surface_changed` event starts a refetch. The refetch has three outcomes,
//! and two of them are empty. This suite holds the two empty ones apart:
//!
//! - The daemon answers that the surface is absent. A plugin uninstall does
//!   this, so the app must stop drawing the panel.
//! - The refetch fails. The daemon may be busy or briefly unreachable, so the
//!   surface may still exist and the panel must stay.
//!
//! The whole defect this covers is the two cases sharing one silent path.

use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::actions::refresh_outcome;

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
