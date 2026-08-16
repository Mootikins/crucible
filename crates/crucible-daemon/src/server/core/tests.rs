//! Tests for the connection layer: request concurrency, the broadcast gap
//! marker, the write timeout, and the panic boundary.
//!
//! Split out of `core/mod.rs` to keep that file inside the 1000-line module
//! budget `no_new_oversized_modules` enforces. Every module below was written
//! against a socket pair rather than a live daemon, which is what makes the
//! hazards — a lagged receiver, a peer that stops draining, a parked handler —
//! reachable without timing guesses.

mod panic_boundary_tests {
    use futures::FutureExt;

    /// The property `handle_request` provides: a panicking handler becomes an
    /// error response, not a dropped connection.
    ///
    /// This exercises the same `AssertUnwindSafe` + `catch_unwind` composition
    /// the real path uses. Driving `handle_request` itself would need a whole
    /// live `ServerContext`; what can actually regress here is the catch
    /// composition, since the panic escapes the moment it is removed.
    #[tokio::test]
    async fn a_panicking_future_is_caught_rather_than_unwinding() {
        let caught = std::panic::AssertUnwindSafe(async {
            // The shape of the real one: a byte index inside a codepoint.
            let s = "an em dash — here";
            let _ = &s[..12];
            "unreachable"
        })
        .catch_unwind()
        .await;

        let panic = caught.expect_err("slicing mid-codepoint must panic");
        let detail = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("unknown panic");

        assert!(
            detail.contains("char boundary"),
            "the panic payload is recoverable for the error message: {detail:?}"
        );
    }

    /// A handler that does not panic must be entirely unaffected.
    #[tokio::test]
    async fn a_normal_future_passes_through_untouched() {
        let result = std::panic::AssertUnwindSafe(async { 42 })
            .catch_unwind()
            .await;
        assert_eq!(result.ok(), Some(42));
    }
}

mod per_request_spawn_tests {
    use super::super::*;
    use tokio::io::AsyncBufReadExt;
    use tokio::sync::oneshot;

