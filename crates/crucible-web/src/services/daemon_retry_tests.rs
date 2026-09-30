//! A peer applies a request then drops its socket WITHOUT replying.
//! Reconnects use the same isolated listener, never the developer's daemon.
use super::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::time::{timeout, Duration};

struct Peer {
    daemon: Arc<ReconnectingDaemon>,
    applied: Arc<AtomicUsize>,
    subscriptions: Arc<AtomicUsize>,
    unsubscriptions: Arc<AtomicUsize>,
    reject_subscription: Arc<AtomicBool>,
    hold_subscription: Arc<AtomicBool>,
    subscription_started: Arc<tokio::sync::Notify>,
    release_subscription: Arc<tokio::sync::Notify>,
    task: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Peer {
    async fn losing_first_reply(method: &'static str) -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let applied = Arc::new(AtomicUsize::new(0));
        let subscriptions = Arc::new(AtomicUsize::new(0));
        let calls = applied.clone();
        let subs = subscriptions.clone();
        let unsubscriptions = Arc::new(AtomicUsize::new(0));
        let unsubs = unsubscriptions.clone();
        let reject_subscription = Arc::new(AtomicBool::new(false));
        let reject = reject_subscription.clone();
        let hold_subscription = Arc::new(AtomicBool::new(false));
        let hold = hold_subscription.clone();
        let subscription_started = Arc::new(tokio::sync::Notify::new());
        let started = subscription_started.clone();
        let release_subscription = Arc::new(tokio::sync::Notify::new());
        let release = release_subscription.clone();
        let task = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let (read, mut write) = socket.into_split();
                let mut lines = BufReader::new(read).lines();
                let mut active = std::collections::HashSet::<String>::new();
                while let Some(line) = lines.next_line().await.unwrap() {
                    let request: Value = serde_json::from_str(&line).unwrap();
                    if request["method"] == method && calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        // The effect landed. Neither a reply nor an RPC error did.
                        break;
                    }
                    if request["method"] == "session.subscribe" {
                        if reject.swap(false, Ordering::SeqCst) {
                            let reply = json!({"jsonrpc":"2.0", "id":request["id"],
                                "error":{"code":-32602,"message":"subscription refused"}});
                            write
                                .write_all(format!("{reply}\n").as_bytes())
                                .await
                                .unwrap();
                            continue;
                        }
                        for id in request["params"]["session_ids"].as_array().unwrap() {
                            active.insert(id.as_str().unwrap().to_owned());
                        }
                        subs.fetch_add(1, Ordering::SeqCst);
                        if hold.swap(false, Ordering::SeqCst) {
                            started.notify_one();
                            release.notified().await;
                        }
                    }
                    if request["method"] == "session.unsubscribe" {
                        unsubs.fetch_add(1, Ordering::SeqCst);
                        for id in request["params"]["session_ids"].as_array().unwrap() {
                            active.remove(id.as_str().unwrap());
                        }
                    }
                    let result = if request["method"] == "plugin.commands" {
                        json!({ "commands": [] })
                    } else if request["method"] == "session.events_after" {
                        json!([])
                    } else {
                        json!({})
                    };
                    let reply = json!({"jsonrpc":"2.0", "id":request["id"], "result":result});
                    write
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                    if request["method"] == "plugin.commands" {
                        for id in &active {
                            let event = if id == "system" {
                                SessionEvent::new(id, "file_changed", json!({}))
                            } else {
                                // A restarted daemon numbers the session from
                                // its log again, so the seq can be low.
                                let mut event = SessionEvent::new(
                                    id,
                                    "text_delta",
                                    json!({"content": "after reconnect"}),
                                );
                                event.seq = Some(1);
                                event
                            };
                            write
                                .write_all(
                                    format!("{}\n", serde_json::to_string(&event).unwrap())
                                        .as_bytes(),
                                )
                                .await
                                .unwrap();
                        }
                    }
                }
            }
        });
        let (client, rx) = DaemonClient::connect_to_with_events(&path).await.unwrap();
        let mut daemon = ReconnectingDaemon::new(client, rx, Arc::new(EventBroker::new()));
        daemon.reconnect_socket = Some(path);
        Self {
            daemon: Arc::new(daemon),
            unsubscriptions,
            reject_subscription,
            hold_subscription,
            subscription_started,
            release_subscription,
            applied,
            subscriptions,
            task,
            _dir: dir,
        }
    }
}

