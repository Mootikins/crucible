# Learnings - Interaction Primitives

## [2026-01-25] Task 1-2: State and Event Setup

### Patterns Found

**InteractionModalState Location**: `crates/crucible-cli/src/tui/oil/chat_app.rs:267-281`
- Fields: `request_id`, `request`, `selected`, `filter`, `other_text`, `mode`
- Mode enum: `Selecting | TextInput`

**Event Flow**: 
- `SessionEvent::InteractionRequested` → `ChatAppMsg::OpenInteraction` → `InkChatApp::open_interaction()`
- Response: User action → `ChatAppMsg::CloseInteraction` → RPC call (pending)

### Conventions

**Modal Lifecycle**:
1. Daemon emits `SessionEvent::InteractionRequested { request_id, request }`
2. TUI opens modal with `open_interaction()`
3. User interacts (key events handled in `handle_interaction_key()`)
4. Submit/cancel → `CloseInteraction` message → RPC response

**Code Locations**:
- State: `chat_app.rs:267-281`
- Event handler: `chat_runner.rs` (SessionEvent match arm)
- View: `chat_app.rs:473-483` (dispatch to render_ask/render_perm)
- Key handling: `chat_app.rs:1865+` (`handle_interaction_key()`)

## [2026-01-25] Task 4: PermRequest Complete

### Implementation Details

**Render Method**: `render_perm_interaction()` at `chat_app.rs` (after prettify_tool_args)
- Displays permission type (Bash/Read/Write/Tool)
- Shows action details (tokens/path/tool name)
- Footer with key hints

**Key Handler**: `handle_perm_key()` at `chat_app.rs:1865-1918`
- `y`/`Y` → `PermResponse::allow()`
- `n`/`N` → `PermResponse::deny()`
- `p`/`P` → `PermResponse::allow_pattern()` using `pattern_at()`
- `Esc` → `InteractionResponse::Cancelled`

### Gotchas

- `handle_interaction_key()` currently ONLY handles Permission - needs refactor to dispatch both Ask and Perm
- Must clone `request_id` before consuming in response (ownership)

## [2026-01-25] Task 3: AskRequest Key Handling Complete

### Implementation Details

**Refactored `handle_interaction_key()`** at `chat_app.rs:1865-1881`
- Now dispatches by request type: `Ask => handle_ask_key()`, `Permission => handle_perm_key()`
- Clones `request_id` before dispatch (ownership pattern)
- Returns `Action::Continue` for unknown request types

**New `handle_ask_key()` method** at `chat_app.rs:1883-1971`
- Calculates `total_items = choices_count + (allow_other ? 1 : 0)`
- **Selecting mode**:
  - Up/k/K: Navigate up with wrapping (first→last)
  - Down/j/J: Navigate down with wrapping (last→first)
  - Enter: Submit choice (index < choices_count) or switch to TextInput (at "Other...")
  - Tab: Switch to TextInput if allow_other=true
  - Esc: Cancel (send `InteractionResponse::Cancelled`)
- **TextInput mode**:
  - Enter: Submit free-text (send `AskResponse::other(text)`)
  - Esc: Go back to Selecting mode
  - Backspace: Delete character
  - Char: Add character to buffer

**Extracted `handle_perm_key()` method** at `chat_app.rs:1973-2005`
- Moved existing Permission logic from old `handle_interaction_key()`
- Signature: `(&mut self, key, perm_request, request_id) -> Action<ChatAppMsg>`

### Key Patterns

**Choice Index Calculation** (matches `render_ask_interaction()`):
```rust
let choices_count = ask_request.choices.as_ref().map(|c| c.len()).unwrap_or(0);
let total_items = choices_count + if ask_request.allow_other { 1 } else { 0 };
```

**Wrapping Navigation**:
- Up: `if selected == 0 { selected = total_items - 1 } else { selected -= 1 }`
- Down: `selected = (selected + 1) % total_items.max(1)`

**Response Building**:
- Choice: `InteractionResponse::Ask(AskResponse::selected(index))`
- Free-text: `InteractionResponse::Ask(AskResponse::other(text))`
- Cancel: `InteractionResponse::Cancelled`

### Testing

- All 1564 tests pass (crucible-cli)
- No new test failures introduced
- Snapshot tests deferred to Task 4 (acceptance criteria)

### Gotchas Resolved

- ✅ Early return on non-Permission requests fixed (now dispatches Ask)
- ✅ Modal state mutations require `&mut self.interaction_modal`
- ✅ Cloning request_id prevents ownership issues in response building
- ✅ Wrapping navigation handles edge cases (empty choices, single item)

## [2026-01-25] Task 5: RPC Infrastructure Complete

### Implementation Summary

Added full RPC round-trip for interaction responses from TUI → daemon:

**Files Modified**:
1. `crucible-core/src/traits/chat.rs` - Added `interaction_respond()` method to `AgentHandle` trait
2. `crucible-daemon-client/src/client.rs` - Added `session_interaction_respond()` RPC method
3. `crucible-daemon-client/src/agent.rs` - Implemented `interaction_respond()` in `DaemonAgentHandle`
4. `crucible-daemon/src/server.rs` - Added RPC handler for `session.interaction_respond`
5. `crucible-cli/src/tui/oil/chat_runner.rs` - Wired `CloseInteraction` message to call agent RPC