    /// Connect a socket pair to `serve_requests` and hand back the client end.
    fn spawn_loop<F, Fut>(
        max_inflight: usize,
        serve: F,
    ) -> (
        tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
        tokio::net::unix::OwnedWriteHalf,
        tokio::task::JoinHandle<Result<()>>,
    )
    where
        F: Fn(String) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        spawn_loop_with_shutdown(max_inflight, unarmed_shutdown(), serve)
    }

    /// A latch no test arms, for the loops that are not about shutdown.
    fn unarmed_shutdown() -> Arc<DeferredShutdown> {
        Arc::new(DeferredShutdown::new(broadcast::channel(1).0))
    }

    /// As `spawn_loop`, against a shutdown latch the caller owns.
    fn spawn_loop_with_shutdown<F, Fut>(
        max_inflight: usize,
        shutdown: Arc<DeferredShutdown>,
        serve: F,
    ) -> (
        tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
        tokio::net::unix::OwnedWriteHalf,
        tokio::task::JoinHandle<Result<()>>,
    )
    where
        F: Fn(String) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        let (server_side, client_side) = tokio::net::UnixStream::pair().expect("socketpair");
        let (server_read, server_write) = server_side.into_split();
        let (client_read, client_write) = client_side.into_split();
        let task = tokio::spawn(serve_requests(
            server_read,
            Arc::new(Mutex::new(server_write)),
            ClientId::new(),
            max_inflight,
            shutdown,
            serve,
        ));
        (tokio::io::BufReader::new(client_read), client_write, task)
    }

    async fn send(w: &mut tokio::net::unix::OwnedWriteHalf, id: u64, method: &str) {
        w.write_all(
            format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"{method}\"}}\n").as_bytes(),
        )
        .await
        .expect("write request");
    }

    /// Reply ids in the order they come back. A deadline, not a timing
    /// assumption: a request that never gets served would otherwise hang the
    /// test instead of failing it.
    async fn next_reply_id(r: &mut tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>) -> u64 {
        let mut line = String::new();
        tokio::time::timeout(std::time::Duration::from_secs(10), r.read_line(&mut line))
            .await
            .expect("a reply must arrive; a blocked loop would hang here")
            .expect("read a line");
        let v: serde_json::Value = serde_json::from_str(&line).expect("parse reply");
        v["id"].as_u64().expect("numeric id")
    }

    /// The latency bug, stated as the behaviour that fixes it: a request parked
    /// inside its handler must not delay a request that arrives after it.
    ///
    /// Deterministic — the slow handler waits on a channel this test owns, so the
    /// second reply can only be produced by the loop having read and served
    /// request 2 while request 1 was still parked. Nothing here waits on a clock.
    ///
    /// `one_permit_serialises_handlers_on_a_connection` below is the negative
    /// control: with a single permit the loop is serial again — the behaviour this
    /// replaces — and this test's first assertion fails.
    #[tokio::test]
    async fn a_parked_request_does_not_delay_a_later_one_on_the_same_connection() {
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let release = Arc::new(Mutex::new(Some(release_rx)));

        let (mut reader, mut writer, task) = spawn_loop(4, move |line: String| {
            let release = release.clone();
            async move {
                let req: Request = serde_json::from_str(&line).expect("parse request");
                if req.method == "slow" {
                    let rx = release
                        .lock()
                        .await
                        .take()
                        .expect("exactly one slow request in this test");
                    let _ = rx.await;
                }
                Response::success(req.id, serde_json::json!(req.method))
            }
        });

        send(&mut writer, 1, "slow").await;
        send(&mut writer, 2, "fast").await;

        assert_eq!(
            next_reply_id(&mut reader).await,
            2,
            "request 2 must be answered while request 1 is still parked"
        );

        release_tx
            .send(())
            .expect("the slow handler is still waiting");
        assert_eq!(next_reply_id(&mut reader).await, 1);

        drop(writer);
        task.await.expect("loop task").expect("clean EOF");
    }

    /// The bound has to actually engage, or the spawn is unbounded task growth
    /// wearing a constant.
    ///
    /// Asserted as an *ordering*, not as an absence: with one permit, handler 2
    /// cannot begin until handler 1 has returned, so the log must read
    /// enter-1, exit-1, enter-2, exit-2. Unbounded, handler 2 is spawned and
    /// polled immediately after request 2 is read — before this test has even
    /// released handler 1 — so the log reads enter-1, enter-2, … and the
    /// assertion fails. `#[tokio::test]` is a current-thread runtime, so that
    /// interleaving is deterministic rather than merely likely.
    #[tokio::test]
    async fn one_permit_serialises_handlers_on_a_connection() {
        let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (gate_tx, gate_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let gate = Arc::new(Mutex::new(gate_rx));

        let (mut reader, mut writer, task) = spawn_loop(1, move |line: String| {
            let log = log_tx.clone();
            let gate = gate.clone();
            async move {
                let req: Request = serde_json::from_str(&line).expect("parse request");
                let id = match req.id.clone() {
                    Some(RequestId::Number(n)) => n,
                    other => panic!("expected a numeric id, got {other:?}"),
                };
                let _ = log.send(format!("enter-{id}"));
                // One release per handler, handed out in the order the test
                // decides — so "handler 2 has not started" is observable as
                // "enter-2 has not been logged".
                gate.lock()
                    .await
                    .recv()
                    .await
                    .expect("a release per handler");
                let _ = log.send(format!("exit-{id}"));
                Response::success(req.id, serde_json::json!("ok"))
            }
        });

        send(&mut writer, 1, "park").await;
        send(&mut writer, 2, "park").await;

        assert_eq!(log_rx.recv().await.as_deref(), Some("enter-1"));
        gate_tx.send(()).expect("handler 1 is waiting");
        assert_eq!(log_rx.recv().await.as_deref(), Some("exit-1"));
        assert_eq!(
            log_rx.recv().await.as_deref(),
            Some("enter-2"),
            "handler 2 must only begin once handler 1 released its permit"
        );
        gate_tx.send(()).expect("handler 2 is waiting");
        assert_eq!(log_rx.recv().await.as_deref(), Some("exit-2"));

        assert_eq!(next_reply_id(&mut reader).await, 1);
        assert_eq!(next_reply_id(&mut reader).await, 2);

        drop(writer);
        task.await.expect("loop task").expect("clean EOF");
    }

    /// A shutdown armed by a handler reaches the accept loop — but only once its
    /// confirmation has been written to the socket.
    ///
    /// The daemon exits within a scheduling slice of that signal, so the reply
    /// has to be on the wire first; signalling from inside the handler left
    /// `cru daemon stop` and the lifecycle e2e test reading EOF instead of their
    /// own confirmation whenever the machine was loaded enough to deschedule the
    /// writer. `dispatching_shutdown_confirms_before_it_signals` is the other
    /// half: the handler arms and does not signal.
    #[tokio::test]
    async fn an_armed_shutdown_is_signalled_once_its_reply_is_written() {
        let shutdown = unarmed_shutdown();
        let mut signal = shutdown.subscribe();
        let armer = shutdown.clone();
        let (mut reader, mut writer, task) =
            spawn_loop_with_shutdown(4, shutdown, move |line: String| {
                let armer = armer.clone();
                async move {
                    let req: Request = serde_json::from_str(&line).expect("parse request");
                    armer.arm();
                    Response::success(req.id, serde_json::json!("shutting down"))
                }
            });

        send(&mut writer, 1, "shutdown").await;
        assert_eq!(next_reply_id(&mut reader).await, 1);

        tokio::time::timeout(std::time::Duration::from_secs(10), signal.recv())
            .await
            .expect("the armed shutdown must reach the accept loop")
            .expect("the signal sender outlives the loop");

        drop(writer);
        task.await.expect("loop task").expect("clean EOF");
    }

    /// A client that stops draining must still close the connection, now that the
    /// write happens inside a handler rather than in the read loop. Without the
    /// cancellation token the loop would go on accepting requests whose replies
    /// can never be delivered, and never reach `handle_client`'s cleanup.
    ///
    /// Paused time, so the 30s write timeout costs nothing and fires exactly when
    /// the write genuinely cannot progress.
    #[tokio::test(start_paused = true)]
    async fn a_write_that_times_out_closes_the_connection() {
        let (server_side, client_side) = tokio::net::UnixStream::pair().expect("socketpair");
        let (server_read, server_write) = server_side.into_split();
        // Never read from: the reply below fills the socket buffers and blocks.
        let (_client_read, mut client_write) = client_side.into_split();

        let task = tokio::spawn(serve_requests(
            server_read,
            Arc::new(Mutex::new(server_write)),
            ClientId::new(),
            4,
            unarmed_shutdown(),
            move |line: String| async move {
                let req: Request = serde_json::from_str(&line).expect("parse request");
                // Well past any plausible SO_SNDBUF + SO_RCVBUF.
                Response::success(req.id, serde_json::json!("x".repeat(16 * 1024 * 1024)))
            },
        ));

        send(&mut client_write, 1, "huge").await;

        // No outer timeout on purpose. The write timeout must be the *only*
        // pending timer: auto-advance jumps to the earliest deadline, and a second
        // timer registered before the handler is first polled would be jumped to
        // instead, leaving the clock past both. A regression here therefore hangs
        // rather than asserting, and nextest's per-test timeout reports it.
        let err = task
            .await
            .expect("loop task")
            .expect_err("a client that never reads must not leave the loop running");
        assert!(
            err.to_string().contains("closing"),
            "the loop must report why it closed: {err}"
        );
    }
}