#[tokio::test]
async fn mutations_are_not_replayed_when_the_reply_is_lost() {
    for method in [
        "plugin.run_command",
        "plugin.option_execute",
        "session.knob.set",
        "diff.resolve_comment",
    ] {
        let peer = Peer::losing_first_reply(method).await;
        let result = timeout(Duration::from_secs(2), async {
            match method {
                "plugin.run_command" => peer
                    .daemon
                    .plugin_run_command("test", json!({}), None)
                    .await
                    .map(|_| ()),
                "plugin.option_execute" => {
                    peer.daemon
                        .plugin_option_execute("test", vec!["run".into()])
                        .await
                }
                "session.knob.set" => peer
                    .daemon
                    .session_knob_set("s", crucible_core::types::KnobValue::Model("m".into()))
                    .await
                    .map(|_| ()),
                // `diff.resolve_comment` no longer has its own named forwarder
                // (Simplification Plan step 19: the browser reaches it through
                // `rpc_forward`, the same generic path every `POST
                // /api/rpc/{method}` call takes). `rpc_forward` is `Once`
                // unconditionally, so this proves the browser's own path
                // never replays an ambiguous write, not just the named
                // forwarders that still exist.
                "diff.resolve_comment" => peer
                    .daemon
                    .rpc_forward(
                        RpcMethod::DiffResolveComment,
                        json!({
                            "source": { "kind": "session_record", "session": "s" },
                            "comment_id": "c",
                        }),
                    )
                    .await
                    .map(|_| ()),
                _ => unreachable!(),
            }
        })
        .await
        .expect("a disconnected request must finish promptly");
        assert!(
            result.is_err(),
            "{method}: an ambiguous outcome must reach the caller"
        );
        assert_eq!(
            peer.applied.load(Ordering::SeqCst),
            1,
            "{method}: apply once"
        );
        // Recovery is still possible on the next read, without resending the write.
        peer.daemon.plugin_commands().await.unwrap();
        assert_eq!(peer.applied.load(Ordering::SeqCst), 1);
    }
}

/// `POST /api/rpc/{method}` (`rpc_forward`, `ReplayPolicy::Once`) carries an
/// arbitrary method chosen at the HTTP layer, so a lost reply still reaches
/// its caller as an error — the same ambiguous-write guard the test above
/// proves for the dedicated `Once` forwarders. But the connection itself
/// still heals: reconnecting performs no daemon-side call of its own, so it
/// carries none of the double-execution risk a resubmitted write does. A
/// poll like `kiln-restart.live.spec.ts`'s (stop the daemon, then retry the
/// same generic call until it succeeds) needs exactly this — before this
/// test's own fix, nothing ever reconnected the shared handle unless some
/// OTHER, `Safe`-policy call happened to be in flight at the same time.
#[tokio::test]
async fn a_lost_reply_through_the_generic_forwarder_still_heals_the_connection() {
    let peer = Peer::losing_first_reply("kiln.list").await;

    let first = timeout(
        Duration::from_secs(2),
        peer.daemon
            .rpc_forward(crucible_core::protocol::RpcMethod::KilnList, Value::Null),
    )
    .await
    .expect("a disconnected request must finish promptly");
    assert!(first.is_err(), "an ambiguous outcome must reach the caller");
    assert_eq!(
        peer.applied.load(Ordering::SeqCst),
        1,
        "the daemon saw one call"
    );

    // The SAME generic call, not a different `Safe` one: the failed `Once`
    // call above must have reconnected the handle by itself.
    let second = timeout(
        Duration::from_secs(2),
        peer.daemon
            .rpc_forward(crucible_core::protocol::RpcMethod::KilnList, Value::Null),
    )
    .await
    .expect("the retry must finish promptly")
    .expect("the healed connection answers the retry");
    assert_eq!(second, json!({}));
    assert_eq!(
        peer.applied.load(Ordering::SeqCst),
        2,
        "the write was never resubmitted; this is the caller's own retry"
    );
}