### Key Patterns

**AgentHandle Trait Method**:
```rust
async fn interaction_respond(
    &mut self,
    request_id: String,
    response: InteractionResponse,
) -> ChatResult<()>
```

**RPC Client Method** (follows `session_send_message` pattern):
```rust
pub async fn session_interaction_respond(
    &self,
    session_id: &str,
    request_id: &str,
    response: InteractionResponse,
) -> Result<()> {
    self.call(
        "session.interaction_respond",
        serde_json::json!({
            "session_id": session_id,
            "request_id": request_id,
            "response": response
        }),
    ).await?;
    Ok(())
}
```

**Daemon Handler** (emits event via `SessionEventMessage::new`):
```rust
async fn handle_session_interaction_respond(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    // Parse params
    let response: InteractionResponse = 
        serde_json::from_value(serde_json::Value::Object(response_obj.clone()))?;
    
    // Emit event
    event_tx.send(SessionEventMessage::new(
        session_id,
        "interaction_completed",
        serde_json::json!({
            "request_id": request_id,
            "response": response,
        }),
    ));
    
    Response::success(req.id, ...)
}
```

**TUI Event Loop Handler** (in `chat_runner.rs:process_action`):
```rust
ChatAppMsg::CloseInteraction { request_id, response } => {
    match agent.interaction_respond(request_id.clone(), response.clone()).await {
        Ok(()) => tracing::info!("Interaction response sent"),
        Err(e) => tracing::warn!("Failed to send: {}", e),
    }
}
```

### Architecture Decisions

**Why in `process_action` not `process_message`?**
- `process_action` has mutable access to `agent: &mut A`
- `process_message` only has immutable access
- RPC calls need `&mut self` for async trait methods

**Why emit event instead of calling session method?**
- Daemon doesn't have a `Session::respond_to_interaction()` method yet
- Event emission is sufficient for TUI to receive confirmation
- Future: Could add session method for persistence/validation

**SessionEventMessage structure**:
- NOT the same as `SessionEvent` enum from crucible-core
- Uses `SessionEventMessage::new(session_id, event_type, data)` constructor
- `event_type` is string (e.g., "interaction_completed")
- `data` is JSON object with event-specific fields

### Gotchas Resolved

**Compilation Error 1**: `SessionEvent` type not in scope
- Fixed: Use `SessionEventMessage::new()` instead of struct literal

**Compilation Error 2**: `response_obj` is `Map<String, Value>` not `Value`
- Fixed: Wrap with `serde_json::Value::Object(response_obj.clone())`

**Compilation Error 3**: Missing `data` and `msg_type` fields
- Fixed: Use `SessionEventMessage::new()` constructor (sets `msg_type: "event"` automatically)

**Unused variable warning**: `am` parameter not used in handler
- Acceptable: Future handlers may need session access for validation

### Testing

- All 1640 tests pass (crucible-cli + crucible-daemon-client)
- No new test failures introduced
- Integration tests verify RPC round-trip works

### Flow Verification

**Complete round-trip**:
1. User responds to interaction in TUI → `CloseInteraction` message
2. `chat_runner.rs` intercepts message → calls `agent.interaction_respond()`
3. `DaemonAgentHandle` → calls `client.session_interaction_respond()`
4. `DaemonClient` → sends JSON-RPC to daemon
5. Daemon handler → emits `SessionEventMessage` with "interaction_completed"
6. Event broadcast to all subscribed clients

**Next Steps** (deferred to future tasks):
- Add `Session::respond_to_interaction()` method for state tracking
- Persist interaction responses in session history
- Add validation (e.g., request_id must match pending interaction)

## [2026-01-25] QA Blocker: Event Subscription Gap

### Problem Discovered

The TUI's `handle_session_event()` function in `chat_runner.rs` is **never called** because:

1. The TUI only listens for events DURING active message streaming via `DaemonAgentHandle.send_message_stream()`
2. The `send_message_stream()` function only converts streaming events (text_delta, thinking, tool_call, etc.) to `ChatChunk`
3. `interaction_requested` events are logged as "Unknown session event type" and ignored
4. There is NO background event listener for out-of-band events like interactions

### Current Architecture (Broken for Interactions)

```
TUI sends message → starts listening → receives streaming events → stops listening
                    ↑ ONLY listens here ↑
```

When daemon emits `interaction_requested`:
1. If TUI is streaming → event received but ignored (not a ChatChunk type)
2. If TUI is idle → event received by no one

### Required Architecture

```
TUI connects → subscribes to ALL session events → handles streaming + interactions in parallel
              ↑ ALWAYS listening ↑
```

### Root Cause Locations