mod stream_gap_tests {
    use super::super::*;
    use tokio::io::AsyncBufReadExt;

    /// Drive `forward_events` over a socket pair and hand back the client end.
    fn spawn_forwarder(
        event_rx: broadcast::Receiver<SessionEventMessage>,
        client_id: ClientId,
        sub_manager: Arc<SubscriptionManager>,
    ) -> (
        tokio::io::BufReader<tokio::net::UnixStream>,
        CancellationToken,
        tokio::task::JoinHandle<()>,
    ) {
        let (server_side, client_side) = tokio::net::UnixStream::pair().expect("socketpair");
        let (_r, writer) = server_side.into_split();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(forward_events(
            event_rx,
            Arc::new(Mutex::new(writer)),
            sub_manager,
            client_id,
            cancel.clone(),
        ));
        (tokio::io::BufReader::new(client_side), cancel, task)
    }

    async fn next_event(
        reader: &mut tokio::io::BufReader<tokio::net::UnixStream>,
    ) -> SessionEventMessage {
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("read a line");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("parse {line:?}: {e}"))
    }

    /// A client whose cursor fell off the back of the ring must be *told*, in
    /// band, how many events it lost.
    ///
    /// Before this, `Lagged(n)` was a `warn!` on the daemon's side and nothing
    /// else: the client rendered a transcript with a hole in it and had no way to
    /// notice. That is the one data-loss item in this set — every other hazard
    /// costs latency or one wedged connection.
    ///
    /// Fully deterministic, no sleeps: both receivers subscribe before anything
    /// is sent, so the ring wraps under their cursors and the first `recv` is
    /// `Lagged` by construction.
    #[tokio::test]
    async fn a_lagged_client_is_told_how_many_events_it_lost() {
        const CAP: usize = 4;
        const OVERFLOW: u64 = 3;

        let (tx, forwarder_rx) = broadcast::channel::<SessionEventMessage>(CAP);
        let mut witness = tx.subscribe();

        for i in 0..(CAP as u64 + OVERFLOW) {
            tx.send(SessionEventMessage::text_delta("s1", format!("chunk-{i}")))
                .expect("two receivers are alive");
        }

        // The hazard itself, asserted: a cursor that was subscribed before those
        // sends really does lose exactly OVERFLOW events. Without this the test
        // could pass against a channel that never lagged, and a gap marker that
        // is never reached would look like a working guard.
        match witness.recv().await {
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => assert_eq!(
                n, OVERFLOW,
                "the setup must drop exactly the events the marker will claim"
            ),
            other => panic!("expected the ring to have wrapped, got {other:?}"),
        }

        let client_id = ClientId::new();
        let sub_manager = Arc::new(SubscriptionManager::new());
        sub_manager.subscribe(client_id, "s1");
        let (mut reader, cancel, task) = spawn_forwarder(forwarder_rx, client_id, sub_manager);

        let gap = next_event(&mut reader).await;
        // Expected name derived from the payload enum, not a literal or a
        // parallel constant: the enum is what produces the wire name, so this
        // cannot pass while the two disagree.
        let (gap_name, _) = crucible_core::protocol::session_events::SessionEventPayload::from(
            crucible_core::protocol::session_events::SystemPayload::StreamGap { dropped: 0 },
        )
        .to_wire();
        assert_eq!(
            gap.event, gap_name,
            "the gap must be the first thing the client sees, before the \
             surviving events, or it would attribute the hole to the wrong place"
        );
        assert_eq!(gap.data["dropped"], OVERFLOW);
        assert_eq!(
            gap.session_id,
            crate::subscription::WILDCARD_SESSION,
            "Lagged(n) names no session, so the marker must not pretend to"
        );
        assert!(
            gap.seq.is_none(),
            "the marker is minted outside any session's sequence space; a seq \
             here would corrupt the very contiguity check it exists to explain"
        );

        // The forwarder must carry on, not die on the gap: the oldest surviving
        // event follows immediately.
        let survivor = next_event(&mut reader).await;
        assert_eq!(survivor.event, "text_delta");
        assert_eq!(
            survivor.data["content"], "chunk-3",
            "with capacity 4 and 7 sends, the ring holds chunks 3..6"
        );

        cancel.cancel();
        task.await.expect("forwarder task");
    }

    /// A client that keeps up must never see a gap marker — the signal is only
    /// worth anything if its absence means something.
    #[tokio::test]
    async fn a_client_that_keeps_up_sees_no_gap_marker() {
        let (tx, rx) = broadcast::channel::<SessionEventMessage>(16);
        let client_id = ClientId::new();
        let sub_manager = Arc::new(SubscriptionManager::new());
        sub_manager.subscribe(client_id, "s1");
        let (mut reader, cancel, task) = spawn_forwarder(rx, client_id, sub_manager);

        for i in 0..3u64 {
            tx.send(SessionEventMessage::text_delta("s1", format!("chunk-{i}")))
                .expect("receiver alive");
        }

        for i in 0..3u64 {
            let event = next_event(&mut reader).await;
            assert_eq!(event.event, "text_delta");
            assert_eq!(event.data["content"], format!("chunk-{i}"));
        }

        cancel.cancel();
        task.await.expect("forwarder task");
    }
}