#[tokio::test]
async fn replay_safe_reads_reconnect_once_and_restore_active_events() {
    let peer = Peer::losing_first_reply("plugin.commands").await;
    use futures::StreamExt;
    let mut events = peer.daemon.subscribe_events("system").await.unwrap();
    let result = timeout(Duration::from_secs(2), peer.daemon.plugin_commands())
        .await
        .expect("read should reconnect promptly")
        .unwrap();
    assert!(result.is_empty());
    assert_eq!(peer.applied.load(Ordering::SeqCst), 2);
    assert_eq!(peer.subscriptions.load(Ordering::SeqCst), 2);
    let event = timeout(Duration::from_secs(2), async {
        loop {
            let event = events.next().await.unwrap();
            if event.event != "stream_gap" {
                break Some(event);
            }
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(event.event, "file_changed");
}

async fn browser_stream(peer: &Peer, session: &str) -> axum::body::Body {
    browser_stream_at(peer, &format!("/api/events?topics={session}")).await
}

async fn browser_stream_at(peer: &Peer, uri: &str) -> axum::body::Body {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let state = AppState {
        daemon: peer.daemon.clone(),
        events: peer.daemon.broker.clone(),
        config: Arc::new(CliAppConfig::default()),
        http_client: reqwest::Client::new(),
        client_state_id: Arc::from(crate::services::daemon::WEB_CLIENT_STATE_ID),
        remote_shell: false,
        recents_lock: Arc::new(tokio::sync::Mutex::new(())),
    };
    let response = crate::test_support::build_test_app(state)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let mut body = response.into_body();
    browser_frame(&mut body, "stream_version").await;
    body
}

async fn browser_frame(body: &mut axum::body::Body, marker: &str) -> String {
    use http_body_util::BodyExt;
    timeout(Duration::from_secs(2), async {
        let mut text = String::new();
        loop {
            let frame = body.frame().await.expect("SSE remains open").unwrap();
            if let Some(data) = frame.data_ref() {
                text.push_str(&String::from_utf8_lossy(data));
                if text.contains(marker) {
                    return text;
                }
            }
        }
    })
    .await
    .expect("browser receives the next event without reopening")
}

#[tokio::test]
async fn reconnect_restores_open_browser_streams_and_last_drop_reclaims_interest() {
    let peer = Peer::losing_first_reply("plugin.commands").await;
    let mut first = browser_stream(&peer, "chat").await;
    let mut second = browser_stream(&peer, "chat").await;
    assert_eq!(
        peer.subscriptions.load(Ordering::SeqCst),
        1,
        "shared upstream interest"
    );

    peer.daemon.plugin_commands().await.unwrap();
    for body in [&mut first, &mut second] {
        let text = browser_frame(body, "after reconnect").await;
        assert!(
            text.contains("stream_gap"),
            "reconnect announces the unknown lost span"
        );
    }
    assert_eq!(peer.subscriptions.load(Ordering::SeqCst), 2);

    drop(first);
    peer.daemon.plugin_commands().await.unwrap();
    browser_frame(&mut second, "after reconnect").await;
    drop(second);
    timeout(Duration::from_secs(2), async {
        loop {
            if peer.daemon.broker.sessions.read().await.is_empty()
                && peer.unsubscriptions.load(Ordering::SeqCst) == 1
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("last browser closes both local and upstream interest");

    // A queued cleanup must not unsubscribe a replacement reader.
    let old = browser_stream(&peer, "chat").await;
    drop(old);
    let mut replacement = browser_stream(&peer, "chat").await;
    peer.daemon.plugin_commands().await.unwrap();
    browser_frame(&mut replacement, "after reconnect").await;
}

#[tokio::test]
async fn refused_subscription_releases_its_local_receiver() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let (_mock, client) = crate::test_support::start_mock_daemon_with_errors(
        [(
            crucible_core::protocol::rpc::RpcMethod::SessionSubscribe,
            (-32602, "refused".into()),
        )]
        .into(),
    )
    .await;
    let state = crate::test_support::build_state(client);
    let broker = state.events.clone();
    let response = crate::test_support::build_test_app(state)
        .oneshot(
            Request::builder()
                .uri("/api/events?topics=chat")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    timeout(Duration::from_secs(2), async {
        while !broker.sessions.read().await.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("failed subscription leaves no broker entry");
}

#[tokio::test]
async fn cancelled_subscription_releases_interest_after_the_peer_applies_it() {
    let peer = Peer::losing_first_reply("never").await;
    peer.hold_subscription.store(true, Ordering::SeqCst);
    let daemon = peer.daemon.clone();
    let subscribing = tokio::spawn(async move { daemon.subscribe_events("chat").await });
    timeout(Duration::from_secs(2), peer.subscription_started.notified())
        .await
        .unwrap();
    subscribing.abort();
    assert!(matches!(subscribing.await, Err(error) if error.is_cancelled()));
    peer.release_subscription.notify_one();
    timeout(Duration::from_secs(2), async {
        loop {
            if peer.daemon.broker.sessions.read().await.is_empty()
                && peer.unsubscriptions.load(Ordering::SeqCst) == 1
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancellation releases local and already applied upstream interest");
}

#[tokio::test]
async fn failed_restoration_does_not_mark_a_half_restored_connection_healthy() {
    let peer = Peer::losing_first_reply("plugin.commands").await;
    let mut body = browser_stream(&peer, "chat").await;
    peer.reject_subscription.store(true, Ordering::SeqCst);
    assert!(peer.daemon.plugin_commands().await.is_err());
    assert_eq!(peer.daemon.generation.load(Ordering::SeqCst), 0);
    timeout(Duration::from_secs(2), peer.daemon.plugin_commands())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer.daemon.generation.load(Ordering::SeqCst), 1);
    browser_frame(&mut body, "after reconnect").await;
}

#[tokio::test]
async fn a_reader_replaced_during_reconnect_keeps_its_upstream_subscription() {
    let peer = Arc::new(Peer::losing_first_reply("plugin.commands").await);
    let old = browser_stream(&peer, "chat").await;
    peer.hold_subscription.store(true, Ordering::SeqCst);
    let daemon = peer.daemon.clone();
    let repairing = tokio::spawn(async move { daemon.plugin_commands().await });
    timeout(Duration::from_secs(2), peer.subscription_started.notified())
        .await
        .unwrap();
    drop(old);
    let joining_peer = peer.clone();
    let joining = tokio::spawn(async move { browser_stream(&joining_peer, "chat").await });
    peer.release_subscription.notify_one();
    timeout(Duration::from_secs(2), repairing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let mut body = timeout(Duration::from_secs(2), joining)
        .await
        .unwrap()
        .unwrap();
    peer.daemon.plugin_commands().await.unwrap();
    browser_frame(&mut body, "after reconnect").await;
}

#[tokio::test]
async fn simultaneous_last_readers_unsubscribe_upstream_once() {
    let peer = Peer::losing_first_reply("never").await;
    let mut streams = Vec::new();
    for _ in 0..3 {
        streams.push(peer.daemon.subscribe_events("chat").await.unwrap());
    }
    drop(streams);
    timeout(Duration::from_secs(2), async {
        while !peer.daemon.broker.sessions.read().await.is_empty()
            || peer.unsubscriptions.load(Ordering::SeqCst) == 0
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the last reader releases the upstream interest");
    // Each release settles through the flight of the session.
    peer.daemon.reconcile("chat").await.unwrap();
    assert_eq!(peer.unsubscriptions.load(Ordering::SeqCst), 1);
}

/// A browser that reconnected with a cursor must not lose the events of a
/// restarted daemon. That daemon numbers them from its persisted log, so a
/// new event can have a seq at or below the cursor. The reconnect gap ends
/// the replay filter.
#[tokio::test]
async fn a_cursor_does_not_hide_the_events_of_a_restarted_daemon() {
    let peer = Peer::losing_first_reply("plugin.commands").await;
    let mut body = browser_stream_at(&peer, "/api/events?topics=chat&after=chat:5").await;
    peer.daemon.plugin_commands().await.unwrap();
    let text = browser_frame(&mut body, "after reconnect").await;
    assert!(text.contains("stream_gap"), "{text}");
    assert!(
        text.contains("id: chat:1"),
        "the new event keeps its seq: {text}"
    );
}

/// The router of a dead connection stops before the reconnect gap goes out,
/// so no event of that connection reaches a browser after the gap.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_event_of_the_dead_connection_follows_the_reconnect_gap() {
    use futures::StreamExt;
    let peer = Peer::losing_first_reply("never").await;
    let (stale, stale_rx) = mpsc::unbounded_channel();
    peer.daemon.rewire_events(stale_rx).await;
    let mut events = peer.daemon.subscribe_events("chat").await.unwrap();
    let feeder = std::thread::spawn(move || {
        let event = SessionEvent::new("chat", "text_delta", json!({"content": "stale"}));
        for _ in 0..200_000 {
            if stale.send(event.clone()).is_err() {
                break;
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(5)).await;
    let (_fresh, fresh_rx) = mpsc::unbounded_channel();
    peer.daemon.rewire_events(fresh_rx).await;
    feeder.join().unwrap();

    let mut after_gap = Vec::new();
    let mut gap_seen = false;
    while let Ok(Some(event)) = timeout(Duration::from_millis(200), events.next()).await {
        let reconnect_gap = event.event == "stream_gap" && event.data["dropped"] == 0;
        if reconnect_gap {
            gap_seen = true;
            after_gap.clear();
        } else if gap_seen {
            after_gap.push(event);
        }
    }
    assert!(gap_seen, "the reconnect gap reached the stream");
    assert!(
        after_gap.iter().all(|event| event.event == "stream_gap"),
        "{} stale events followed the gap",
        after_gap.len()
    );
}

/// A slow flight of one session does not delay a stream of another.
#[tokio::test]
async fn a_slow_subscription_of_one_session_does_not_block_another() {
    let peer = Peer::losing_first_reply("never").await;
    let slow = peer.daemon.interest_flight_for_tests("slow");
    let _held = slow.lock().await;
    let other = timeout(
        Duration::from_secs(2),
        peer.daemon.subscribe_events("other"),
    )
    .await
    .expect("another session subscribes while the slow flight runs");
    assert!(other.is_ok());
}

/// A stream that drops with no tokio runtime on its thread still releases
/// the upstream interest.
#[tokio::test]
async fn a_stream_dropped_outside_a_runtime_releases_its_interest() {
    let peer = Peer::losing_first_reply("never").await;
    let stream = peer.daemon.subscribe_events("chat").await.unwrap();
    std::thread::spawn(move || {
        assert!(tokio::runtime::Handle::try_current().is_err());
        drop(stream);
    })
    .join()
    .unwrap();
    timeout(Duration::from_secs(2), async {
        while peer.unsubscriptions.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the release reached the daemon");
    assert!(peer.daemon.broker.sessions.read().await.is_empty());
}

#[tokio::test]
async fn explicit_session_end_closes_streams_without_unsubscribing_a_later_reader() {
    use futures::StreamExt;
    let peer = Peer::losing_first_reply("never").await;
    let mut old = peer.daemon.subscribe_events("chat").await.unwrap();
    peer.daemon.close_event_streams("chat").await;
    assert!(timeout(Duration::from_secs(2), old.next())
        .await
        .unwrap()
        .is_none());
    let mut current = peer.daemon.subscribe_events("chat").await.unwrap();
    drop(old);
    peer.daemon.plugin_commands().await.unwrap();
    let event = timeout(Duration::from_secs(2), current.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event, "text_delta");
    assert_eq!(peer.unsubscriptions.load(Ordering::SeqCst), 1);
}
