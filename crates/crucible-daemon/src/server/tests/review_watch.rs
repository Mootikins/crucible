//! The push half of the review backstop: observed worktree writes becoming
//! `review_changed` events.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::protocol::SessionEventMessage;
use crate::server::external_announce::announce_external_changes;
use crate::watch::external_changes::ExternalChangeTracker;

/// Every `review_changed` the notifier emitted before it went quiet.
async fn drain(rx: &mut broadcast::Receiver<SessionEventMessage>) -> Vec<(String, String)> {
    let mut seen = Vec::new();
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
        if event.event == "review_changed" {
            let reason = event
                .data
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or_default()
                .to_string();
            seen.push((event.session_id, reason));
        }
        // The window has already closed by the time the first event lands, so
        // anything else the notifier meant to send is queued behind it.
        if rx.is_empty() {
            break;
        }
    }
    seen
}

/// `observe` broadcasts per path, and a formatter run or a branch switch is
/// hundreds of paths. Uncoalesced, each one costs every connected client a
/// full recompose of the composed diff.
#[tokio::test]
async fn a_burst_of_external_writes_announces_each_session_once() {
    let root = PathBuf::from("/repo");
    let tracker = Arc::new(ExternalChangeTracker::default());
    tracker.track("s1", std::slice::from_ref(&root));
    tracker.track("s2", std::slice::from_ref(&root));

    let (event_tx, mut event_rx) = broadcast::channel(64);
    let cancel = CancellationToken::new();
    let task = tokio::spawn(announce_external_changes(
        tracker.subscribe(),
        event_tx,
        cancel.clone(),
    ));

    for path in ["/repo/a.rs", "/repo/b.rs", "/repo/a.rs"] {
        tracker.observe(Path::new(path));
    }

    let mut announced = drain(&mut event_rx).await;
    announced.sort();
    assert_eq!(
        announced,
        vec![
            ("s1".to_string(), "external".to_string()),
            ("s2".to_string(), "external".to_string()),
        ],
        "three writes over two sessions should announce twice, once each"
    );

    cancel.cancel();
    task.await.unwrap();
}

/// A bracketed write is the ledger's to attribute. Announcing it would tell
/// every client the worktree moved under them for a change the agent is in
/// the middle of making.
#[tokio::test(start_paused = true)]
async fn a_bracketed_write_announces_nothing() {
    let root = PathBuf::from("/repo");
    let tracker = Arc::new(ExternalChangeTracker::default());
    tracker.track("s1", std::slice::from_ref(&root));

    let (event_tx, mut event_rx) = broadcast::channel(64);
    let cancel = CancellationToken::new();
    let task = tokio::spawn(announce_external_changes(
        tracker.subscribe(),
        event_tx,
        cancel.clone(),
    ));

    let _window = tracker.capture("s1");
    tracker.observe(Path::new("/repo/a.rs"));

    // Paused time: the coalescing window elapses twice over without spending
    // 600ms of anyone's wall clock, and without assuming it will.
    tokio::time::advance(Duration::from_millis(600)).await;
    tokio::task::yield_now().await;
    assert!(
        event_rx.try_recv().is_err(),
        "a write the ledger owns was announced as an external change"
    );

    cancel.cancel();
    task.await.unwrap();
}
