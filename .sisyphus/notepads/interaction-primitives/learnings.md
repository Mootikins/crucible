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