**`crucible-daemon-client/src/agent.rs:89-182`** - `session_event_to_chat_chunk()`:
- Only handles: `text_delta`, `thinking`, `tool_call`, `tool_result`, `message_complete`, `ended`
- Returns `None` for `interaction_requested` → event discarded

**`crucible-cli/src/tui/oil/chat_runner.rs:626-660`** - `handle_session_event()`:
- Correctly handles `SessionEvent::InteractionRequested`
- But `crucible_core::events::SessionEvent` ≠ `crucible_daemon_client::SessionEvent`
- Function is NEVER CALLED

### Fix Options

**Option A: Add background event subscription in TUI**
- Add `event_rx` channel to `InkChatRunner` 
- Subscribe to session events when TUI starts
- Process events in parallel with terminal events in `event_loop()`

**Option B: Extend DaemonAgentHandle to emit interaction events**
- Add callback/channel for non-streaming events
- Convert `interaction_requested` to `ChatAppMsg::OpenInteraction`
- Less architectural change but couples agent handle to TUI

**Option C: Use AgentHandle trait for event subscription**
- Add `subscribe_events()` method to `AgentHandle` trait
- Return stream of `SessionEvent` (core type)
- TUI calls this separately from message streaming

**Recommended**: Option A (cleanest separation, TUI owns event loop)

### Workaround for QA

Manual QA cannot be performed until the event subscription gap is fixed. The test RPC `session.test_interaction` works correctly (verified via nc), but the TUI never sees the event.

**Verification that daemon emits correctly**:
```bash
echo '{"jsonrpc":"2.0","id":2,"method":"session.test_interaction","params":{"session_id":"...", "type":"ask"}}' | nc -U /run/user/1000/crucible.sock
# Returns: {"jsonrpc":"2.0","id":2,"result":{"session_id":"...","request_id":"test-...","type":"ask"}}
```

**Verification that TUI doesn't receive**:
- No modal appears
- No logs about InteractionRequested in TUI
- Event goes to void because TUI isn't subscribed

## [2026-01-25] Event Subscription Gap FIXED

### Solution Implemented

**Option chosen**: Modified Option A - Add event routing in `DaemonAgentHandle` with separate interaction channel

### Changes Made

1. **`crucible-core/src/traits/chat.rs`**:
   - Added `take_interaction_receiver()` method to `AgentHandle` trait
   - Returns `Option<mpsc::UnboundedReceiver<InteractionEvent>>`
   - Added blanket implementation for `Box<dyn AgentHandle>`

2. **`crucible-core/src/interaction.rs`**:
   - Added `InteractionEvent` struct: `{ request_id: String, request: InteractionRequest }`
   - Used for out-of-band delivery through channels

3. **`crucible-daemon-client/src/agent.rs`**:
   - Refactored `DaemonAgentHandle::new()` to spawn background event router task
   - Event router (`event_router()`) splits incoming events:
     - `interaction_requested` → parsed to `InteractionEvent` → sent to `interaction_tx`
     - All other events → forwarded to `streaming_tx`
   - `take_interaction_receiver()` returns the interaction channel (once)
   - `send_message_stream()` now uses `streaming_rx` instead of `event_rx`

4. **`crucible-cli/src/tui/oil/chat_runner.rs`**:
   - `run_with_factory()` calls `agent.take_interaction_receiver()` before event loop
   - `event_loop()` now takes optional `interaction_rx` parameter
   - Added new branch in `tokio::select!`:
     ```rust
     Some(interaction_event) = async { ... } => {
         let session_event = SessionEvent::InteractionRequested { ... };
         if let Some(msg) = Self::handle_session_event(session_event) {
             let _ = app.on_message(msg);
         }
     }
     ```

### Architecture After Fix

```
Daemon → event_rx → event_router task → interaction_tx → TUI event loop → OpenInteraction
                                      ↘ streaming_tx → send_message_stream() → ChatChunk
```

### Key Patterns

**Event Router** (background task):
- Runs continuously while handle exists
- Filters events by session_id
- Deserializes `interaction_requested` to `InteractionEvent`
- Non-interaction events pass through to streaming channel

**Interaction Channel Extraction**:
- `take_interaction_receiver()` returns `Option<T>` (once)
- Caller must store the receiver for the lifetime of the event loop
- Subsequent calls return `None`

### Testing

- All 2431 tests pass
- No test failures introduced
- Integration tests verify event routing works

### Manual QA Ready

The TUI should now receive interaction events. Test with:

1. Start daemon: `cargo run -p crucible-daemon --bin cru-server`
2. Start TUI: `RUST_LOG=crucible_cli=info cargo run --bin cru -- chat`
3. In another terminal, get session ID and send test interaction:
   ```bash
   # Get session ID from TUI logs or session.list RPC
   echo '{"jsonrpc":"2.0","id":1,"method":"session.test_interaction","params":{"session_id":"<SESSION_ID>","type":"ask"}}' | nc -U /run/user/1000/crucible.sock
   ```
4. Verify:
   - TUI logs show "Received interaction event"
   - Modal appears with Ask question
   - Keyboard navigation works (Up/Down/Enter/Esc)
   - Response sends back to daemon
