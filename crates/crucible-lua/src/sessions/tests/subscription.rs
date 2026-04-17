use super::super::*;
use super::MockDaemonApi;
use crate::test_support::TestLuaBuilder;
use mlua::Table;
use std::sync::Arc;

#[tokio::test]
async fn sessions_subscribe_returns_iterator() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    // Subscribe and read events
    let result: Table = lua
        .load(
            r#"
            local next_event, err = cru.sessions.subscribe("test-session")
            assert(err == nil, "subscribe error: " .. tostring(err))
            assert(type(next_event) == "function", "expected function iterator")

            local events = {}
            -- Read the two events the mock sends
            local e1 = next_event()
            if e1 then events[#events + 1] = e1 end
            local e2 = next_event()
            if e2 then events[#events + 1] = e2 end

            return {
                count = #events,
                first_type = events[1] and events[1].type or "none",
                first_text = events[1] and events[1].data and events[1].data.content or "none",
                second_text = events[2] and events[2].data and events[2].data.content or "none",
            }
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.get::<i64>("count").unwrap(), 2);
    assert_eq!(result.get::<String>("first_type").unwrap(), "text_delta");
    assert_eq!(result.get::<String>("first_text").unwrap(), "Hello");
    assert_eq!(result.get::<String>("second_text").unwrap(), " World");
}

// -----------------------------------------------------------------------
// Diagnostic tests for async subscribe/next_event channel delivery
// -----------------------------------------------------------------------
//
// These tests reproduce the Discord plugin scenario where:
//   1. Lua calls subscribe() -> gets next_event function
//   2. Lua calls send_message() -> triggers agent processing
//   3. A background task sends events into the mpsc channel
//   4. Lua calls next_event() -> should receive events
//
// The existing MockDaemonApi sends events synchronously (before subscribe
// returns), so the receiver already has buffered data. The real daemon
// sends events asynchronously AFTER subscribe returns. These tests use
// a mock that delays event delivery to surface timing/async issues.

/// Mock that holds onto the mpsc sender so events can be sent asynchronously
/// after subscribe() returns.
struct AsyncMockDaemonApi {
    /// Shared sender — tests inject events after subscribe returns.
    event_tx: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>>,
    /// Notify when subscribe() has been called and the sender is available.
    subscribe_barrier: Arc<tokio::sync::Notify>,
}

impl AsyncMockDaemonApi {
    fn new() -> Self {
        Self {
            event_tx: std::sync::Mutex::new(None),
            subscribe_barrier: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Get a clone of the event sender (waits until subscribe is called).
    fn get_sender(&self) -> Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>> {
        self.event_tx.lock().unwrap().clone()
    }
}

impl DaemonSessionApi for AsyncMockDaemonApi {
    fn create_session(
        &self,
        _: String,
        _: Option<String>,
        _: Option<String>,
        _: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        Box::pin(async { Ok(serde_json::json!({"id": "s1"})) })
    }
    fn get_session(
        &self,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>, String>> + Send>>
    {
        Box::pin(async { Ok(None) })
    }
    fn list_sessions(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn configure_agent(
        &self,
        _: String,
        _: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn send_message(
        &self,
        _: String,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        Box::pin(async { Ok("msg-001".to_string()) })
    }
    fn cancel(&self, _: String) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
        Box::pin(async { Ok(true) })
    }
    fn pause(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn resume(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn end_session(
        &self,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn respond_to_permission(
        &self,
        _: String,
        _: String,
        _: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn subscribe(
        &self,
        _session_id: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
                        String,
                    >,
                > + Send,
        >,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        // Store the sender so the test can inject events later
        *self.event_tx.lock().unwrap() = Some(tx);
        self.subscribe_barrier.notify_one();
        Box::pin(async { Ok(rx) })
    }

    fn unsubscribe(
        &self,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn load_messages(
        &self,
        _: String,
        _: Option<String>,
        _: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async { Ok(vec![]) })
    }

    fn inject_context(
        &self,
        _session_id: String,
        _role: String,
        _content: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn collect_subagents(
        &self,
        _job_ids: Vec<String>,
        _timeout_secs: Option<f64>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async { Ok(vec![]) })
    }

    fn fork_session(
        &self,
        _session_id: String,
        _up_to: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        Box::pin(async {
            Ok(serde_json::json!({
                "id": "fork-123",
                "parent_id": "parent-123",
                "messages_copied": 0,
            }))
        })
    }

    fn send_and_collect(
        &self,
        _session_id: String,
        _content: String,
        _timeout_secs: Option<f64>,
        _max_tool_result_len: Option<usize>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>,
                > + Send,
        >,
    > {
        Box::pin(async {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let _ = tx.send(ResponsePart::Text {
                content: "mock response".to_string(),
            });
            drop(tx);
            Ok(rx)
        })
    }
}

/// Test 1: subscribe + next_event with events sent AFTER subscribe returns.
///
/// This is the core scenario: Lua calls subscribe(), gets a next_event
/// function, then calls next_event() which must await on the mpsc receiver.
/// A Rust task sends events into the channel after a delay.
///
/// If this test fails, the issue is in mlua's create_async_function + recv.
#[tokio::test]
async fn subscribe_next_event_receives_async_events() {
    let api = Arc::new(AsyncMockDaemonApi::new());
    let barrier = Arc::clone(&api.subscribe_barrier);

    let lua = TestLuaBuilder::new()
        .with_sessions_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>)
        .build();

    // Spawn a Rust task that waits for subscribe, then sends events
    let api_clone = Arc::clone(&api);
    tokio::spawn(async move {
        // Wait until Lua calls subscribe()
        barrier.notified().await;
        // Small delay to ensure next_event() is already awaiting
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        if let Some(tx) = api_clone.get_sender() {
            let _ = tx.send(serde_json::json!({
                "type": "text_delta",
                "session_id": "test-session",
                "data": { "text": "async-hello" }
            }));
            // Drop the sender so next_event eventually returns nil
            drop(tx);
        }
    });

    let result: Table = lua
        .load(
            r#"
            local next_event, err = cru.sessions.subscribe("test-session")
            assert(err == nil, "subscribe error: " .. tostring(err))
            assert(type(next_event) == "function", "expected function, got " .. type(next_event))

            -- This should block/yield until the Rust task sends the event
            local event, event_err = next_event()
            assert(event ~= nil, "expected event, got nil (event_err=" .. tostring(event_err) .. ")")

            return {
                event_type = event.type,
                text = event.data and event.data.text or "none",
            }
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(
        result.get::<String>("event_type").unwrap(),
        "text_delta",
        "next_event() should have received the async event"
    );
    assert_eq!(
        result.get::<String>("text").unwrap(),
        "async-hello",
        "Event data should match what was sent"
    );
}

/// Test 2: Full Discord plugin flow — subscribe, send_message, then
/// next_event in the same Lua execution context.
///
/// This reproduces the exact sequence from responder.lua:
///   local next_event = cru.sessions.subscribe(session_id)
///   cru.sessions.send_message(session_id, content)
///   local event = next_event()
#[tokio::test]
async fn subscribe_send_message_then_next_event() {
    let api = Arc::new(AsyncMockDaemonApi::new());
    let barrier = Arc::clone(&api.subscribe_barrier);

    let lua = TestLuaBuilder::new()
        .with_sessions_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>)
        .build();

    // Spawn a Rust task that sends events after subscribe + send_message
    let api_clone = Arc::clone(&api);
    tokio::spawn(async move {
        barrier.notified().await;
        // Simulate agent processing delay
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        if let Some(tx) = api_clone.get_sender() {
            let _ = tx.send(serde_json::json!({
                "type": "text_delta",
                "session_id": "test-session",
                "data": { "text": "response chunk" }
            }));
            let _ = tx.send(serde_json::json!({
                "type": "stream_end",
                "session_id": "test-session",
                "data": {}
            }));
            drop(tx);
        }
        // Clear the stored sender so ALL senders are dropped and recv() returns None
        *api_clone.event_tx.lock().unwrap() = None;
    });

    let result: Table = lua
        .load(
            r#"
            -- Step 1: Subscribe
            local next_event, sub_err = cru.sessions.subscribe("test-session")
            assert(sub_err == nil, "subscribe error: " .. tostring(sub_err))

            -- Step 2: Send message (triggers agent processing)
            local msg_id, msg_err = cru.sessions.send_message("test-session", "Hello!")
            assert(msg_err == nil, "send_message error: " .. tostring(msg_err))

            -- Step 3: Read events (should yield until events arrive)
            local events = {}
            while true do
                local event = next_event()
                if event == nil then break end
                events[#events + 1] = event
            end

            return {
                msg_id = msg_id,
                event_count = #events,
                first_type = events[1] and events[1].type or "none",
                last_type = events[#events] and events[#events].type or "none",
            }
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.get::<String>("msg_id").unwrap(), "msg-001");
    assert_eq!(
        result.get::<i64>("event_count").unwrap(),
        2,
        "Should have received 2 events"
    );
    assert_eq!(result.get::<String>("first_type").unwrap(), "text_delta");
    assert_eq!(result.get::<String>("last_type").unwrap(), "stream_end");
}

/// Test 3: next_event called from within a timer.timeout wrapper.
///
/// Tests that create_async_function works when nested inside another
/// async Lua call (timeout wraps the function in tokio::time::timeout).
#[tokio::test]
async fn subscribe_next_event_inside_timeout() {
    let api = Arc::new(AsyncMockDaemonApi::new());
    let barrier = Arc::clone(&api.subscribe_barrier);

    let lua = TestLuaBuilder::new()
        .with_sessions_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>)
        .build();
    crate::timer::register_timer_module(&lua).unwrap();

    let api_clone = Arc::clone(&api);
    tokio::spawn(async move {
        barrier.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        if let Some(tx) = api_clone.get_sender() {
            let _ = tx.send(serde_json::json!({
                "type": "text_delta",
                "data": { "text": "timed" }
            }));
            drop(tx);
        }
    });

    let result: Table = lua
        .load(
            r#"
            local next_event, err = cru.sessions.subscribe("test-session")
            assert(err == nil, "subscribe error: " .. tostring(err))

            -- Wrap next_event in a timeout to avoid hanging forever if broken
            local ok, event = cru.timer.timeout(5.0, function()
                return next_event()
            end)

            return {
                timed_out = not ok,
                text = ok and event and event.data and event.data.text or "none",
            }
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let timed_out: bool = result.get("timed_out").unwrap();
    assert!(
        !timed_out,
        "next_event() should not have timed out — event was sent"
    );
    assert_eq!(result.get::<String>("text").unwrap(), "timed");
}

/// Test 4: Verify that events are NOT lost due to the channel being
/// dropped prematurely.
///
/// This specifically tests that the UnboundedReceiver returned by
/// subscribe() stays alive as long as the Lua next_event closure exists.
#[tokio::test]
async fn subscribe_receiver_not_dropped_prematurely() {
    let api = Arc::new(AsyncMockDaemonApi::new());
    let barrier = Arc::clone(&api.subscribe_barrier);

    let lua = TestLuaBuilder::new()
        .with_sessions_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>)
        .build();
    crate::timer::register_timer_module(&lua).unwrap();

    let api_clone = Arc::clone(&api);
    tokio::spawn(async move {
        barrier.notified().await;
        // Wait longer — the Lua side will sleep before calling next_event
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        if let Some(tx) = api_clone.get_sender() {
            let _ = tx.send(serde_json::json!({
                "type": "text_delta",
                "data": { "text": "delayed-event" }
            }));
            drop(tx);
        }
    });

    let result: Table = lua
        .load(
            r#"
            local next_event, err = cru.sessions.subscribe("test-session")
            assert(err == nil, "subscribe error: " .. tostring(err))

            -- Simulate some work between subscribe and reading events
            -- (like send_message + other setup in the Discord plugin)
            cru.timer.sleep(0.1)

            -- Now read the event
            local ok, event = cru.timer.timeout(5.0, function()
                return next_event()
            end)

            return {
                timed_out = not ok,
                text = ok and event and event.data and event.data.text or "none",
            }
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let timed_out: bool = result.get("timed_out").unwrap();
    assert!(
        !timed_out,
        "next_event() timed out — receiver may have been dropped"
    );
    assert_eq!(result.get::<String>("text").unwrap(), "delayed-event");
}

/// Test 5: The cru.spawn pattern — subscribe + next_event from a spawned
/// Lua task, mimicking the Discord plugin's responder flow.
///
/// This is the exact pattern that was failing in production:
///   cru.spawn(function()
///     local next_event = cru.sessions.subscribe(session_id)
///     cru.sessions.send_message(session_id, content)
///     local event = next_event()  -- THIS was hanging
///   end)
///
/// NOTE: cru.spawn uses tokio::spawn which requires the mlua `send`
/// feature. Without it, this test will not compile.
#[cfg(feature = "send")]
#[tokio::test]
async fn subscribe_next_event_via_cru_spawn() {
    let api = Arc::new(AsyncMockDaemonApi::new());
    let barrier = Arc::clone(&api.subscribe_barrier);

    let lua = TestLuaBuilder::new()
        .with_sessions_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>)
        .build();
    crate::timer::register_timer_module(&lua).unwrap();

    // Spawn a Rust task that sends events after subscribe is called
    let api_clone = Arc::clone(&api);
    tokio::spawn(async move {
        barrier.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        if let Some(tx) = api_clone.get_sender() {
            let _ = tx.send(serde_json::json!({
                "type": "text_delta",
                "data": { "text": "spawned-event" }
            }));
            drop(tx);
        }
        *api_clone.event_tx.lock().unwrap() = None;
    });

    // Use a shared table to capture results from the spawned task.
    // cru.spawn is fire-and-forget, so we use a global to communicate.
    let result = lua
        .load(
            r#"
            -- Shared result table
            _G.spawn_result = { done = false, text = "not-set" }

            cru.spawn(function()
                local next_event, err = cru.sessions.subscribe("test-session")
                if err then
                    _G.spawn_result.text = "subscribe error: " .. err
                    _G.spawn_result.done = true
                    return
                end

                local event = next_event()
                if event then
                    _G.spawn_result.text = event.data and event.data.text or "no-text"
                else
                    _G.spawn_result.text = "nil-event"
                end
                _G.spawn_result.done = true
            end)

            -- Wait for the spawned task to complete (poll with sleep)
            local waited = 0
            while not _G.spawn_result.done and waited < 5 do
                cru.timer.sleep(0.05)
                waited = waited + 0.05
            end

            return _G.spawn_result
            "#,
        )
        .eval_async::<Table>()
        .await;

    match result {
        Ok(table) => {
            let done: bool = table.get("done").unwrap();
            let text: String = table.get("text").unwrap();
            assert!(done, "Spawned task should have completed");
            assert_eq!(
                text, "spawned-event",
                "Event should have been received in the spawned task"
            );
        }
        Err(e) => {
            panic!(
                "Lua execution failed: {}. This likely means cru.spawn \
                 cannot call async functions — check if mlua 'send' feature is enabled",
                e
            );
        }
    }
}

/// Test 6: Multiple sequential next_event calls receive events in order.
///
/// Verifies that the Arc<Mutex<Receiver>> pattern works correctly across
/// multiple invocations of the same next_event closure.
#[tokio::test]
async fn subscribe_multiple_next_event_calls_receive_in_order() {
    let api = Arc::new(AsyncMockDaemonApi::new());
    let barrier = Arc::clone(&api.subscribe_barrier);

    let lua = TestLuaBuilder::new()
        .with_sessions_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>)
        .build();
    crate::timer::register_timer_module(&lua).unwrap();

    let api_clone = Arc::clone(&api);
    tokio::spawn(async move {
        barrier.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        if let Some(tx) = api_clone.get_sender() {
            for i in 1..=5 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "data": { "text": format!("chunk-{}", i) }
                }));
            }
            drop(tx);
        }
        // Clear stored sender so recv() returns None
        *api_clone.event_tx.lock().unwrap() = None;
    });

    let result: Table = lua
        .load(
            r#"
            local next_event, err = cru.sessions.subscribe("test-session")
            assert(err == nil, "subscribe error: " .. tostring(err))

            local texts = {}
            local ok, res = cru.timer.timeout(5.0, function()
                while true do
                    local event = next_event()
                    if event == nil then break end
                    texts[#texts + 1] = event.data.text
                end
            end)

            return {
                timed_out = not ok,
                count = #texts,
                first = texts[1] or "none",
                last = texts[#texts] or "none",
            }
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let timed_out: bool = result.get("timed_out").unwrap();
    assert!(!timed_out, "Should not time out");
    assert_eq!(
        result.get::<i64>("count").unwrap(),
        5,
        "Should receive all 5 events"
    );
    assert_eq!(result.get::<String>("first").unwrap(), "chunk-1");
    assert_eq!(result.get::<String>("last").unwrap(), "chunk-5");
}
