//! Session context strategy forwarded with explicit replay policy.

use super::daemon::ReconnectingDaemon;

impl ReconnectingDaemon {
    forward_rpc! {
        Once SessionSetContextStrategy =>
        session_set_context_strategy(session_id: &str, strategy: &str)
        -> () = session_set_context_strategy(&session_id, &strategy);
    }

    forward_rpc! {
        Safe SessionGetContextStrategy =>
        session_get_context_strategy(session_id: &str)
        -> Option<String> = session_get_context_strategy(&session_id);
    }
}
