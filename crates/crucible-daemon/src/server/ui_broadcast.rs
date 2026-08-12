//! Broadcasting style changes to attached clients.
//!
//! `ui.config` is the snapshot half of a handshake; this is the stream half.
//! Without it a client could only be styled once, at attach, so switching a
//! theme or editing `init.lua` would require restarting every TUI.
//!
//! The shape follows Neovim's: a client gets the full state when it attaches
//! (`ui_send_all_hls`), then pushes when the config changes
//! (`highlight_changed` → `hl_attr_define`). Remote Neovim UIs get live
//! colorscheme changes for free and subscribe to nothing extra; the same should
//! be true here.
//!
//! Writing a value and telling a client to repaint are separate events. Over a
//! socket, with the TUI idle-blocked on input, a changed store repaints nothing
//! by itself — every comparable statusline implementation needs an explicit
//! redraw signal for exactly this reason.

use crucible_core::protocol::SessionEventMessage;
use tokio::sync::broadcast;
use tracing::debug;

use crate::agent_manager::AgentManager;
use crate::event_emitter::emit_event;

/// The event name clients match on.
pub const UI_STYLE_CHANGED: &str = "ui_style_changed";

/// Session id for changes that are not session-specific.
///
/// Events reach only clients subscribed to their `session_id` (or to the
/// wildcard), so a theme, geometry or bar change — which is global — has to go
/// out on the wildcard. Addressing it to a single session, or to a placeholder
/// like "system", silently delivers it to nobody: every attached TUI subscribes
/// to *its own* session id.
pub const GLOBAL: &str = crate::subscription::WILDCARD_SESSION;

/// Build and broadcast the current style payload.
///
/// Pass [`GLOBAL`] for config-level changes and a real session id only for
/// changes scoped to that session (a pushed statusline value).
///
/// Carries the same shape as the `ui.config` result, so a client applies it with
/// the same code path it uses at attach — one parser, not two that can drift.
///
/// Fire-and-forget: a send with no subscribers is not an error. A daemon with no
/// attached TUI should not log noise, and the next client to attach gets the
/// state from the snapshot anyway.
/// Tell a session's clients that an expression value changed.
///
/// Addressed to the real session rather than [`GLOBAL`]: a value belongs to one
/// session, unlike a theme or layout change.
pub fn broadcast_exprs_changed(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    agents: &AgentManager,
    session_id: &str,
) {
    let payload = crate::rpc::ui::expr_payload(agents, session_id);
    let msg = SessionEventMessage::new(session_id, UI_STYLE_CHANGED, payload);
    if !emit_event(event_tx, msg) {
        debug!("statusline value change had no subscribers");
    }
}

pub fn broadcast_style_changed(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    agents: &AgentManager,
    session_id: &str,
) {
    let payload = crate::rpc::ui::style_payload(agents, session_id);
    let msg = SessionEventMessage::new(session_id, UI_STYLE_CHANGED, payload);
    if !emit_event(event_tx, msg) {
        debug!("ui style change had no subscribers");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::{SubscriptionManager, WILDCARD_SESSION};

    /// The bug this constant exists to prevent, pinned.
    ///
    /// Delivery filters on the event's `session_id`, and every attached TUI
    /// subscribes to *its own* session. A global change addressed to a
    /// placeholder id therefore reaches nobody — which is exactly what happened
    /// before, silently, with the RPC reporting success.
    #[test]
    fn global_is_the_wildcard_not_a_placeholder_id() {
        assert_eq!(GLOBAL, WILDCARD_SESSION);
        assert_ne!(GLOBAL, "system");
    }

    /// A client subscribed only to its own session must still be reachable by a
    /// wildcard-addressed event. `is_subscribed` alone does not give this — it
    /// answers the other direction — so the delivery path special-cases it.
    #[test]
    fn a_session_subscriber_is_not_matched_by_is_subscribed_for_a_global_event() {
        let manager = SubscriptionManager::new();
        let client = crate::subscription::ClientId::new();
        manager.subscribe(client, "my-tui-session");

        assert!(manager.is_subscribed(client, "my-tui-session"));
        assert!(
            !manager.is_subscribed(client, GLOBAL),
            "if this ever becomes true, the wildcard special-case in \
             server::core delivery is redundant and should be removed"
        );
    }
}
