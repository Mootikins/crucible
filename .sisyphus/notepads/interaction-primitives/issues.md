# Issues & Gotchas - Interaction Primitives

## [2026-01-25] Task 3 Incomplete

**Issue**: `render_ask_interaction()` exists but key handling doesn't work

**Location**: `chat_app.rs:1865-1918` (`handle_interaction_key()`)

**Problem**: Early return for non-Permission requests:
```rust
let perm_request = match &modal.request {
    InteractionRequest::Permission(req) => req,
    _ => return Action::Continue,  // ← Ask requests get ignored!
};
```

**Fix Required**: Refactor to dispatch by type (see decisions.md)

## [2026-01-25] Task 5 Blocked

**Blocker**: No RPC method exists for sending interaction responses

**Missing**:
- `DaemonClient::session_interaction_respond(session_id, request_id, response)`
- RPC handler `session.interaction_respond` in daemon server
- Wire `ChatAppMsg::CloseInteraction` to call RPC

**Reference Pattern**: `session_send_message` in `daemon-client/src/client.rs:892-904`
