//! Push notification for worktree writes no capture bracket owned.
//!
//! Split from [`super`] for the 1000-line module budget: the daemon's run
//! loop starts and cancels this task, but nothing about it belongs to the
//! server's wiring.

use std::time::Duration;

use crate::protocol::SessionEventMessage;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use super::emit_event;

/// Turn observed external changes into one `review_changed` per session.
///
/// Coalesced, because
/// [`ExternalChangeTracker::observe`](crate::watch::external_changes::ExternalChangeTracker::observe)
/// broadcasts per path:
/// one `cargo fmt` or one branch switch is hundreds of events over a handful
/// of sessions, and each one would otherwise cost every connected client a
/// full recompose of the composed diff. The reason is `"external"` — the
/// clients' cue that the worktree moved under them rather than that a decision
/// was recorded.
pub(super) async fn announce_external_changes(
    mut rx: broadcast::Receiver<crate::watch::external_changes::ExternalChange>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    cancel: CancellationToken,
) {
    /// Long enough to fold an editor's save burst and a formatter's rewrite
    /// into one notification, short enough that a hand edit still feels live.
    const COALESCE: Duration = Duration::from_millis(250);

    let mut pending: std::collections::HashSet<String> = std::collections::HashSet::new();
    // `None` means nothing is waiting, so the flush arm parks forever rather
    // than spinning a timer for an idle daemon.
    // Tokio's clock, not `std`'s, so the window is virtual under a paused
    // runtime instead of a quarter second of wall time per test.
    let mut deadline: Option<tokio::time::Instant> = None;
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                for session_id in pending.drain() {
                    emit_event(
                        &event_tx,
                        SessionEventMessage::review_changed(&session_id, "external"),
                    );
                }
                deadline = None;
            }
            received = rx.recv() => match received {
                Ok(change) => {
                    pending.extend(change.sessions);
                    deadline.get_or_insert_with(|| tokio::time::Instant::now() + COALESCE);
                }
                // A lagging receiver dropped notifications, not changes: the
                // next write re-announces, and a client that reloads sees the
                // same worktree either way.
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(dropped = n, "review watch notifier lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
}