mod client_write_timeout_tests {
    use super::super::*;

    /// A peer that stops draining must cost the daemon one closed connection,
    /// not a permanently wedged writer.
    ///
    /// Before the timeout, `write_all` on a full socket buffer blocked forever
    /// with the writer mutex held: the event forwarder and the request loop both
    /// stalled, `handle_client` never reached its cleanup, and the connection
    /// leaked a task plus a subscription entry for the daemon's lifetime.
    ///
    /// Deterministic and instant: `tokio::time::pause()` means the runtime
    /// auto-advances the clock once every task is blocked, so the timeout fires
    /// the moment `write_all` genuinely cannot make progress. No sleeps, and no
    /// 30s of wall time.
    #[tokio::test(start_paused = true)]
    async fn a_client_that_stops_reading_gets_its_connection_closed() {
        let (server_side, _client_side) = tokio::net::UnixStream::pair().expect("socketpair");
        let (_reader, writer) = server_side.into_split();
        let writer = Mutex::new(writer);

        // `_client_side` is never read from, so this fills the socket buffers
        // and then blocks — which is precisely the wedge being closed. Well
        // past any plausible SO_SNDBUF + SO_RCVBUF.
        let payload = vec![b'x'; 16 * 1024 * 1024];

        let result = write_line_or_close(&writer, &payload, ClientId::new()).await;

        assert!(
            result.is_err(),
            "a write the peer never accepts must fail so the caller closes the connection"
        );
    }

    /// The ordinary case is untouched: a peer that reads gets its bytes and the
    /// connection stays up.
    #[tokio::test(start_paused = true)]
    async fn a_client_that_reads_is_written_to_normally() {
        use tokio::io::AsyncReadExt;

        let (server_side, mut client_side) = tokio::net::UnixStream::pair().expect("socketpair");
        let (_reader, writer) = server_side.into_split();
        let writer = Mutex::new(writer);

        let reading = tokio::spawn(async move {
            let mut buf = [0_u8; 5];
            client_side.read_exact(&mut buf).await.expect("read");
            buf
        });

        write_line_or_close(&writer, b"hello", ClientId::new())
            .await
            .expect("a draining peer must be written to without error");

        assert_eq!(&reading.await.expect("reader task"), b"hello");
    }
}
